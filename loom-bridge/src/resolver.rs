//! Tenant → ExecSpec resolver. T46 cycle 5c (advances #598).
//!
//! Bridges the cycle-3 auth result (a [`TenantId`]) to the cycle-5b
//! [`ClaudeExecSpec`] the channel-open handler hands to the spawner.
//! The resolver is the typed contract for the questions cycle 5a's
//! handler can't answer alone:
//!
//!   * What unix uid runs `claude` for THIS tenant?
//!   * Where is the `claude` binary on disk?
//!   * What working directory should the child use?
//!   * What `--resume <session-id>` is associated with the incoming
//!     SSH session? (Cycle T46.4 cookie↔uid bridge wires the session
//!     id from the admin-portal HTTP cookie via a unix-socket lookup;
//!     for cycle 5c the resolver returns the spec given a pre-resolved
//!     session id.)
//!
//! The trait is sync because the bridge calls it inside
//! `channel_open_session`, which is itself already async — but the
//! resolution itself is cheap (in-memory lookup against a registry).
//! Async-resolver implementations (DB-backed, cookie-bridge-backed)
//! can implement the trait with an `async_trait` wrapper that
//! `tokio::task::block_in_place`s, OR a future cycle replaces the
//! trait with an async one. For cycle 5c we keep it sync to avoid
//! pulling tokio into the default-features build.

use crate::exec_spec::{ClaudeExecSpec, ClaudeSessionId, ExecSpecError, TenantUid};
use crate::tenant::TenantId;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Why a resolver might refuse to issue an ExecSpec.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ResolverError {
    /// No mapping for this tenant. Either the tenant was just deleted
    /// or the registry is out of sync with the auth layer. Cycle 3
    /// already authenticated the key, so the tenant MUST exist; this
    /// surface fires on race conditions.
    #[error("no exec mapping for tenant: {0}")]
    UnknownTenant(TenantId),
    /// Spec construction failed (bad uid / bad session id / bad path).
    /// Wrapped so callers don't need to depend on `exec_spec` directly.
    #[error("exec-spec validation: {0}")]
    Spec(#[from] ExecSpecError),
}

/// Resolve `(tenant, session_id)` → an `ExecSpec`. Sync; cheap.
///
/// BUG ASSUMPTION: resolvers are stateless from the bridge's
/// perspective. The bridge constructs one resolver at startup and
/// reuses it for every channel-open. Implementations that need to
/// reload mid-flight (operator-side tenant rotation) handle that
/// internally via `Arc<RwLock<_>>`.
pub trait TenantResolver: Send + Sync + std::fmt::Debug {
    /// Build the exec spec for `tenant` with the given session id.
    ///
    /// # Errors
    ///
    /// Returns [`ResolverError::UnknownTenant`] when there is no
    /// per-tenant mapping registered, or [`ResolverError::Spec`]
    /// when the per-tenant inputs fail [`ClaudeExecSpec::new`]'s
    /// validation (e.g. an operator typo in the bridge config).
    fn resolve(
        &self,
        tenant: &TenantId,
        session_id: ClaudeSessionId,
    ) -> Result<ClaudeExecSpec, ResolverError>;
}

/// One row in the static resolver's mapping table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticTenantEntry {
    /// Unix uid for the tenant.
    pub uid: TenantUid,
    /// Absolute path to the `claude` binary.
    pub claude_binary_path: PathBuf,
    /// Working directory for the child.
    pub workdir: PathBuf,
}

/// Resolver backed by an in-memory `TenantId → StaticTenantEntry` map.
///
/// The expected deployment shape: `loom-cli tenant create acme`
/// allocates the uid + writes the entry to disk; the bridge loads
/// the registry at startup into a `StaticTenantResolver`. Hot-reload
/// is OUT OF SCOPE for cycle 5c; future cycles can swap the
/// resolver behind an `Arc<RwLock<TenantRegistry>>`.
#[derive(Debug, Clone, Default)]
pub struct StaticTenantResolver {
    table: BTreeMap<TenantId, StaticTenantEntry>,
}

impl StaticTenantResolver {
    /// Empty resolver — every `resolve` call returns `UnknownTenant`.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Insert or replace one entry.
    pub fn upsert(&mut self, tenant: TenantId, entry: StaticTenantEntry) {
        self.table.insert(tenant, entry);
    }

    /// How many entries are registered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.table.len()
    }

    /// True iff zero entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }
}

impl TenantResolver for StaticTenantResolver {
    fn resolve(
        &self,
        tenant: &TenantId,
        session_id: ClaudeSessionId,
    ) -> Result<ClaudeExecSpec, ResolverError> {
        let Some(entry) = self.table.get(tenant) else {
            return Err(ResolverError::UnknownTenant(tenant.clone()));
        };
        ClaudeExecSpec::new(
            tenant.clone(),
            entry.uid,
            session_id,
            entry.claude_binary_path.clone(),
            entry.workdir.clone(),
        )
        .map_err(ResolverError::Spec)
    }
}

