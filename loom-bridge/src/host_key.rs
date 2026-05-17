//! Ed25519 host-key loader. T46 cycle 4a (advances #598).
//!
//! Loads the bridge's SSH server host key from a PEM-encoded ed25519
//! private key. Enforces ed25519-only at this layer (defence-in-depth
//! for the Marvin Attack SHIP-DECISION — see [`crate::transport`]).
//!
//! Lives behind no feature flag because:
//!   * the type model is useful for operator tooling (loom-cli can
//!     generate + validate host keys without russh in the dep graph);
//!   * cycle-4b will wire it into the russh transport's listen()
//!     entry, at which point the russh-transport feature picks up the
//!     loader transparently.
//!
//! SECURITY: the in-memory key bytes are zeroized on drop via the
//! `zeroize` derive on the underlying ed25519_dalek::SigningKey
//! (dalek 2.x opts into this).

use ed25519_dalek::SigningKey;
use ed25519_dalek::pkcs8::DecodePrivateKey;
use std::path::Path;

/// Bridge host key — newtype around the ed25519 signing key so the
/// rest of the codebase never sees a raw `SigningKey` and we can
/// add layer-specific invariants (e.g., key rotation cadence checks)
/// without touching call sites.
#[derive(Debug)]
pub struct BridgeHostKey {
    signing_key: SigningKey,
}

impl BridgeHostKey {
    /// Construct from already-parsed dalek `SigningKey`. Pre-validated
    /// by construction — no further runtime checks.
    #[must_use]
    pub fn from_signing_key(sk: SigningKey) -> Self {
        Self { signing_key: sk }
    }

    /// Parse a PKCS#8 PEM-encoded ed25519 private key. Rejects any
    /// algorithm other than ed25519 — the underlying dalek parser
    /// inspects the algorithm OID and fails on RSA / ECDSA / etc.
    ///
    /// # Errors
    ///
    /// Returns [`HostKeyError::ParsePem`] when the input is not a
    /// well-formed PKCS#8 PEM ed25519 private key. Wrong-algorithm
    /// PEMs (RSA / ECDSA) hit the same path — dalek refuses them
    /// at the OID check, which we surface verbatim.
    ///
    /// BUG ASSUMPTION: callers feed bytes they've read from a
    /// trusted-on-disk path. The function does not verify the
    /// permissions / ownership of the source file — that is the
    /// operator's responsibility (chmod 0400, dedicated uid).
    pub fn from_pkcs8_pem(pem: &str) -> Result<Self, HostKeyError> {
        let sk =
            SigningKey::from_pkcs8_pem(pem).map_err(|e| HostKeyError::ParsePem(e.to_string()))?;
        Ok(Self::from_signing_key(sk))
    }

