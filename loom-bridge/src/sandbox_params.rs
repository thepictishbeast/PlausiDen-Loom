//! Typed sandbox-template parameters for the bridge server.
//!
//! T46 cycle 5p (2026-05-17). The transport layer needs four pieces
//! of information at channel-open to build a [`crate::spawn::BridgeLaunch`]:
//!
//! 1. **`sandbox_root`** — absolute directory under which per-tenant
//!    sandbox subtrees live (each tenant gets `<sandbox_root>/<tenant_id>/`).
//! 2. **`ceilings`** — CPU + memory + pid resource ceilings the
//!    cgroup writer applies.
//! 3. **`cgroup_root`** — where the cgroup hierarchy is mounted
//!    (`/sys/fs/cgroup` in production; a tempdir in tests).
//! 4. **`nft_binary`** — path to the `nft` executable used for
//!    egress allowlist apply (`nft` in production; a sh-wrapper in
//!    tests).
//!
//! These four fields previously lived as positional defaults inside
//! `BridgeLaunch::new()` (`/sys/fs/cgroup`, `"nft"`). The transport
//! layer needs them as a configurable bundle, so this module
//! centralises them with validation + a builder.
//!
//! Cycle 5q wires `BridgeSandboxParams` into `BridgeServerConfig`
//! and uses it in `channel_open_session` to build the per-session
//! `BridgeLaunch`.
//!
//! SECURITY: `sandbox_root` MUST be absolute — a relative path would
//! resolve against the bridge daemon's CWD, which an operator may
//! not control across systemd unit reloads. Validation is the first
//! line of defence; the resolver layer is the second.

use std::path::{Path, PathBuf};

use crate::resource::ResourceCeilings;

/// Errors raised by [`BridgeSandboxParams`] construction.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SandboxParamsError {
    /// `sandbox_root` is empty.
    #[error("sandbox_root is empty")]
    SandboxRootEmpty,
    /// `sandbox_root` is not absolute. Relative paths would resolve
    /// against the daemon's CWD which is operator-unfriendly.
    #[error("sandbox_root must be absolute: {0}")]
    SandboxRootNotAbsolute(PathBuf),
    /// `cgroup_root` is empty.
    #[error("cgroup_root is empty")]
    CgroupRootEmpty,
    /// `nft_binary` is empty.
    #[error("nft_binary is empty")]
    NftBinaryEmpty,
}

/// Sandbox-template parameters consumed by the bridge server.
///
/// Constructed via [`BridgeSandboxParams::new`] with mandatory
/// `sandbox_root` + `ceilings`; cgroup root + nft binary default to
/// their production values (`/sys/fs/cgroup`, `nft`) and may be
/// overridden via the builder methods.
///
/// BUG ASSUMPTION: the caller's process has the necessary
/// capabilities (`CAP_SYS_ADMIN` for cgroup writes,
/// `CAP_NET_ADMIN` for nft) — this struct does NOT verify them.
/// Surfaced via the `LaunchError::Cgroup` / `LaunchError::Egress`
/// returns from `BridgeLaunch::prepare` at runtime.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct BridgeSandboxParams {
    /// Absolute directory containing per-tenant sandbox subtrees.
    pub sandbox_root: PathBuf,
    /// Resource ceilings the cgroup writer applies.
    pub ceilings: ResourceCeilings,
    /// Where the cgroup hierarchy is mounted (production:
    /// `/sys/fs/cgroup`).
    pub cgroup_root: String,
    /// Path to the `nft` binary (production: `nft` on PATH).
    pub nft_binary: String,
}

impl BridgeSandboxParams {
    /// Construct with mandatory `sandbox_root` + `ceilings`. Defaults
    /// the cgroup root to `/sys/fs/cgroup` and the nft binary to
    /// `nft`.
    ///
    /// # Errors
    ///
    /// * [`SandboxParamsError::SandboxRootEmpty`] — `sandbox_root` is empty.
    /// * [`SandboxParamsError::SandboxRootNotAbsolute`] — `sandbox_root`
    ///   is relative.
    pub fn new(
        sandbox_root: PathBuf,
        ceilings: ResourceCeilings,
    ) -> Result<Self, SandboxParamsError> {
        Self::validate_sandbox_root(&sandbox_root)?;
        Ok(Self {
            sandbox_root,
            ceilings,
            cgroup_root: "/sys/fs/cgroup".to_owned(),
            nft_binary: "nft".to_owned(),
        })
    }

