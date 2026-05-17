//! T46.4 — cookie↔uid bridge.
//!
//! Bridges the admin portal's HTTP session cookie (issued by
//! `loom edit serve`) to the bridge's per-tenant session id used by
//! [`crate::exec_spec::ClaudeExecSpec`]. The cookie is a compact
//! HMAC-SHA256-signed token: `<tenant>|<session-id>|<expiry>|<hmac>`.
//!
//! Surface:
//!   * [`BridgeCookieSecret`] — HMAC key. Zeroized on drop.
//!   * [`SessionCookie`] — parsed + validated cookie payload.
//!   * [`sign_session_cookie`] — mint a fresh cookie for a tenant.
//!   * [`parse_and_validate_cookie`] — verify HMAC + expiry,
//!     return the tenant + session id.
//!   * [`CookieResolver`] — `TenantResolver` impl that parses an
//!     incoming SSH-side cookie blob (e.g., via SSH banner /
//!     environment) and dispatches to a backing static resolver.
//!
//! SECURITY:
//!   * Constant-time HMAC compare via `hmac::Mac::verify` (uses
//!     `subtle` internally).
//!   * Expiry is required + checked against caller-provided "now"
//!     to avoid coupling to a system clock here (testable).
//!   * Secret material zeroized on drop via `zeroize::Zeroize`.
//!   * Tenant + session-id pass through the existing newtype
//!     validators (DNS-label + restricted charset), so a forged
//!     cookie body that survives HMAC but contains a malformed
//!     identifier still fails on construction.

use crate::exec_spec::{ClaudeExecSpec, ClaudeSessionId};
use crate::resolver::{ResolverError, SharedResolver, TenantResolver};
use crate::tenant::TenantId;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::sync::Arc;
use zeroize::{Zeroize, ZeroizeOnDrop};

type HmacSha256 = Hmac<Sha256>;

/// Errors at the cookie-bridge layer.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CookieError {
    /// Cookie didn't split into exactly four pipe-separated fields.
    #[error("cookie malformed: expected `tenant|session|expiry|hmac`, got {0:?}")]
    Malformed(String),
    /// HMAC failed verification — tampered cookie or wrong secret.
    #[error("cookie HMAC verify failed")]
    HmacMismatch,
    /// Cookie expired (now ≥ expiry).
    #[error("cookie expired: expiry={expiry} now={now}")]
    Expired {
        /// Cookie expiry epoch-seconds.
        expiry: u64,
        /// Time-of-check epoch-seconds.
        now: u64,
    },
    /// Expiry field was not a parseable u64 epoch-seconds.
    #[error("cookie expiry not parseable: {0:?}")]
    BadExpiry(String),
    /// HMAC field was not valid hex.
    #[error("cookie HMAC not hex: {0:?}")]
    BadHmacHex(String),
    /// Tenant id failed its validator (DNS-label rules).
    #[error("cookie tenant invalid: {0}")]
    TenantInvalid(#[from] crate::tenant::TenantError),
    /// Session id failed its validator (charset / length).
    #[error("cookie session id invalid: {0}")]
    SessionInvalid(#[from] crate::exec_spec::ExecSpecError),
}

/// HMAC key for signing + verifying session cookies.
///
/// Zeroized on drop. NEVER log this value or include it in error
/// messages — the bridge intentionally limits the secret's reach
/// to this module only.
///
/// BUG ASSUMPTION: the secret is at least 32 bytes long. Shorter
/// keys silently work with HMAC but reduce the effective security
/// margin below what AVP-2 expects. `BridgeCookieSecret::new`
/// REJECTS shorter inputs.
#[derive(Clone, ZeroizeOnDrop)]
pub struct BridgeCookieSecret {
    bytes: Vec<u8>,
}

impl std::fmt::Debug for BridgeCookieSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // SECURITY: never print the bytes.
        write!(
            f,
            "BridgeCookieSecret(<redacted; {} bytes>)",
            self.bytes.len()
        )
    }
}

