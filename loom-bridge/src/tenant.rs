//! Tenant identity + ed25519 key registry.
//!
//! Tenants are the unit of isolation in the SSH bridge: one tenant =
//! one unix uid + one cgroup + one chroot/bwrap mount namespace + one
//! `claude --resume <session-id>` invocation. Tenants own one or more
//! ed25519 public keys; an incoming SSH connection presenting a key
//! that matches `(tenant_id, fingerprint)` is authenticated to that
//! tenant.
//!
//! The registry is in-memory + persisted via [`TenantRegistry::load`]
//! / [`save`](TenantRegistry::save). On-disk format is plain JSON so
//! `loom-cli` can manage it without the daemon being live.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Newtype around the tenant slug. Validated on construction.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TenantId(String);

impl TenantId {
    /// Construct from an arbitrary string. Tenant ids must be 1–64
    /// chars of `[a-z0-9-]` (DNS-label-ish) so they're safe to use
    /// as unix usernames, cgroup names, and mount-namespace tags
    /// without escaping.
    ///
    /// # Errors
    ///
    /// Returns [`TenantError::InvalidId`] when the string is empty,
    /// longer than 64 characters, or contains characters outside the
    /// allowed set.
    pub fn new(s: impl Into<String>) -> Result<Self, TenantError> {
        let s: String = s.into();
        if s.is_empty() || s.len() > 64 {
            return Err(TenantError::InvalidId(s));
        }
        if !s
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(TenantError::InvalidId(s));
        }
        // Reject leading or trailing hyphens (DNS-label rule).
        if s.starts_with('-') || s.ends_with('-') {
            return Err(TenantError::InvalidId(s));
        }
        Ok(Self(s))
    }

    /// Borrow the underlying slug.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TenantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One authorized ed25519 public key, attached to a tenant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TenantKey {
    /// Base64 (no padding) ed25519 public key bytes. 43 ASCII chars
    /// when the underlying 32-byte key is encoded.
    pub b64: String,
    /// Optional human label so the operator can recognize the key in
    /// audit logs without comparing fingerprints.
    pub label: Option<String>,
}

/// One tenant — the unit of isolation in the SSH bridge.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Tenant {
    /// Unique slug. Doubles as the unix username and cgroup name.
    pub id: TenantId,
    /// Authorized SSH public keys for this tenant.
    pub keys: Vec<TenantKey>,
}

impl Tenant {
    /// Construct a tenant with no keys.
    #[must_use]
    pub fn new(id: TenantId) -> Self {
        Self {
            id,
            keys: Vec::new(),
        }
    }

    /// True iff the given base64-encoded ed25519 public key is on
    /// this tenant's authorized list.
    #[must_use]
    pub fn authorizes(&self, b64: &str) -> bool {
        self.keys.iter().any(|k| k.b64 == b64)
    }
}

/// All tenants known to the bridge.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TenantRegistry {
    /// Tenants keyed by id. BTreeMap so JSON-serialized output is
    /// deterministic for diff-friendly storage.
    pub tenants: BTreeMap<TenantId, Tenant>,
}

impl TenantRegistry {
    /// Empty registry. New deployments start here.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Insert / replace a tenant.
    pub fn upsert(&mut self, tenant: Tenant) {
        self.tenants.insert(tenant.id.clone(), tenant);
    }

    /// Lookup by id.
    #[must_use]
    pub fn get(&self, id: &TenantId) -> Option<&Tenant> {
        self.tenants.get(id)
    }

    /// Resolve a connecting SSH key to its owning tenant. `None` if
    /// no tenant authorizes the key.
    #[must_use]
    pub fn tenant_for_key(&self, b64: &str) -> Option<&Tenant> {
        self.tenants.values().find(|t| t.authorizes(b64))
    }
}