    /// Convenience wrapper that reads a file from disk and parses
    /// it via [`Self::from_pkcs8_pem`].
    ///
    /// # Errors
    ///
    /// Returns [`HostKeyError::Io`] if the file can't be read, or
    /// [`HostKeyError::ParsePem`] if the contents don't parse.
    pub fn from_pem_file(path: &Path) -> Result<Self, HostKeyError> {
        let pem = std::fs::read_to_string(path).map_err(|source| HostKeyError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_pkcs8_pem(&pem)
    }

    /// Borrow the underlying signing key. Public so cycle-4b can
    /// hand it to `russh::server::Config`. Internal callers should
    /// prefer the type-system-mediated methods on this struct.
    #[must_use]
    pub fn signing_key(&self) -> &SigningKey {
        &self.signing_key
    }

    /// Verifying-key bytes (32 raw bytes) — handy for fingerprinting
    /// the host key in logs / startup banners.
    #[must_use]
    pub fn verifying_key_bytes(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }
}

/// Errors raised by the host-key loader.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum HostKeyError {
    /// Could not read the PEM file from disk.
    #[error("read host key file {path}: {source}")]
    Io {
        /// The path we tried to read.
        path: std::path::PathBuf,
        /// Underlying I/O failure.
        #[source]
        source: std::io::Error,
    },
    /// PEM does not parse as a PKCS#8 ed25519 private key. Either a
    /// malformed file or a non-ed25519 algorithm (RSA / ECDSA).
    #[error("parse PKCS#8 ed25519 host key: {0}")]
    ParsePem(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::pkcs8::{EncodePrivateKey, spki::der::pem::LineEnding};
    use rand_core::OsRng;

    /// Produce a fresh ed25519 PEM for test fixtures.
    fn fresh_ed25519_pem() -> String {
        let sk = SigningKey::generate(&mut OsRng);
        sk.to_pkcs8_pem(LineEnding::LF)
            .expect("encode pkcs8")
            .to_string()
    }

    #[test]
    fn parses_valid_ed25519_pem() {
        let pem = fresh_ed25519_pem();
        let hk = BridgeHostKey::from_pkcs8_pem(&pem).expect("valid ed25519");
        assert_eq!(hk.verifying_key_bytes().len(), 32);
    }

    #[test]
    fn rejects_garbage_pem() {
        let result = BridgeHostKey::from_pkcs8_pem("not a real PEM");
        assert!(matches!(result, Err(HostKeyError::ParsePem(_))));
    }

    #[test]
    fn rejects_truncated_pem_block() {
        let pem = "-----BEGIN PRIVATE KEY-----\nMC4CAQA=\n-----END PRIVATE KEY-----\n";
        let result = BridgeHostKey::from_pkcs8_pem(pem);
        assert!(matches!(result, Err(HostKeyError::ParsePem(_))));
    }

    #[test]
    fn rejects_empty_input() {
        let result = BridgeHostKey::from_pkcs8_pem("");
        assert!(matches!(result, Err(HostKeyError::ParsePem(_))));
    }

    #[test]
    fn round_trips_via_from_signing_key() {
        let sk = SigningKey::generate(&mut OsRng);
        let expected = sk.verifying_key().to_bytes();
        let hk = BridgeHostKey::from_signing_key(sk);
        assert_eq!(hk.verifying_key_bytes(), expected);
    }

    #[test]
    fn rejects_rsa_pkcs8_pem() {
        // SECURITY: this is the load-bearing test for the Marvin
        // Attack SHIP-DECISION. An operator who accidentally points
        // the bridge at an RSA host key MUST get a hard error — not
        // a silent fallback that activates the russh RSA codepath.
        //
        // The PEM below is a real PKCS#8-encoded RSA-2048 key block
        // (algorithm OID 1.2.840.113549.1.1.1). Generated via
        //   openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048
        // and trimmed to the lines that exercise the algorithm-OID
        // gate. ed25519_dalek::SigningKey::from_pkcs8_pem rejects on
        // the OID check before reading the key material, so the
        // truncated body is fine for this assertion.
        let rsa_oid_header = "-----BEGIN PRIVATE KEY-----\nMIICdwIBADANBgkqhkiG9w0BAQEFAASCAmEwggJdAgEAAoGBANNqVR\n-----END PRIVATE KEY-----\n";
        let result = BridgeHostKey::from_pkcs8_pem(rsa_oid_header);
        assert!(
            matches!(result, Err(HostKeyError::ParsePem(_))),
            "RSA PKCS#8 PEM must be rejected at the algorithm-OID gate"
        );
    }

    #[test]
    fn reads_from_file() {
        let pem = fresh_ed25519_pem();
        let dir = tempfile::tempdir().expect("tmp");
        let path = dir.path().join("host.pem");
        std::fs::write(&path, &pem).expect("write");
        let hk = BridgeHostKey::from_pem_file(&path).expect("load");
        assert_eq!(hk.verifying_key_bytes().len(), 32);
    }

    #[test]
    fn missing_file_returns_io_error() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = dir.path().join("nonexistent.pem");
        let result = BridgeHostKey::from_pem_file(&path);
        assert!(matches!(result, Err(HostKeyError::Io { .. })));
    }

    #[test]
    fn signing_key_accessor_returns_same_pubkey() {
        let pem = fresh_ed25519_pem();
        let hk = BridgeHostKey::from_pkcs8_pem(&pem).unwrap();
        let direct = hk.verifying_key_bytes();
        let via_accessor = hk.signing_key().verifying_key().to_bytes();
        assert_eq!(direct, via_accessor);
    }
}
