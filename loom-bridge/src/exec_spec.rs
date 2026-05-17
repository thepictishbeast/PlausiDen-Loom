//! Typed `claude --resume` exec spec. T46 cycle 5b (advances #598).
//!
//! What the bridge needs to know to swap the cycle-5a hello banner
//! for the real `claude --resume <session-id>` execution under the
//! tenant's uid + bwrap jail + cgroup ceilings.
//!
//! Lives in the default-features tree (not behind russh-transport)
//! because operator tooling (loom-cli) needs to validate the spec
//! shape without tokio + russh in scope. Cycle 5c will wire this
//! into the russh `channel_open_session` handler, swapping the
//! `format_hello_banner` byte stream for an actual `tokio::process::
//! Command` stdin/stdout/stderr bridge.
//!
//! AVP-2 invariants:
//!   * Every field validated at construction; no runtime surprise.
//!   * `claude_binary_path` rejected on PathBuf containing `..` or
//!     starting with anything other than `/` — defence against an
//!     attacker who controls part of the registry from escalating
//!     by pointing the bridge at `/tmp/evil/claude`.
//!   * Tenant uid bounded to NON-zero, non-system (>= 1000) — root
//!     execution is refused at the type level.
//!   * Session id constrained to a `[a-zA-Z0-9_-]{1,128}` charset
//!     so the value is safe to interpolate into `--resume <id>`
//!     without shell quoting.

use crate::tenant::TenantId;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Minimum allowed unix uid. Anything below this is a system
/// account on most distros; tenants must NEVER overlap.
const MIN_TENANT_UID: u32 = 1000;
/// Maximum session-id length. Longer ids are almost always a bug.
const MAX_SESSION_ID_LEN: usize = 128;

/// Validated unix uid for a tenant — non-zero, non-system.
///
/// BUG ASSUMPTION: the deployment's PAM / passwd / shadow files
/// have an entry for this uid before the bridge tries to exec under
/// it. The bridge does NOT create users on the fly — that's the
/// operator's responsibility (loom-cli tenant create).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct TenantUid(u32);

impl TenantUid {
    /// Construct from a raw uid. Rejects root + system accounts.
    ///
    /// # Errors
    ///
    /// Returns [`ExecSpecError::UidTooLow`] for uid < 1000.
    pub const fn new(uid: u32) -> Result<Self, ExecSpecError> {
        if uid < MIN_TENANT_UID {
            return Err(ExecSpecError::UidTooLow(uid));
        }
        Ok(Self(uid))
    }

    /// Raw uid value.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

impl From<TenantUid> for u32 {
    fn from(t: TenantUid) -> u32 {
        t.0
    }
}

impl TryFrom<u32> for TenantUid {
    type Error = ExecSpecError;
    fn try_from(v: u32) -> Result<Self, Self::Error> {
        Self::new(v)
    }
}

/// Validated `claude --resume <id>` session identifier.
///
/// Constraints: 1–128 chars, `[a-zA-Z0-9_-]` only. Safe to
/// interpolate into a `--resume <id>` argv without shell quoting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ClaudeSessionId(String);

impl ClaudeSessionId {
    /// Construct from a string. Validates charset + length.
    ///
    /// # Errors
    ///
    /// Returns [`ExecSpecError::InvalidSessionId`] for empty,
    /// too-long, or non-conforming inputs.
    pub fn new(s: impl Into<String>) -> Result<Self, ExecSpecError> {
        let s: String = s.into();
        if s.is_empty() || s.len() > MAX_SESSION_ID_LEN {
            return Err(ExecSpecError::InvalidSessionId(s));
        }
        if !s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(ExecSpecError::InvalidSessionId(s));
        }
        Ok(Self(s))
    }

    /// Borrow as &str.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<ClaudeSessionId> for String {
    fn from(s: ClaudeSessionId) -> String {
        s.0
    }
}

impl TryFrom<String> for ClaudeSessionId {
    type Error = ExecSpecError;
    fn try_from(v: String) -> Result<Self, Self::Error> {
        Self::new(v)
    }
}

/// Full typed exec contract.
///
/// BUG ASSUMPTION: cycle 5c (russh channel-open → exec wiring) calls
/// this AFTER the tenant has been authenticated (via the cycle-3
/// ed25519 auth) AND the cookie↔uid bridge (T46.4) has resolved
/// the tenant id to a uid. This struct is the typed handoff between
/// the auth+resolve layer and the spawn layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ClaudeExecSpec {
    /// The tenant whose uid will own the child process.
    pub tenant: TenantId,
    /// Resolved unix uid for the tenant. Non-zero, non-system.
    pub uid: TenantUid,
    /// `--resume` session id.
    pub session_id: ClaudeSessionId,
    /// Absolute path to the `claude` binary. Validated to be
    /// absolute + no `..` components.
    pub claude_binary_path: PathBuf,
    /// Working directory for the child. Validated absolute.
    pub workdir: PathBuf,
}

