//! Sandbox / jail configuration for the per-tenant Claude session.
//!
//! T46 cycle 3 (advances #598). Owns the TYPED jail spec — what
//! filesystem paths the tenant's Claude can see, what network
//! egress is allowed, which syscalls are forbidden. The actual
//! `bwrap` / `systemd-nspawn` invocation lives behind the
//! `russh-transport` feature in [`crate::transport`] and consumes
//! this spec.
//!
//! Splitting spec ↔ executor lets every variant (bwrap on Linux,
//! `sandbox-exec` on macOS dev boxes, no-op stub on CI) consume
//! the same typed config, and lets us test the spec on every
//! platform without needing the underlying jail tool.

use crate::tenant::TenantId;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Fully-typed jail spec for one tenant's `claude --resume` invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SandboxSpec {
    /// Tenant this sandbox belongs to. Doubles as the unix
    /// username the jail runs as.
    pub tenant: TenantId,
    /// The tenant's session-root directory. The jail mounts this
    /// read-write at `/sites/<tenant>` inside the namespace.
    pub session_root: PathBuf,
    /// Network egress allowlist. EXACT hostnames (no wildcards).
    /// Empty = no egress at all.
    pub egress_allowlist: Vec<String>,
    /// Whether to allow the jailed process to spawn sub-processes.
    /// Default: false. Claude's own SDK fans out via tool calls,
    /// not via shelling out, so this stays off in v1.
    pub allow_subprocess: bool,
    /// Whether to allow `ptrace` from the jail. Default: false.
    /// Pretty much never needed; turning on disables one of the
    /// stronger Spectre / process-introspection mitigations.
    pub allow_ptrace: bool,
}

impl SandboxSpec {
    /// Build a minimum-privilege spec for a tenant. Caller must
    /// supply a writable session_root; everything else defaults
    /// to the safest setting.
    ///
    /// The default egress allowlist contains ONLY
    /// `api.anthropic.com` (Claude API) and `github.com` (clone
    /// + fetch). Operators can extend per-tenant later.
    #[must_use]
    pub fn minimum_privilege(tenant: TenantId, session_root: PathBuf) -> Self {
        Self {
            tenant,
            session_root,
            egress_allowlist: vec!["api.anthropic.com".into(), "github.com".into()],
            allow_subprocess: false,
            allow_ptrace: false,
        }
    }

    /// True iff the spec would allow egress to the given hostname.
    /// Case-insensitive exact-match.
    #[must_use]
    pub fn allows_egress_to(&self, host: &str) -> bool {
        self.egress_allowlist
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(host))
    }

    /// Lint the spec for obviously-broken combinations. Returns
    /// the list of violations (empty = OK). Doesn't fail-fast —
    /// the executor decides what to do with each.
    #[must_use]
    pub fn lint(&self) -> Vec<SandboxLint> {
        let mut out = Vec::new();
        if !self.session_root.is_absolute() {
            out.push(SandboxLint::RelativeSessionRoot(self.session_root.clone()));
        }
        if self.allow_subprocess && self.allow_ptrace {
            out.push(SandboxLint::SubprocessAndPtraceTogether);
        }
        if self.egress_allowlist.is_empty() {
            out.push(SandboxLint::NoEgressAllowlist);
        }
        for host in &self.egress_allowlist {
            if host.contains('*') || host.contains('?') {
                out.push(SandboxLint::WildcardHost(host.clone()));
            }
            if host.contains('/') || host.contains(':') {
                out.push(SandboxLint::HostHasPathOrPort(host.clone()));
            }
            if host.is_empty() {
                out.push(SandboxLint::EmptyHost);
            }
        }
        out
    }
}

