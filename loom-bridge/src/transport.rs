//! Russh transport — feature-gated SSH server scaffold.
//!
//! T46 cycle 3 (advances #598). Behind the `russh-transport`
//! feature so the default workspace build stays tokio-free.
//!
//! Cycle 2 landed the typed tenant + resource-ceiling model.
//! Cycle 3 (this commit) lands:
//!   * [`BridgeServerConfig`] — what the server needs to run
//!   * [`BridgeHandler`] — `russh::server::Handler` impl that
//!     enforces ed25519-ONLY publickey auth and looks the offered
//!     key up in the [`crate::tenant::TenantRegistry`]
//!   * [`BridgeServer`] — `russh::server::Server` impl that mints
//!     a fresh handler per inbound connection
//!   * [`AuthOutcome`] — typed audit record of every auth attempt
//!   * [`pubkey_to_base64_no_pad`] — helper translating a russh
//!     `PublicKey` to the wire-shape the tenant registry stores
//!
//! Cycle 4 will land: ed25519 host-key loading + validation, the
//! `BridgeServer::listen` async entry point, and channel-open →
//! `claude --resume` exec under the resolved tenant's uid.
//!
//! ## Defence-in-depth note (Marvin Attack / RUSTSEC-2023-0071)
//!
//! `russh-keys` 0.45 ships with the `rsa` crate as an unconditional
//! transitive dep; we've accepted the residual risk in deny.toml +
//! `.cargo/audit.toml` *on the basis that no RSA codepath is ever
//! exercised at runtime*. The ed25519-only enforcement here is the
//! technical bind that backs that SHIP-DECISION: any non-ed25519
//! key offered for auth is rejected at the russh `Handler` layer
//! BEFORE russh attempts any RSA verify. The check belongs here,
//! not in deny.toml, because a future cycle that loosens key
//! requirements must FIRST be reviewed against the timing-attack
//! threat model.

#![cfg(feature = "russh-transport")]

use crate::tenant::{TenantId, TenantRegistry};
use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD_NO_PAD;
use std::net::SocketAddr;
use std::sync::Arc;

/// Configuration the bridge needs to bring an SSH server up.
///
/// Fields kept on the public surface (vs hidden behind a builder)
/// because every one of them is required and there is no sensible
/// default the loom-cli could supply without the operator's input.
///
/// BUG ASSUMPTION: `registry` is a fully-populated snapshot; mid-flight
/// mutations require an `Arc<RwLock<TenantRegistry>>` swap, which is
/// cycle-4 scope.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct BridgeServerConfig {
    /// Tenants authorised to connect, keyed by id. Cloned into each
    /// per-connection handler via `Arc` so updates aren't visible
    /// mid-session (deliberate — admin portal must restart to swap
    /// the registry, no auth ambiguity).
    pub registry: Arc<TenantRegistry>,
    /// Address the server should bind.
    pub listen_addr: SocketAddr,
}

impl BridgeServerConfig {
    /// Construct a config from its components.
    ///
    /// BUG ASSUMPTION: caller has already validated `listen_addr`
    /// against any deployment-level allowlist (only-bind-loopback,
    /// only-bind-VPN-range, etc.). The bridge intentionally does
    /// not second-guess the socket choice.
    #[must_use]
    pub fn new(registry: Arc<TenantRegistry>, listen_addr: SocketAddr) -> Self {
        Self {
            registry,
            listen_addr,
        }
    }
}

/// Bridge-layer errors. Distinct from `russh::Error` so the loom-cli
/// can match on bridge-specific conditions without depending on
/// russh's types.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BridgeError {
    /// Russh internal error during session / connection setup.
    #[error("russh error: {0}")]
    Russh(String),
    /// Could not bind the configured listen address.
    #[error("bind failed on {addr}: {source}")]
    Bind {
        /// The address we tried to bind.
        addr: SocketAddr,
        /// Underlying I/O failure.
        #[source]
        source: std::io::Error,
    },
}

/// Mapping from `russh::Error` is lossy by design — the bridge does
/// not surface russh internals to its callers; the string form is
/// enough for the loom-cli to log + the operator to grep.
impl From<russh::Error> for BridgeError {
    fn from(e: russh::Error) -> Self {
        Self::Russh(e.to_string())
    }
}

/// Outcome of one publickey auth attempt. Returned to the russh
/// layer indirectly (mapped to `russh::server::Auth`) and also fed
/// to the audit log so we keep a forensic trail of every attempt.
///
/// BUG ASSUMPTION: callers MUST log every outcome. Silent rejects
/// defeat the audit-trail purpose of having this type at all.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuthOutcome {
    /// Key is on the authorised list. Carries the resolved tenant
    /// so downstream stages (channel-open, exec) can pick it up
    /// from the handler state without re-doing the lookup.
    Accept(TenantId),
    /// Auth attempt rejected. `reason` is a stable string the
    /// audit log can group by (e.g., `"non-ed25519-key"`).
    Reject {
        /// Why we rejected (machine-grepable).
        reason: &'static str,
    },
}