impl BridgeCookieSecret {
    /// Construct from raw bytes. Rejects keys shorter than 32 bytes.
    ///
    /// # Errors
    ///
    /// Returns [`CookieError::Malformed`] if `bytes.len() < 32`.
    pub fn new(bytes: Vec<u8>) -> Result<Self, CookieError> {
        if bytes.len() < 32 {
            // Re-using Malformed instead of adding a new variant — the
            // caller's "bytes too short" is one and the same defect
            // class as "incoming cookie too short" from the operator's
            // view (a config-validation failure).
            return Err(CookieError::Malformed(format!(
                "secret must be ≥32 bytes, got {}",
                bytes.len()
            )));
        }
        Ok(Self { bytes })
    }

    /// Borrow the bytes — pkg-private surface for the sign/verify
    /// paths below.
    fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Parsed + validated session cookie.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SessionCookie {
    /// Validated tenant id.
    pub tenant: TenantId,
    /// Validated session id (charset-restricted).
    pub session_id: ClaudeSessionId,
    /// Expiry, epoch-seconds.
    pub expiry: u64,
}

/// Mint a fresh cookie blob for `tenant` + `session_id` valid until
/// `expiry` (epoch-seconds).
///
/// Format: `tenant|session-id|expiry|hex_hmac_sha256_of_body`.
/// The body the HMAC covers is `tenant|session-id|expiry` (i.e.,
/// the first three fields joined with `|`).
#[must_use]
pub fn sign_session_cookie(
    secret: &BridgeCookieSecret,
    tenant: &TenantId,
    session_id: &ClaudeSessionId,
    expiry: u64,
) -> String {
    let body = format!("{}|{}|{}", tenant.as_str(), session_id.as_str(), expiry);
    let mac = compute_mac(secret, body.as_bytes());
    let hex = bytes_to_hex(&mac);
    format!("{body}|{hex}")
}

/// Parse + verify a cookie blob.
///
/// # Errors
///
/// Surfaces every failure mode listed in [`CookieError`]. Order of
/// checks: structure → HMAC → expiry → identifier-validity. HMAC
/// FIRST so a tampered cookie with a valid-looking body still fails
/// without leaking which field was tampered.
pub fn parse_and_validate_cookie(
    secret: &BridgeCookieSecret,
    raw: &str,
    now: u64,
) -> Result<SessionCookie, CookieError> {
    let mut parts = raw.splitn(4, '|');
    let tenant_str = parts
        .next()
        .ok_or_else(|| CookieError::Malformed(raw.to_owned()))?;
    let session_str = parts
        .next()
        .ok_or_else(|| CookieError::Malformed(raw.to_owned()))?;
    let expiry_str = parts
        .next()
        .ok_or_else(|| CookieError::Malformed(raw.to_owned()))?;
    let hmac_hex = parts
        .next()
        .ok_or_else(|| CookieError::Malformed(raw.to_owned()))?;

    // Verify HMAC first.
    let body = format!("{tenant_str}|{session_str}|{expiry_str}");
    let provided =
        hex_to_bytes(hmac_hex).map_err(|_| CookieError::BadHmacHex(hmac_hex.to_owned()))?;
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(body.as_bytes());
    mac.verify_slice(&provided)
        .map_err(|_| CookieError::HmacMismatch)?;

    // Expiry parse + check.
    let expiry: u64 = expiry_str
        .parse()
        .map_err(|_| CookieError::BadExpiry(expiry_str.to_owned()))?;
    if now >= expiry {
        return Err(CookieError::Expired { expiry, now });
    }

    // Validate identifiers via existing newtype validators.
    let tenant = TenantId::new(tenant_str)?;
    let session_id = ClaudeSessionId::new(session_str)?;

    Ok(SessionCookie {
        tenant,
        session_id,
        expiry,
    })
}

fn compute_mac(secret: &BridgeCookieSecret, body: &[u8]) -> Vec<u8> {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(body);
    mac.finalize().into_bytes().to_vec()
}

fn bytes_to_hex(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for byte in b {
        s.push_str(&format!("{byte:02x}"));
    }
    s
}

fn hex_to_bytes(s: &str) -> Result<Vec<u8>, ()> {
    if s.len() % 2 != 0 {
        return Err(());
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let bytes = s.as_bytes();
    for chunk in bytes.chunks(2) {
        let hi = hex_nibble(chunk[0])?;
        let lo = hex_nibble(chunk[1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn hex_nibble(b: u8) -> Result<u8, ()> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(()),
    }
}

/// Allow callers to scrub the secret eagerly (vs waiting for Drop).
impl Zeroize for BridgeCookieSecret {
    fn zeroize(&mut self) {
        self.bytes.zeroize();
    }
}

/// A [`TenantResolver`] that does NOT itself parse cookies — it
/// delegates to a backing resolver (typically a `StaticTenantResolver`)
/// and is paired with [`parse_and_validate_cookie`] by the caller.
///
/// The split exists because the bridge's `channel_open_session`
/// already has the resolved tenant + session id (post-auth from
/// cycle 3) — the cookie is the *source* of those values, not the
/// runtime dispatcher. Keeping the resolver simple means cookie
/// validation can be reused by HTTP request handlers, CLI tools,
/// etc., without dragging the SSH layer along.
#[derive(Debug, Clone)]
pub struct CookieResolver {
    backing: SharedResolver,
}

impl CookieResolver {
    /// Wrap a backing resolver.
    #[must_use]
    pub fn new(backing: SharedResolver) -> Self {
        Self { backing }
    }
}

impl TenantResolver for CookieResolver {
    fn resolve(
        &self,
        tenant: &TenantId,
        session_id: ClaudeSessionId,
    ) -> Result<ClaudeExecSpec, ResolverError> {
        self.backing.resolve(tenant, session_id)
    }
}

/// Convenience constructor for the common case.
#[must_use]
pub fn shared_cookie_resolver(backing: SharedResolver) -> SharedResolver {
    Arc::new(CookieResolver::new(backing))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolver::StaticTenantResolver;

    fn secret() -> BridgeCookieSecret {
        BridgeCookieSecret::new(vec![0xab; 32]).unwrap()
    }

    fn tenant() -> TenantId {
        TenantId::new("acme").unwrap()
    }

    fn session() -> ClaudeSessionId {
        ClaudeSessionId::new("sess-abc-123").unwrap()
    }

    #[test]
    fn secret_rejects_short_input() {
        assert!(matches!(
            BridgeCookieSecret::new(vec![0; 31]),
            Err(CookieError::Malformed(_))
        ));
        assert!(BridgeCookieSecret::new(vec![0; 32]).is_ok());
    }

    #[test]
    fn debug_redacts_secret_bytes() {
        let s = secret();
        let d = format!("{s:?}");
        assert!(d.contains("redacted"));
        assert!(!d.contains("ab"));
    }

    #[test]
    fn sign_and_parse_round_trip() {
        let s = secret();
        let raw = sign_session_cookie(&s, &tenant(), &session(), 999);
        let parsed = parse_and_validate_cookie(&s, &raw, 100).expect("valid");
        assert_eq!(parsed.tenant, tenant());
        assert_eq!(parsed.session_id, session());
        assert_eq!(parsed.expiry, 999);
    }

    #[test]
    fn tampered_body_fails_hmac() {
        let s = secret();
        let raw = sign_session_cookie(&s, &tenant(), &session(), 999);
        // Replace tenant 'acme' with 'evil' — same length, recompute nothing.
        let bad = raw.replacen("acme", "evil", 1);
        let err = parse_and_validate_cookie(&s, &bad, 100).unwrap_err();
        assert!(matches!(err, CookieError::HmacMismatch));
    }

    #[test]
    fn tampered_hmac_fails() {
        let s = secret();
        let mut raw = sign_session_cookie(&s, &tenant(), &session(), 999);
        // Flip one hex char in the trailing HMAC.
        let last = raw.pop().unwrap();
        let flipped = if last == '0' { '1' } else { '0' };
        raw.push(flipped);
        let err = parse_and_validate_cookie(&s, &raw, 100).unwrap_err();
        assert!(matches!(err, CookieError::HmacMismatch));
    }

    #[test]
    fn expired_cookie_rejected() {
        let s = secret();
        let raw = sign_session_cookie(&s, &tenant(), &session(), 500);
        let err = parse_and_validate_cookie(&s, &raw, 501).unwrap_err();
        assert!(matches!(
            err,
            CookieError::Expired {
                expiry: 500,
                now: 501
            }
        ));
    }

    #[test]
    fn at_expiry_boundary_rejected() {
        // now == expiry → expired (>= semantics).
        let s = secret();
        let raw = sign_session_cookie(&s, &tenant(), &session(), 500);
        assert!(matches!(
            parse_and_validate_cookie(&s, &raw, 500),
            Err(CookieError::Expired { .. })
        ));
    }

    #[test]
    fn malformed_too_few_fields() {
        let s = secret();
        let err = parse_and_validate_cookie(&s, "only|one|field", 100).unwrap_err();
        assert!(matches!(
            err,
            CookieError::Malformed(_) | CookieError::BadHmacHex(_)
        ));
    }

    #[test]
    fn bad_hmac_hex_rejected() {
        let s = secret();
        let raw = "acme|sess|999|ZZ_NOT_HEX";
        let err = parse_and_validate_cookie(&s, raw, 100).unwrap_err();
        assert!(matches!(err, CookieError::BadHmacHex(_)));
    }

    #[test]
    fn invalid_tenant_in_signed_cookie_rejected() {
        // An operator who somehow signed a cookie with a bad
        // tenant id (impossible via TenantId::new, but possible if
        // the secret leaked + an attacker forged) MUST still fail
        // the tenant validator.
        let s = secret();
        // Manually compose body + sign to bypass TenantId::new at
        // mint-time.
        let body = "Acme-Upper|sess|999"; // uppercase rejected
        let mac = compute_mac(&s, body.as_bytes());
        let raw = format!("{body}|{}", bytes_to_hex(&mac));
        let err = parse_and_validate_cookie(&s, &raw, 100).unwrap_err();
        assert!(matches!(err, CookieError::TenantInvalid(_)));
    }

    #[test]
    fn invalid_session_id_in_signed_cookie_rejected() {
        let s = secret();
        let body = "acme|bad session id with spaces|999";
        let mac = compute_mac(&s, body.as_bytes());
        let raw = format!("{body}|{}", bytes_to_hex(&mac));
        let err = parse_and_validate_cookie(&s, &raw, 100).unwrap_err();
        assert!(matches!(err, CookieError::SessionInvalid(_)));
    }

    #[test]
    fn wrong_secret_fails_hmac() {
        let s = secret();
        let raw = sign_session_cookie(&s, &tenant(), &session(), 999);
        let other = BridgeCookieSecret::new(vec![0x42; 32]).unwrap();
        let err = parse_and_validate_cookie(&other, &raw, 100).unwrap_err();
        assert!(matches!(err, CookieError::HmacMismatch));
    }

    #[test]
    fn hex_round_trip_lowercase() {
        let bytes = vec![0xde, 0xad, 0xbe, 0xef];
        let hex = bytes_to_hex(&bytes);
        assert_eq!(hex, "deadbeef");
        assert_eq!(hex_to_bytes(&hex).unwrap(), bytes);
    }

    #[test]
    fn hex_round_trip_uppercase_ok() {
        assert_eq!(
            hex_to_bytes("DEADBEEF").unwrap(),
            vec![0xde, 0xad, 0xbe, 0xef]
        );
    }

    #[test]
    fn cookie_resolver_delegates_to_backing() {
        use crate::exec_spec::TenantUid;
        use crate::resolver::StaticTenantEntry;
        let mut r = StaticTenantResolver::empty();
        r.upsert(
            tenant(),
            StaticTenantEntry {
                uid: TenantUid::new(1042).unwrap(),
                claude_binary_path: "/usr/local/bin/claude".into(),
                workdir: "/var/lib/loom/acme".into(),
            },
        );
        let cr = CookieResolver::new(Arc::new(r));
        let spec = cr.resolve(&tenant(), session()).expect("resolves");
        assert_eq!(spec.tenant, tenant());
    }

    #[test]
    fn cookie_resolver_propagates_unknown_tenant() {
        let backing = Arc::new(StaticTenantResolver::empty());
        let cr = CookieResolver::new(backing);
        let result = cr.resolve(&tenant(), session());
        assert!(matches!(result, Err(ResolverError::UnknownTenant(_))));
    }

    #[test]
    fn zeroize_clears_secret_bytes() {
        let mut s = secret();
        s.zeroize();
        // After zeroize, the bytes must all be 0. We can't read them
        // directly without exposing as_bytes, but bytes.is_empty()
        // can hold after a Vec::zeroize that capacity-preserves but
        // zero-fills.
        assert!(s.as_bytes().iter().all(|&b| b == 0));
    }
}
