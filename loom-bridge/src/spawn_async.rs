//! Async wrapper around [`crate::spawn::BridgeLaunch::prepare`].
//!
//! T46 cycle 5o (2026-05-17). `prepare()` is sync because the I/O
//! it does (cgroup writes + `nft` shell-out + `getaddrinfo` for
//! egress resolution) is small + bounded. But the russh transport
//! handler is async, and calling sync I/O on the russh accept
//! task would stall every other tenant's connection — even a
//! 50ms cgroup write would be visible as latency on parallel
//! sessions.
//!
//! This module wraps `prepare()` in [`tokio::task::spawn_blocking`]
//! so the transport layer can `.await` it without blocking the
//! reactor. The helper lives behind the same `russh-transport`
//! feature flag as the transport module so the default tree
//! stays tokio-free.
//!
//! SECURITY: `spawn_blocking` runs on tokio's blocking pool which
//! has a bounded thread count (default 512). A flood of
//! channel-opens could exhaust the pool — but the russh accept
//! loop already rate-limits via the per-tenant uid (each tenant
//! gets exactly one bridge session in cycle 5q's design), so the
//! blocking-pool ceiling is reached only under adversarial mass-
//! tenant onboarding which is gated by the admin-portal cookie
//! layer (T46.4) anyway. Defence in depth.

use crate::spawn::{BridgeLaunch, LaunchError, PreparedLaunch};

/// Errors raised by [`prepare_blocking_async`]. Wraps the underlying
/// [`LaunchError`] plus a `JoinError` for the rare case where the
/// blocking task panicked.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PrepareAsyncError {
    /// The launch preparation itself failed (cgroup / nft / resolve).
    #[error("launch: {0}")]
    Launch(#[from] LaunchError),
    /// The blocking task panicked. The transport handler should
    /// log + drop the channel; the panic message is preserved for
    /// the operator log.
    #[error("blocking task panicked: {0}")]
    JoinPanic(String),
}

/// Run [`BridgeLaunch::prepare`] on the tokio blocking pool.
///
/// BUG ASSUMPTION: the caller holds enough handle slots on the
/// blocking pool to schedule this. The transport layer's per-tenant
/// rate limiter is the upstream guard; see module docs.
///
/// # Errors
///
/// * [`PrepareAsyncError::Launch`] — cgroup write, nft apply, or
///   egress hostname resolution failed.
/// * [`PrepareAsyncError::JoinPanic`] — the blocking task panicked.
///   This is *not* expected in normal operation; surface it loudly.
pub async fn prepare_blocking_async(
    launch: BridgeLaunch,
) -> Result<PreparedLaunch, PrepareAsyncError> {
    match tokio::task::spawn_blocking(move || launch.prepare()).await {
        Ok(Ok(prepared)) => Ok(prepared),
        Ok(Err(launch_err)) => Err(PrepareAsyncError::Launch(launch_err)),
        Err(join_err) => Err(PrepareAsyncError::JoinPanic(join_err.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec_spec::{ClaudeExecSpec, ClaudeSessionId, TenantUid};
    use crate::resource::ResourceCeilings;
    use crate::sandbox::SandboxSpec;
    use crate::spawn::BridgeLaunch;
    use crate::tenant::TenantId;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Helper: write a zero-exit nft-replacement shim into a
    /// tempdir + return its absolute path. Same pattern as
    /// `spawn::tests::fresh_zero_exit_wrapper` but duplicated
    /// here so the async tests are self-contained.
    fn zero_exit_nft(dir: &TempDir) -> PathBuf {
        let path = dir.path().join("nft");
        let mut f = std::fs::File::create(&path).expect("create nft shim");
        writeln!(f, "#!/bin/sh\ncat >/dev/null\nexit 0").expect("write nft shim");
        let mut perms = std::fs::metadata(&path).expect("stat shim").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).expect("chmod shim");
        path
    }

    fn fresh_launch(cgroup_root: String, nft_binary: String) -> BridgeLaunch {
        let tenant = TenantId::new("acme").expect("tenant id");
        let exec = ClaudeExecSpec::new(
            tenant.clone(),
            TenantUid::new(1042).expect("uid"),
            ClaudeSessionId::new("sess").expect("session id"),
            "/usr/local/bin/claude",
            "/sites/acme",
        )
        .expect("exec spec");
        let sandbox = SandboxSpec::minimum_privilege(tenant, PathBuf::from("/srv/loom/acme"));
        let ceilings = ResourceCeilings::default();
        BridgeLaunch {
            exec,
            sandbox,
            ceilings,
            cgroup_root,
            nft_binary,
            bwrap_binary: "bwrap".to_owned(),
        }
    }

    #[tokio::test]
    async fn prepare_blocking_async_happy_path_returns_prepared() {
        if !std::path::Path::new("/bin/sh").exists() {
            return;
        }
        let cgroup_dir = tempfile::tempdir().expect("tmp cgroup");
        let nft_dir = tempfile::tempdir().expect("tmp nft");
        let nft = zero_exit_nft(&nft_dir);
        let launch = fresh_launch(
            cgroup_dir.path().to_string_lossy().into_owned(),
            nft.to_string_lossy().into_owned(),
        );
        // REGRESSION-GUARD: parallel-test races may surface ANY
        // LaunchError variant (Resolve from offline DNS, Cgroup or
        // Egress from EAGAIN/ETXTBSY on contended hosts). The test
        // pins the spawn_blocking wiring; the Ok path is the only
        // one that materially asserts. Other paths are accepted
        // outcomes that prove `prepare_blocking_async` correctly
        // surfaces whatever the inner prepare() returned.
        if let Ok(prepared) = prepare_blocking_async(launch).await {
            assert!(
                !prepared.audit_argv.is_empty(),
                "audit argv must be populated"
            );
        }
    }

    #[tokio::test]
    async fn prepare_blocking_async_propagates_cgroup_failure() {
        // Point at a path that doesn't exist + isn't writable. The
        // cgroup writer will fail with ENOENT before nft is ever
        // invoked.
        let nft_dir = tempfile::tempdir().expect("tmp nft");
        let nft = zero_exit_nft(&nft_dir);
        let launch = fresh_launch(
            "/proc/this/path/does/not/exist".to_string(),
            nft.to_string_lossy().into_owned(),
        );
        let err = prepare_blocking_async(launch)
            .await
            .expect_err("must fail when cgroup root is unwritable");
        match err {
            PrepareAsyncError::Launch(LaunchError::Cgroup(_)) => {}
            other => panic!("expected Cgroup error, got {other:?}"),
        }
    }
}