/// Convert a russh `PublicKey` to the no-padding base64 wire form
/// the `TenantRegistry` stores its authorised keys as.
///
/// Returns `None` for any non-ed25519 key — the bridge refuses to
/// reason about RSA / ECDSA keys at all (see Marvin Attack note).
///
/// BUG ASSUMPTION: the format is stable across russh-keys 0.45.x.
/// A breaking change in the upstream `ed25519_dalek::VerifyingKey`
/// byte layout would silently invalidate every entry in the on-disk
/// registry. The `pubkey_to_b64_ed25519_round_trips` test below is
/// the canary.
#[must_use]
pub fn pubkey_to_base64_no_pad(pk: &russh::keys::key::PublicKey) -> Option<String> {
    use russh::keys::key::PublicKey;
    match pk {
        PublicKey::Ed25519(vk) => Some(STANDARD_NO_PAD.encode(vk.as_bytes())),
        // Defence in depth: do not even ATTEMPT to b64 anything else.
        // The auth layer must short-circuit before it gets here.
        _ => None,
    }
}

/// Per-connection handler. One `BridgeHandler` per inbound client.
/// Owns an `Arc` to the immutable-for-this-session registry snapshot
/// + remembers which tenant authenticated so cycle-4's channel-open
/// can read it without a second registry walk.
#[derive(Debug)]
pub struct BridgeHandler {
    registry: Arc<TenantRegistry>,
    /// `None` until publickey auth succeeds.
    authenticated_as: Option<TenantId>,
}

impl BridgeHandler {
    /// Construct a fresh handler for one client.
    #[must_use]
    fn new(registry: Arc<TenantRegistry>) -> Self {
        Self {
            registry,
            authenticated_as: None,
        }
    }

    /// Classify an offered key against the registry. Public so unit
    /// tests can exercise it without standing a real russh session.
    ///
    /// BUG ASSUMPTION: the registry is queried as-is; no
    /// retry-on-Arc-update path. Cycle-4 swap behaviour requires
    /// caller-side rebind.
    #[must_use]
    pub fn classify(&self, key: &russh::keys::key::PublicKey) -> AuthOutcome {
        let Some(b64) = pubkey_to_base64_no_pad(key) else {
            return AuthOutcome::Reject {
                reason: "non-ed25519-key",
            };
        };
        match self.registry.tenant_for_key(&b64) {
            Some(t) => AuthOutcome::Accept(t.id.clone()),
            None => AuthOutcome::Reject {
                reason: "unknown-key",
            },
        }
    }

    /// Once cycle 4 lands channel-open + exec, this is what the
    /// downstream stages read to know whose uid to switch to. Public
    /// so callers can assert post-auth invariants in tests.
    #[must_use]
    pub fn tenant(&self) -> Option<&TenantId> {
        self.authenticated_as.as_ref()
    }
}

#[async_trait]
impl russh::server::Handler for BridgeHandler {
    type Error = BridgeError;

    async fn auth_publickey(
        &mut self,
        _user: &str,
        public_key: &russh::keys::key::PublicKey,
    ) -> Result<russh::server::Auth, Self::Error> {
        // SECURITY: enforce ed25519-only at this exact line. Russh
        // would otherwise call into the RSA verify path before we
        // ever see the key — but `classify` returns Reject for
        // non-ed25519 BEFORE any algorithm-specific work runs in
        // our handler. The deny.toml RUSTSEC-2023-0071 ignore is
        // load-bearing on this gate.
        let outcome = self.classify(public_key);
        match outcome {
            AuthOutcome::Accept(tenant) => {
                tracing::info!(tenant = %tenant, "ssh auth accept");
                self.authenticated_as = Some(tenant);
                Ok(russh::server::Auth::Accept)
            }
            AuthOutcome::Reject { reason } => {
                tracing::warn!(reason, "ssh auth reject");
                Ok(russh::server::Auth::Reject {
                    proceed_with_methods: None,
                })
            }
        }
    }
}

/// Top-level russh `Server`. Hands out fresh `BridgeHandler`s.
///
/// BUG ASSUMPTION: `new_client` runs synchronously on the russh
/// accept loop, so it must stay allocation-cheap. Wrapping the
/// registry in `Arc` is the whole point — clone is O(1).
#[derive(Debug, Clone)]
pub struct BridgeServer {
    registry: Arc<TenantRegistry>,
}

impl BridgeServer {
    /// Construct from a config. The listen_addr is held for
    /// cycle-4's listen() impl; cycle 3 only exercises the trait.
    #[must_use]
    pub fn new(config: BridgeServerConfig) -> Self {
        Self {
            registry: config.registry,
        }
    }
}

#[async_trait]
impl russh::server::Server for BridgeServer {
    type Handler = BridgeHandler;