    /// Override the cgroup root. Useful for tests that point at a
    /// tempdir instead of `/sys/fs/cgroup`.
    ///
    /// # Errors
    ///
    /// * [`SandboxParamsError::CgroupRootEmpty`] — empty input.
    pub fn with_cgroup_root(mut self, cgroup_root: String) -> Result<Self, SandboxParamsError> {
        if cgroup_root.is_empty() {
            return Err(SandboxParamsError::CgroupRootEmpty);
        }
        self.cgroup_root = cgroup_root;
        Ok(self)
    }

    /// Override the nft binary. Useful for tests that point at a
    /// sh-wrapper instead of the system `nft`.
    ///
    /// # Errors
    ///
    /// * [`SandboxParamsError::NftBinaryEmpty`] — empty input.
    pub fn with_nft_binary(mut self, nft_binary: String) -> Result<Self, SandboxParamsError> {
        if nft_binary.is_empty() {
            return Err(SandboxParamsError::NftBinaryEmpty);
        }
        self.nft_binary = nft_binary;
        Ok(self)
    }

    fn validate_sandbox_root(p: &Path) -> Result<(), SandboxParamsError> {
        if p.as_os_str().is_empty() {
            return Err(SandboxParamsError::SandboxRootEmpty);
        }
        if !p.is_absolute() {
            return Err(SandboxParamsError::SandboxRootNotAbsolute(p.to_path_buf()));
        }
        Ok(())
    }

    /// Path to a specific tenant's sandbox subtree
    /// (`<sandbox_root>/<tenant_id>`). Pure path arithmetic; does NOT
    /// create the directory.
    #[must_use]
    pub fn tenant_sandbox_dir(&self, tenant_id: &str) -> PathBuf {
        self.sandbox_root.join(tenant_id)
    }