/// Errors at the tenant layer.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TenantError {
    /// The id failed validation (length / charset / DNS-label rules).
    #[error("invalid tenant id: {0:?}")]
    InvalidId(String),
    /// Lookup miss.
    #[error("unknown tenant: {0}")]
    UnknownTenant(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_accepts_dns_label() {
        assert!(TenantId::new("acme").is_ok());
        assert!(TenantId::new("acme-corp").is_ok());
        assert!(TenantId::new("a1b2c3").is_ok());
    }

    #[test]
    fn id_rejects_uppercase() {
        assert!(matches!(
            TenantId::new("Acme"),
            Err(TenantError::InvalidId(_))
        ));
    }

    #[test]
    fn id_rejects_empty_and_long() {
        assert!(matches!(TenantId::new(""), Err(TenantError::InvalidId(_))));
        let long = "a".repeat(65);
        assert!(matches!(
            TenantId::new(long),
            Err(TenantError::InvalidId(_))
        ));
    }

    #[test]
    fn id_rejects_leading_or_trailing_hyphen() {
        assert!(matches!(
            TenantId::new("-acme"),
            Err(TenantError::InvalidId(_))
        ));
        assert!(matches!(
            TenantId::new("acme-"),
            Err(TenantError::InvalidId(_))
        ));
    }

    #[test]
    fn id_rejects_underscore() {
        assert!(matches!(
            TenantId::new("acme_corp"),
            Err(TenantError::InvalidId(_))
        ));
    }

    #[test]
    fn tenant_authorizes_known_key() {
        let id = TenantId::new("acme").unwrap();
        let mut t = Tenant::new(id);
        t.keys.push(TenantKey {
            b64: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
            label: None,
        });
        assert!(t.authorizes("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"));
        assert!(!t.authorizes("different"));
    }

    #[test]
    fn registry_resolves_key_to_owning_tenant() {
        let mut r = TenantRegistry::empty();
        let acme = TenantId::new("acme").unwrap();
        let widgets = TenantId::new("widgets-co").unwrap();
        let mut t1 = Tenant::new(acme.clone());
        t1.keys.push(TenantKey {
            b64: "key-A".into(),
            label: None,
        });
        let mut t2 = Tenant::new(widgets.clone());
        t2.keys.push(TenantKey {
            b64: "key-B".into(),
            label: None,
        });
        r.upsert(t1);
        r.upsert(t2);

        assert_eq!(r.tenant_for_key("key-A").map(|t| &t.id), Some(&acme));
        assert_eq!(r.tenant_for_key("key-B").map(|t| &t.id), Some(&widgets));
        assert!(r.tenant_for_key("unknown").is_none());
    }

    #[test]
    fn registry_upsert_replaces_existing() {
        let mut r = TenantRegistry::empty();
        let id = TenantId::new("acme").unwrap();
        let mut a = Tenant::new(id.clone());
        a.keys.push(TenantKey {
            b64: "v1".into(),
            label: None,
        });
        r.upsert(a);
        let mut b = Tenant::new(id.clone());
        b.keys.push(TenantKey {
            b64: "v2".into(),
            label: None,
        });
        r.upsert(b);
        assert!(r.get(&id).unwrap().authorizes("v2"));
        assert!(!r.get(&id).unwrap().authorizes("v1"));
    }

    #[test]
    fn registry_serializes_to_stable_json() {
        let mut r = TenantRegistry::empty();
        for id in ["zebra", "acme"] {
            r.upsert(Tenant::new(TenantId::new(id).unwrap()));
        }
        let json = serde_json::to_string(&r).unwrap();
        // BTreeMap keys: alphabetical → acme appears before zebra
        let acme_idx = json.find("acme").unwrap();
        let zebra_idx = json.find("zebra").unwrap();
        assert!(acme_idx < zebra_idx);
    }

    #[test]
    fn tenant_id_display_round_trips_to_str() {
        let id = TenantId::new("acme-corp").unwrap();
        assert_eq!(format!("{id}"), "acme-corp");
        assert_eq!(id.as_str(), "acme-corp");
    }
}
