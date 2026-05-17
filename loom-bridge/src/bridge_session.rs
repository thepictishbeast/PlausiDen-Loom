//! Live bridge session — the spawned `claude --resume` child plus
//! the handles needed to bridge its stdio to the russh channel.
//!
//! T46 cycle 5s (2026-05-17). The transport handler invokes
//! [`spawn_claude_session`] after [`crate::spawn_async::prepare_blocking_async`]
//! to actually fire the prepared command. The returned
//! [`BridgeSession`] owns the [`tokio::process::Child`] plus the
//! piped stdio handles; cycle 5t will plug those into the russh
//! channel's `data()` / `extended_data()` plumbing.
//!
//! This module is feature-gated behind `russh-transport` because
//! tokio is gated there. The conversion `std::process::Command →
//! tokio::process::Command` happens here so the spawn-blocking
//! `PreparedLaunch` can hand off cleanly into the async world.
//!
//! SECURITY: spawning the child completes the cgroup-then-nft-then-
//! exec pipeline. By the time `spawn_claude_session` returns, the
//! child PID is already constrained by the cgroup CPU/memory ceiling
//! AND its first socket attempt will hit the per-tenant nftables
//! allowlist. Any attempt to exfiltrate before the egress sets are
//! populated would be DROPped at the kernel layer.

use std::process::Stdio;

use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command as TokioCommand};

use crate::egress::{NftablesRuleset, ResolvedAllowlist};
use crate::spawn::PreparedLaunch;