    /// T46 cycle 5r (2026-05-17): compose a `BridgeLaunch` from this
    /// sandbox-params bundle plus the per-session `ClaudeExecSpec`.
    ///
    /// Centralises what the transport handler used to do inline
    /// (`channel_open_session` in cycle 5q), so the launch composition
    /// can be unit-tested without standing up a russh session and so
    /// future callers (loom-cli's smoke harness, the dry-run flag,
    /// etc.) share the same construction.
    ///
    /// BUG ASSUMPTION: the caller has already validated that
    /// `exec.tenant` matches the tenant the resolver returned the
    /// spec for. Mismatch would silently sandbox the wrong tenant's
    /// exec — a SECURITY-relevant invariant the cycle-5d resolver
    /// path guarantees.
    #[must_use]
    pub fn build_launch(
        &self,
        exec: crate::exec_spec::ClaudeExecSpec,
    ) -> crate::spawn::BridgeLaunch {
        let sandbox = crate::sandbox::SandboxSpec::minimum_privilege(
            exec.tenant.clone(),
            self.tenant_sandbox_dir(exec.tenant.as_str()),
        );
        crate::spawn::BridgeLaunch {
            exec,
            sandbox,
            ceilings: self.ceilings.clone(),
            cgroup_root: self.cgroup_root.clone(),
            nft_binary: self.nft_binary.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ceilings() -> ResourceCeilings {
        ResourceCeilings::default()
    }

    #[test]
    fn new_with_absolute_root_succeeds() {
        let p = BridgeSandboxParams::new(PathBuf::from("/srv/loom"), ceilings()).expect("ok");
        assert_eq!(p.sandbox_root, PathBuf::from("/srv/loom"));
        assert_eq!(p.cgroup_root, "/sys/fs/cgroup");
        assert_eq!(p.nft_binary, "nft");
    }

    #[test]
    fn new_with_empty_root_returns_error() {
        let err = BridgeSandboxParams::new(PathBuf::new(), ceilings()).expect_err("empty fails");
        assert!(matches!(err, SandboxParamsError::SandboxRootEmpty));
    }

    #[test]
    fn new_with_relative_root_returns_error() {
        let err = BridgeSandboxParams::new(PathBuf::from("relative/path"), ceilings())
            .expect_err("relative fails");
        assert!(matches!(err, SandboxParamsError::SandboxRootNotAbsolute(_)));
    }

    #[test]
    fn with_cgroup_root_overrides_and_rejects_empty() {
        let p = BridgeSandboxParams::new(PathBuf::from("/srv/loom"), ceilings())
            .expect("ok")
            .with_cgroup_root("/tmp/cgtest".to_owned())
            .expect("ok override");
        assert_eq!(p.cgroup_root, "/tmp/cgtest");

        let err = BridgeSandboxParams::new(PathBuf::from("/srv/loom"), ceilings())
            .expect("ok")
            .with_cgroup_root(String::new())
            .expect_err("empty cgroup fails");
        assert!(matches!(err, SandboxParamsError::CgroupRootEmpty));
    }

    #[test]
    fn with_nft_binary_overrides_and_rejects_empty() {
        let p = BridgeSandboxParams::new(PathBuf::from("/srv/loom"), ceilings())
            .expect("ok")
            .with_nft_binary("/tmp/sh-wrapper".to_owned())
            .expect("ok override");
        assert_eq!(p.nft_binary, "/tmp/sh-wrapper");

        let err = BridgeSandboxParams::new(PathBuf::from("/srv/loom"), ceilings())
            .expect("ok")
            .with_nft_binary(String::new())
            .expect_err("empty nft fails");
        assert!(matches!(err, SandboxParamsError::NftBinaryEmpty));
    }

    #[test]
    fn tenant_sandbox_dir_joins_tenant_id() {
        let p = BridgeSandboxParams::new(PathBuf::from("/srv/loom"), ceilings()).expect("ok");
        assert_eq!(
            p.tenant_sandbox_dir("acme"),
            PathBuf::from("/srv/loom/acme")
        );
        assert_eq!(
            p.tenant_sandbox_dir("widgetco"),
            PathBuf::from("/srv/loom/widgetco")
        );
    }

    #[test]
    fn params_are_clonable_for_per_session_use() {
        let p = BridgeSandboxParams::new(PathBuf::from("/srv/loom"), ceilings()).expect("ok");
        let cloned = p.clone();
        assert_eq!(p.sandbox_root, cloned.sandbox_root);
        assert_eq!(p.cgroup_root, cloned.cgroup_root);
        assert_eq!(p.nft_binary, cloned.nft_binary);
    }

    // ---------- cycle 5r: build_launch ----------

    #[test]
    fn build_launch_composes_per_tenant_sandbox_and_cgroup() {
        use crate::exec_spec::{ClaudeExecSpec, ClaudeSessionId, TenantUid};
        use crate::tenant::TenantId;
        let tenant = TenantId::new("acme").expect("tenant id");
        let exec = ClaudeExecSpec::new(
            tenant.clone(),
            TenantUid::new(1042).expect("uid"),
            ClaudeSessionId::new("sess-x").expect("session id"),
            "/usr/local/bin/claude",
            "/sites/acme",
        )
        .expect("exec spec");
        let params = BridgeSandboxParams::new(PathBuf::from("/srv/loom"), ceilings())
            .expect("ok")
            .with_cgroup_root("/tmp/cg".to_owned())
            .expect("cgroup")
            .with_nft_binary("/tmp/nft-stub".to_owned())
            .expect("nft");
        let launch = params.build_launch(exec.clone());
        assert_eq!(launch.exec.tenant, tenant);
        assert_eq!(launch.cgroup_root, "/tmp/cg");
        assert_eq!(launch.nft_binary, "/tmp/nft-stub");
        assert_eq!(launch.sandbox.session_root, PathBuf::from("/srv/loom/acme"));
    }

    #[test]
    fn build_launch_clones_ceilings_for_per_session_isolation() {
        use crate::exec_spec::{ClaudeExecSpec, ClaudeSessionId, TenantUid};
        use crate::tenant::TenantId;
        let tenant = TenantId::new("widgetco").expect("tenant id");
        let exec = ClaudeExecSpec::new(
            tenant.clone(),
            TenantUid::new(2000).expect("uid"),
            ClaudeSessionId::new("sess-y").expect("session id"),
            "/usr/local/bin/claude",
            "/sites/widgetco",
        )
        .expect("exec spec");
        let params = BridgeSandboxParams::new(PathBuf::from("/srv/loom"), ceilings()).expect("ok");
        let launch_a = params.build_launch(exec.clone());
        let launch_b = params.build_launch(exec);
        assert_eq!(launch_a.ceilings.cpu_percent, launch_b.ceilings.cpu_percent);
        assert_eq!(launch_a.ceilings.memory_mib, launch_b.ceilings.memory_mib);
        // Same tenant → same sandbox subtree (path arithmetic identity).
        assert_eq!(launch_a.sandbox.session_root, launch_b.sandbox.session_root);
    }
}