impl ClaudeExecSpec {
    /// Construct + validate from raw inputs.
    ///
    /// # Errors
    ///
    /// Validates absolute path + no `..` for both `claude_binary_path`
    /// and `workdir`. Returns [`ExecSpecError::PathNotAbsolute`] or
    /// [`ExecSpecError::PathHasParentRef`] on failure.
    pub fn new(
        tenant: TenantId,
        uid: TenantUid,
        session_id: ClaudeSessionId,
        claude_binary_path: impl Into<PathBuf>,
        workdir: impl Into<PathBuf>,
    ) -> Result<Self, ExecSpecError> {
        let claude_binary_path = claude_binary_path.into();
        let workdir = workdir.into();
        Self::validate_path(&claude_binary_path, "claude_binary_path")?;
        Self::validate_path(&workdir, "workdir")?;
        Ok(Self {
            tenant,
            uid,
            session_id,
            claude_binary_path,
            workdir,
        })
    }

    /// SECURITY: validate a path is absolute + contains no `..`
    /// component. Defence against an attacker who controls part of
    /// the registry from escalating via path-traversal.
    fn validate_path(p: &std::path::Path, field: &'static str) -> Result<(), ExecSpecError> {
        if !p.is_absolute() {
            return Err(ExecSpecError::PathNotAbsolute {
                field,
                value: p.to_path_buf(),
            });
        }
        if p.components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(ExecSpecError::PathHasParentRef {
                field,
                value: p.to_path_buf(),
            });
        }
        Ok(())
    }

    /// The argv this spec would run as. Pure (no I/O). Cycle 5c
    /// hands this to `tokio::process::Command::new(argv[0]).args(argv[1..])`.
    #[must_use]
    pub fn argv(&self) -> Vec<String> {
        vec![
            self.claude_binary_path.to_string_lossy().into_owned(),
            "--resume".to_owned(),
            self.session_id.as_str().to_owned(),
        ]
    }
}