/// Errors raised by [`spawn_claude_session`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SpawnSessionError {
    /// The fork+exec failed. Wraps `std::io::Error`.
    #[error("spawn claude session: {0}")]
    Spawn(#[from] std::io::Error),
    /// Tokio didn't hand back one of the stdio handles. Should be
    /// impossible if we set `Stdio::piped()` on all three before
    /// `.spawn()` — surfaced loudly so a future regression in
    /// `tokio::process::Command::from` is caught.
    #[error("stdio handle missing post-spawn: {0}")]
    StdioMissing(&'static str),
}

/// A live bridge session — the spawned child + its stdio handles +
/// the audit metadata the transport layer needs to surface in the
/// session log.
///
/// `child`, `stdin`, `stdout`, `stderr` are wrapped in `Option` so
/// the transport layer can `take()` individual handles when it sets
/// up pump tasks (the pump tasks then own the handles for their
/// lifetime). The `child` itself stays on the session until reaped.
#[derive(Debug)]
#[non_exhaustive]
pub struct BridgeSession {
    /// The spawned `claude --resume` (or test stand-in) child.
    pub child: Option<Child>,
    /// Piped stdin. `take()` it to give to the inbound-data pump task.
    pub stdin: Option<ChildStdin>,
    /// Piped stdout. `take()` it to give to the outbound pump task.
    pub stdout: Option<ChildStdout>,
    /// Piped stderr. `take()` it to give to the extended-data pump task.
    pub stderr: Option<ChildStderr>,
    /// Audit-trail argv (same shape as `PreparedLaunch::audit_argv`).
    /// Carried through so log lines + the transport banner have the
    /// exact argv that landed the child.
    pub audit_argv: Vec<String>,
    /// nftables ruleset that was applied. Carried through for audit.
    pub applied_ruleset: NftablesRuleset,
    /// Resolved allowlist (or None when the sandbox spec had no
    /// hosts to resolve). Carried through for audit.
    pub resolved_allowlist: Option<ResolvedAllowlist>,
}

/// Convert a [`PreparedLaunch`] into a [`BridgeSession`] by
/// spawning the child under tokio with all three stdio streams
/// piped.
///
/// BUG ASSUMPTION: callers have already validated the prepared
/// command via `prepare_blocking_async` — cgroup writes succeeded,
/// the nftables ruleset is in force, the egress allowlist is
/// populated (or empty = DROP-all). Calling this function with
/// an unprepared command would land an unconstrained child.
///
/// # Errors
///
/// * [`SpawnSessionError::Spawn`] — fork+exec failed (binary
///   missing, EAGAIN under fd pressure, no exec permission).
/// * [`SpawnSessionError::StdioMissing`] — tokio failed to hand
///   back a stdio handle despite `Stdio::piped()` being set.
pub fn spawn_claude_session(prepared: PreparedLaunch) -> Result<BridgeSession, SpawnSessionError> {
    let PreparedLaunch {
        command,
        audit_argv,
        applied_ruleset,
        resolved_allowlist,
    } = prepared;
    let mut tokio_cmd = TokioCommand::from(command);
    tokio_cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // The cgroup/nft pipeline is OUR responsibility; the child
        // process group should NOT inherit any wedge controlled by
        // a parent group (e.g., a CTRL+C from the operator's
        // controlling terminal — the russh server has no controlling
        // terminal, but defence-in-depth says be explicit).
        .kill_on_drop(true);
    let mut child = tokio_cmd.spawn()?;
    let stdin = child
        .stdin
        .take()
        .ok_or(SpawnSessionError::StdioMissing("stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or(SpawnSessionError::StdioMissing("stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or(SpawnSessionError::StdioMissing("stderr"))?;
    Ok(BridgeSession {
        child: Some(child),
        stdin: Some(stdin),
        stdout: Some(stdout),
        stderr: Some(stderr),
        audit_argv,
        applied_ruleset,
        resolved_allowlist,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::egress::NftablesRuleset;
    use crate::spawn::PreparedLaunch;
    use std::process::Command as StdCommand;

    /// Helper: build a `PreparedLaunch` whose command runs
    /// `/bin/sh -c "<body>"`. Used to exercise spawn without
    /// needing a real bwrap/claude binary on the test host.
    fn prepared_with_sh(body: &str) -> PreparedLaunch {
        let mut cmd = StdCommand::new("/bin/sh");
        cmd.arg("-c").arg(body);
        PreparedLaunch {
            command: cmd,
            audit_argv: vec!["/bin/sh".to_owned(), "-c".to_owned(), body.to_owned()],
            applied_ruleset: NftablesRuleset {
                ruleset: String::new(),
                table_name: "test-table".to_owned(),
                set4_name: "test_v4".to_owned(),
                set6_name: "test_v6".to_owned(),
                allowlist_hosts: vec![],
            },
            resolved_allowlist: None,
        }
    }

    #[tokio::test]
    async fn spawn_returns_handles_for_all_three_stdio_streams() {
        if !std::path::Path::new("/bin/sh").exists() {
            return;
        }
        let prepared = prepared_with_sh("cat");
        let session = spawn_claude_session(prepared).expect("spawn ok");
        assert!(session.child.is_some(), "child handle present");
        assert!(session.stdin.is_some(), "stdin piped");
        assert!(session.stdout.is_some(), "stdout piped");
        assert!(session.stderr.is_some(), "stderr piped");
        assert_eq!(session.audit_argv[0], "/bin/sh");
    }

    #[tokio::test]
    async fn spawn_stdin_to_stdout_round_trips() {
        // /bin/cat echoes stdin to stdout. Write "hello", close
        // stdin, read stdout, await exit. Proves the spawn +
        // stdio plumbing works end-to-end.
        if !std::path::Path::new("/bin/sh").exists() {
            return;
        }
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let prepared = prepared_with_sh("cat");
        let mut session = spawn_claude_session(prepared).expect("spawn ok");
        let mut stdin = session.stdin.take().expect("stdin take");
        let mut stdout = session.stdout.take().expect("stdout take");
        stdin.write_all(b"hello\n").await.expect("stdin write");
        drop(stdin);
        let mut buf = String::new();
        stdout.read_to_string(&mut buf).await.expect("stdout read");
        assert_eq!(buf, "hello\n");
        let status = session
            .child
            .as_mut()
            .expect("child")
            .wait()
            .await
            .expect("wait");
        assert!(status.success(), "cat should exit 0 on EOF");
    }

    #[tokio::test]
    async fn spawn_missing_binary_surfaces_spawn_error() {
        let mut cmd = StdCommand::new("/no/such/binary/exists/for/this/test");
        cmd.arg("--ignore");
        let prepared = PreparedLaunch {
            command: cmd,
            audit_argv: vec!["/no/such/binary".to_owned()],
            applied_ruleset: NftablesRuleset {
                ruleset: String::new(),
                table_name: "t".to_owned(),
                set4_name: "v4".to_owned(),
                set6_name: "v6".to_owned(),
                allowlist_hosts: vec![],
            },
            resolved_allowlist: None,
        };
        let err = spawn_claude_session(prepared).expect_err("missing binary");
        assert!(matches!(err, SpawnSessionError::Spawn(_)));
    }
}