/// One lint diagnostic for a [`SandboxSpec`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SandboxLint {
    /// session_root is relative; bwrap won't accept that.
    RelativeSessionRoot(PathBuf),
    /// Both allow_subprocess + allow_ptrace = remote attacker
    /// who escalates to subprocess can also ptrace the host.
    SubprocessAndPtraceTogether,
    /// No egress allowlist at all — Claude API call will fail.
    NoEgressAllowlist,
    /// Wildcard host (not supported by the exact-match egress filter).
    WildcardHost(String),
    /// Host contains `:` or `/` — the egress filter wants hostname
    /// only, not URL.
    HostHasPathOrPort(String),
    /// Empty hostname.
    EmptyHost,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tid() -> TenantId {
        TenantId::new("acme").unwrap()
    }

    #[test]
    fn minimum_privilege_has_no_subprocess_or_ptrace() {
        let s = SandboxSpec::minimum_privilege(tid(), PathBuf::from("/srv/loom/acme"));
        assert!(!s.allow_subprocess);
        assert!(!s.allow_ptrace);
    }

    #[test]
    fn minimum_privilege_allows_anthropic_and_github() {
        let s = SandboxSpec::minimum_privilege(tid(), PathBuf::from("/srv/loom/acme"));
        assert!(s.allows_egress_to("api.anthropic.com"));
        assert!(s.allows_egress_to("github.com"));
        assert!(!s.allows_egress_to("evil.example.com"));
    }

    #[test]
    fn egress_lookup_is_case_insensitive() {
        let s = SandboxSpec::minimum_privilege(tid(), PathBuf::from("/srv/loom/acme"));
        assert!(s.allows_egress_to("API.ANTHROPIC.COM"));
        assert!(s.allows_egress_to("GitHub.com"));
    }

    #[test]
    fn lint_passes_on_minimum_privilege() {
        let s = SandboxSpec::minimum_privilege(tid(), PathBuf::from("/srv/loom/acme"));
        assert!(s.lint().is_empty());
    }

    #[test]
    fn lint_flags_relative_session_root() {
        let s = SandboxSpec::minimum_privilege(tid(), PathBuf::from("relative/path"));
        assert!(matches!(
            s.lint().as_slice(),
            [SandboxLint::RelativeSessionRoot(_)]
        ));
    }

    #[test]
    fn lint_flags_subprocess_and_ptrace_combo() {
        let mut s = SandboxSpec::minimum_privilege(tid(), PathBuf::from("/srv/loom/acme"));
        s.allow_subprocess = true;
        s.allow_ptrace = true;
        let lints = s.lint();
        assert!(
            lints
                .iter()
                .any(|l| matches!(l, SandboxLint::SubprocessAndPtraceTogether))
        );
    }

    #[test]
    fn lint_flags_empty_egress_allowlist() {
        let mut s = SandboxSpec::minimum_privilege(tid(), PathBuf::from("/srv/loom/acme"));
        s.egress_allowlist.clear();
        assert!(
            s.lint()
                .iter()
                .any(|l| matches!(l, SandboxLint::NoEgressAllowlist))
        );
    }

    #[test]
    fn lint_flags_wildcard_host() {
        let mut s = SandboxSpec::minimum_privilege(tid(), PathBuf::from("/srv/loom/acme"));
        s.egress_allowlist.push("*.evil.com".into());
        assert!(
            s.lint()
                .iter()
                .any(|l| matches!(l, SandboxLint::WildcardHost(h) if h == "*.evil.com"))
        );
    }

    #[test]
    fn lint_flags_url_in_host_field() {
        let mut s = SandboxSpec::minimum_privilege(tid(), PathBuf::from("/srv/loom/acme"));
        s.egress_allowlist.push("https://evil.com/path".into());
        assert!(
            s.lint()
                .iter()
                .any(|l| matches!(l, SandboxLint::HostHasPathOrPort(_)))
        );
    }

    #[test]
    fn lint_flags_empty_host_entry() {
        let mut s = SandboxSpec::minimum_privilege(tid(), PathBuf::from("/srv/loom/acme"));
        s.egress_allowlist.push("".into());
        assert!(s.lint().iter().any(|l| matches!(l, SandboxLint::EmptyHost)));
    }

    #[test]
    fn spec_round_trips_through_json() {
        let s = SandboxSpec::minimum_privilege(tid(), PathBuf::from("/srv/loom/acme"));
        let json = serde_json::to_string(&s).unwrap();
        let back: SandboxSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.tenant.as_str(), s.tenant.as_str());
        assert_eq!(back.session_root, s.session_root);
        assert_eq!(back.egress_allowlist, s.egress_allowlist);
    }
}
