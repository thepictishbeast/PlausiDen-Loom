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
}