/// A boxed/Arc'd resolver suitable for sharing across the bridge's
/// per-connection handlers. The russh `BridgeServer` will hold one.
pub type SharedResolver = Arc<dyn TenantResolver>;

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(uid: u32, bin: &str, wd: &str) -> StaticTenantEntry {
        StaticTenantEntry {
            uid: TenantUid::new(uid).unwrap(),
            claude_binary_path: bin.into(),
            workdir: wd.into(),
        }
    }

    fn acme() -> TenantId {
        TenantId::new("acme").unwrap()
    }

    fn sid(s: &str) -> ClaudeSessionId {
        ClaudeSessionId::new(s).unwrap()
    }

    #[test]
    fn empty_resolver_returns_unknown_tenant() {
        let r = StaticTenantResolver::empty();
        let result = r.resolve(&acme(), sid("s1"));
        assert!(matches!(result, Err(ResolverError::UnknownTenant(_))));
    }

    #[test]
    fn registered_tenant_resolves_to_spec() {
        let mut r = StaticTenantResolver::empty();
        r.upsert(
            acme(),
            entry(1042, "/usr/local/bin/claude", "/var/lib/loom/acme"),
        );
        let spec = r.resolve(&acme(), sid("s1")).expect("resolves");
        assert_eq!(spec.tenant, acme());
        assert_eq!(spec.uid.as_u32(), 1042);
        assert_eq!(spec.session_id.as_str(), "s1");
        assert_eq!(
            spec.claude_binary_path,
            std::path::PathBuf::from("/usr/local/bin/claude")
        );
    }

    #[test]
    fn upsert_replaces_existing_entry() {
        let mut r = StaticTenantResolver::empty();
        r.upsert(
            acme(),
            entry(1042, "/usr/local/bin/claude", "/var/lib/loom/acme"),
        );
        r.upsert(acme(), entry(2042, "/opt/claude", "/var/lib/loom/acme-v2"));
        let spec = r.resolve(&acme(), sid("s1")).expect("resolves");
        assert_eq!(spec.uid.as_u32(), 2042);
        assert_eq!(
            spec.claude_binary_path,
            std::path::PathBuf::from("/opt/claude")
        );
    }

    #[test]
    fn resolver_propagates_spec_validation_errors() {
        // SECURITY: the resolver must surface ExecSpec validation
        // errors verbatim. If the operator misconfigures the binary
        // path (relative, contains ..), the bridge MUST refuse to
        // resolve rather than silently coerce.
        let mut r = StaticTenantResolver::empty();
        r.upsert(
            acme(),
            StaticTenantEntry {
                uid: TenantUid::new(1042).unwrap(),
                claude_binary_path: "relative/claude".into(), // bad: not absolute
                workdir: "/var/lib/loom".into(),
            },
        );
        let result = r.resolve(&acme(), sid("s1"));
        assert!(matches!(result, Err(ResolverError::Spec(_))));
    }

    #[test]
    fn resolver_propagates_parent_ref_validation() {
        let mut r = StaticTenantResolver::empty();
        r.upsert(
            acme(),
            StaticTenantEntry {
                uid: TenantUid::new(1042).unwrap(),
                claude_binary_path: "/usr/local/../tmp/claude".into(),
                workdir: "/var/lib/loom".into(),
            },
        );
        let result = r.resolve(&acme(), sid("s1"));
        assert!(matches!(result, Err(ResolverError::Spec(_))));
    }

    #[test]
    fn len_and_is_empty_track_table() {
        let mut r = StaticTenantResolver::empty();
        assert_eq!(r.len(), 0);
        assert!(r.is_empty());
        r.upsert(
            acme(),
            entry(1042, "/usr/local/bin/claude", "/var/lib/loom"),
        );
        assert_eq!(r.len(), 1);
        assert!(!r.is_empty());
    }

    #[test]
    fn shared_resolver_arc_dispatches() {
        // Make sure the trait object compiles + dispatches via Arc.
        let mut r = StaticTenantResolver::empty();
        r.upsert(
            acme(),
            entry(1042, "/usr/local/bin/claude", "/var/lib/loom"),
        );
        let shared: SharedResolver = Arc::new(r);
        let spec = shared.resolve(&acme(), sid("s1")).expect("resolves");
        assert_eq!(spec.uid.as_u32(), 1042);
    }

    #[test]
    fn resolver_is_send_sync_for_arc_use() {
        // Compile-time check the trait object can cross thread
        // boundaries (Bridge per-conn handlers may be spawned on
        // distinct tokio worker threads).
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SharedResolver>();
    }

    #[test]
    fn unknown_tenant_error_includes_id() {
        let r = StaticTenantResolver::empty();
        let err = r.resolve(&acme(), sid("s1")).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("acme"), "error must include tenant id: {msg}");
    }

    #[test]
    fn multiple_tenants_distinct_specs() {
        let mut r = StaticTenantResolver::empty();
        r.upsert(
            acme(),
            entry(1042, "/usr/local/bin/claude", "/var/lib/loom/acme"),
        );
        r.upsert(
            TenantId::new("widgets-co").unwrap(),
            entry(1099, "/usr/local/bin/claude", "/var/lib/loom/widgets-co"),
        );
        let acme_spec = r.resolve(&acme(), sid("s1")).unwrap();
        let widgets_spec = r
            .resolve(&TenantId::new("widgets-co").unwrap(), sid("s2"))
            .unwrap();
        assert_ne!(acme_spec.uid, widgets_spec.uid);
        assert_ne!(acme_spec.workdir, widgets_spec.workdir);
    }
}