    fn new_client(&mut self, _peer_addr: Option<SocketAddr>) -> Self::Handler {
        BridgeHandler::new(Arc::clone(&self.registry))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tenant::{Tenant, TenantKey};
    use ed25519_dalek::SigningKey;
    use rand_core::OsRng;
    use russh::server::Server as _;
    use std::net::{IpAddr, Ipv4Addr};
    use subtle::ConstantTimeEq;

    fn fresh_ed25519() -> (
        ed25519_dalek::SigningKey,
        ed25519_dalek::VerifyingKey,
        String,
    ) {
        let sk = SigningKey::generate(&mut OsRng);
        let vk = sk.verifying_key();
        let b64 = STANDARD_NO_PAD.encode(vk.as_bytes());
        (sk, vk, b64)
    }

    fn registry_with(tenant: &str, key_b64: &str) -> Arc<TenantRegistry> {
        let mut r = TenantRegistry::empty();
        let id = TenantId::new(tenant).unwrap();
        let mut t = Tenant::new(id);
        t.keys.push(TenantKey {
            b64: key_b64.to_owned(),
            label: None,
        });
        r.upsert(t);
        Arc::new(r)
    }

    #[test]
    fn config_constructs() {
        let r = Arc::new(TenantRegistry::empty());
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let cfg = BridgeServerConfig::new(r, addr);
        assert_eq!(cfg.listen_addr.port(), 0);
    }

    #[test]
    fn pubkey_to_b64_ed25519_round_trips() {
        let (_sk, vk, b64_direct) = fresh_ed25519();
        let pk = russh::keys::key::PublicKey::Ed25519(vk);
        let b64_via = pubkey_to_base64_no_pad(&pk).expect("ed25519 → b64");
        assert_eq!(b64_via, b64_direct);
    }

    #[test]
    fn classify_accepts_known_tenant() {
        let (_sk, vk, b64) = fresh_ed25519();
        let r = registry_with("acme", &b64);
        let h = BridgeHandler::new(r);
        let pk = russh::keys::key::PublicKey::Ed25519(vk);
        match h.classify(&pk) {
            AuthOutcome::Accept(id) => assert_eq!(id.as_str(), "acme"),
            AuthOutcome::Reject { reason } => panic!("expected accept, got reject: {reason}"),
        }
    }

    #[test]
    fn classify_rejects_unknown_key() {
        let (_sk_a, _vk_a, b64_a) = fresh_ed25519();
        let (_sk_b, vk_b, _b64_b) = fresh_ed25519();
        let r = registry_with("acme", &b64_a);
        let h = BridgeHandler::new(r);
        let pk = russh::keys::key::PublicKey::Ed25519(vk_b);
        assert_eq!(
            h.classify(&pk),
            AuthOutcome::Reject {
                reason: "unknown-key"
            }
        );
    }

    #[test]
    fn classify_empty_registry_rejects() {
        let (_sk, vk, _b64) = fresh_ed25519();
        let r = Arc::new(TenantRegistry::empty());
        let h = BridgeHandler::new(r);
        let pk = russh::keys::key::PublicKey::Ed25519(vk);
        assert_eq!(
            h.classify(&pk),
            AuthOutcome::Reject {
                reason: "unknown-key"
            }
        );
    }

    #[test]
    fn handler_tenant_unset_until_auth() {
        let (_sk, _vk, b64) = fresh_ed25519();
        let r = registry_with("acme", &b64);
        let h = BridgeHandler::new(r);
        assert!(h.tenant().is_none());
    }

    #[test]
    fn server_mints_fresh_handler_per_client() {
        let r = Arc::new(TenantRegistry::empty());
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let mut s = BridgeServer::new(BridgeServerConfig::new(r, addr));
        let h1 = s.new_client(None);
        let h2 = s.new_client(None);
        // Each handler is independent — auth state on one doesn't
        // bleed to the other.
        assert!(h1.tenant().is_none());
        assert!(h2.tenant().is_none());
    }

    #[test]
    fn auth_outcome_eq_is_value_based() {
        let id = TenantId::new("acme").unwrap();
        assert_eq!(AuthOutcome::Accept(id.clone()), AuthOutcome::Accept(id));
        assert_ne!(
            AuthOutcome::Reject {
                reason: "unknown-key"
            },
            AuthOutcome::Reject {
                reason: "non-ed25519-key"
            }
        );
    }

    #[test]
    fn subtle_constant_time_eq_for_key_bytes() {
        // SUPERSOCIETY: backstops the registry's plain-eq lookup in
        // case a future refactor swaps in raw byte compare on a key
        // material slice. subtle::ConstantTimeEq is the right
        // primitive for any post-Marvin equality check on auth-
        // adjacent bytes.
        let (_sk, vk, _b64) = fresh_ed25519();
        let bytes = vk.as_bytes();
        assert!(bool::from(bytes.ct_eq(bytes)));
        let mut tampered = *bytes;
        tampered[0] ^= 1;
        assert!(!bool::from(bytes.ct_eq(&tampered)));
    }

    #[test]
    fn bridge_error_from_russh_preserves_message() {
        let e: BridgeError = russh::Error::Disconnect.into();
        let msg = e.to_string();
        assert!(msg.contains("russh error"));
    }
}