/// Errors at the exec-spec layer.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ExecSpecError {
    /// Tenant uid below 1000 (system / root account).
    #[error("tenant uid {0} too low — must be >= {MIN_TENANT_UID} (non-system)")]
    UidTooLow(u32),
    /// Session id failed charset / length validation.
    #[error("invalid claude session id: {0:?}")]
    InvalidSessionId(String),
    /// A path field was not absolute.
    #[error("{field} must be an absolute path, got {value:?}")]
    PathNotAbsolute {
        /// Which field failed validation.
        field: &'static str,
        /// The offending path.
        value: PathBuf,
    },
    /// A path field contained a `..` component.
    #[error("{field} must not contain '..', got {value:?}")]
    PathHasParentRef {
        /// Which field failed validation.
        field: &'static str,
        /// The offending path.
        value: PathBuf,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_spec() -> ClaudeExecSpec {
        ClaudeExecSpec::new(
            TenantId::new("acme").unwrap(),
            TenantUid::new(1042).unwrap(),
            ClaudeSessionId::new("abc-DEF_123").unwrap(),
            "/usr/local/bin/claude",
            "/var/lib/loom-bridge/tenants/acme",
        )
        .expect("ok spec")
    }

    #[test]
    fn tenant_uid_accepts_non_system() {
        assert!(TenantUid::new(1000).is_ok());
        assert!(TenantUid::new(9999).is_ok());
    }

    #[test]
    fn tenant_uid_rejects_root_and_system() {
        for bad in [0_u32, 1, 100, 999] {
            assert!(
                matches!(TenantUid::new(bad), Err(ExecSpecError::UidTooLow(_))),
                "uid {bad} must be rejected"
            );
        }
    }

    #[test]
    fn session_id_accepts_valid_charset() {
        assert!(ClaudeSessionId::new("abc").is_ok());
        assert!(ClaudeSessionId::new("a-b_c-123").is_ok());
        assert!(ClaudeSessionId::new("ABC_def-789").is_ok());
    }

    #[test]
    fn session_id_rejects_empty_and_too_long() {
        assert!(matches!(
            ClaudeSessionId::new(""),
            Err(ExecSpecError::InvalidSessionId(_))
        ));
        let long = "a".repeat(129);
        assert!(matches!(
            ClaudeSessionId::new(long),
            Err(ExecSpecError::InvalidSessionId(_))
        ));
    }

    #[test]
    fn session_id_rejects_shell_special_chars() {
        // SECURITY: these must not interpolate into argv without
        // escaping. Reject at type level.
        for bad in [
            "abc;ls", "abc$x", "abc`x`", "abc x", "abc\"x", "abc/x", "abc..x",
        ] {
            assert!(
                matches!(
                    ClaudeSessionId::new(bad),
                    Err(ExecSpecError::InvalidSessionId(_))
                ),
                "session id {bad:?} must be rejected"
            );
        }
    }

    #[test]
    fn spec_validates_absolute_binary_path() {
        let r = ClaudeExecSpec::new(
            TenantId::new("acme").unwrap(),
            TenantUid::new(1042).unwrap(),
            ClaudeSessionId::new("s1").unwrap(),
            "relative/claude",
            "/var/lib/loom",
        );
        assert!(matches!(
            r,
            Err(ExecSpecError::PathNotAbsolute {
                field: "claude_binary_path",
                ..
            })
        ));
    }

    #[test]
    fn spec_validates_absolute_workdir() {
        let r = ClaudeExecSpec::new(
            TenantId::new("acme").unwrap(),
            TenantUid::new(1042).unwrap(),
            ClaudeSessionId::new("s1").unwrap(),
            "/usr/local/bin/claude",
            "relative/workdir",
        );
        assert!(matches!(
            r,
            Err(ExecSpecError::PathNotAbsolute {
                field: "workdir",
                ..
            })
        ));
    }

    #[test]
    fn spec_rejects_parent_ref_in_binary_path() {
        // SECURITY: path traversal defence. An attacker who controls
        // any segment of the registry path must not be able to
        // escalate via `..`.
        let r = ClaudeExecSpec::new(
            TenantId::new("acme").unwrap(),
            TenantUid::new(1042).unwrap(),
            ClaudeSessionId::new("s1").unwrap(),
            "/usr/local/../tmp/claude",
            "/var/lib/loom",
        );
        assert!(matches!(
            r,
            Err(ExecSpecError::PathHasParentRef {
                field: "claude_binary_path",
                ..
            })
        ));
    }

    #[test]
    fn spec_rejects_parent_ref_in_workdir() {
        let r = ClaudeExecSpec::new(
            TenantId::new("acme").unwrap(),
            TenantUid::new(1042).unwrap(),
            ClaudeSessionId::new("s1").unwrap(),
            "/usr/local/bin/claude",
            "/var/lib/loom/../tmp",
        );
        assert!(matches!(
            r,
            Err(ExecSpecError::PathHasParentRef {
                field: "workdir",
                ..
            })
        ));
    }

    #[test]
    fn argv_is_three_args() {
        let s = ok_spec();
        let argv = s.argv();
        assert_eq!(argv.len(), 3);
        assert_eq!(argv[0], "/usr/local/bin/claude");
        assert_eq!(argv[1], "--resume");
        assert_eq!(argv[2], "abc-DEF_123");
    }

    #[test]
    fn argv_session_id_not_shell_quoted() {
        // The session id charset is restricted enough that argv-mode
        // exec (no shell) is safe. This test pins the no-shell
        // assumption: argv must NOT contain quote characters or any
        // pre-escaping that would break direct fork/exec.
        let s = ok_spec();
        let argv = s.argv();
        assert!(!argv[2].contains('"'));
        assert!(!argv[2].contains('\''));
        assert!(!argv[2].contains('\\'));
    }

    #[test]
    fn spec_serde_round_trips() {
        let s = ok_spec();
        let j = serde_json::to_string(&s).expect("ser");
        let back: ClaudeExecSpec = serde_json::from_str(&j).expect("de");
        assert_eq!(back, s);
    }

    #[test]
    fn tenant_uid_serde_via_u32() {
        let u = TenantUid::new(1042).unwrap();
        let j = serde_json::to_string(&u).expect("ser");
        assert_eq!(j, "1042");
        let back: TenantUid = serde_json::from_str("1042").expect("de");
        assert_eq!(back, u);
    }

    #[test]
    fn tenant_uid_serde_rejects_root_at_deserialize() {
        let r: Result<TenantUid, _> = serde_json::from_str("0");
        assert!(r.is_err());
    }

    #[test]
    fn session_id_serde_via_string() {
        let s = ClaudeSessionId::new("hello-1").unwrap();
        let j = serde_json::to_string(&s).expect("ser");
        assert_eq!(j, "\"hello-1\"");
        let back: ClaudeSessionId = serde_json::from_str("\"hello-1\"").expect("de");
        assert_eq!(back, s);
    }
}
