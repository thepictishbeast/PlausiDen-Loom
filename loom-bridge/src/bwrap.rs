//! Bubblewrap (`bwrap`) argv builder.
//!
//! T46 cycle 4 (advances #598). Renders a [`SandboxSpec`] +
//! [`ResourceCeilings`] into the exact argv list that
//! `bwrap` will be invoked with — but does NOT execute. The
//! executor (which calls `Command::new("bwrap").args(...)`)
//! lives behind the `russh-transport` feature.
//!
//! Splitting render ↔ exec means:
//!   - the argv list is unit-testable on every platform (bwrap
//!     only exists on Linux, but rendering is pure)
//!   - the executor stays small (call into this module then spawn)
//!   - the audit log records the exact argv used per session,
//!     reproducibly, before any process state mutates

use crate::{ResourceCeilings, SandboxSpec};
use std::ffi::OsString;

/// Render the argv list for `bwrap` to apply this sandbox spec
/// to a child process. The returned `Vec` is suitable for
/// `Command::new("bwrap").args(&argv).arg("--").arg(child)…`.
///
/// The argv intentionally does NOT include the trailing `--` +
/// child command — caller appends those so this function can be
/// reused to render the audit-log line.
#[must_use]
pub fn render_bwrap_argv(spec: &SandboxSpec, ceilings: &ResourceCeilings) -> Vec<OsString> {
    let mut a: Vec<OsString> = Vec::new();
    let push = |a: &mut Vec<OsString>, s: &str| a.push(OsString::from(s));

    // Base: drop everything; we'll add back what's needed.
    push(&mut a, "--unshare-all");
    push(&mut a, "--die-with-parent");

    // Read-only /usr + /etc (basic toolchain access; tenants
    // don't need to mutate these).
    push(&mut a, "--ro-bind");
    push(&mut a, "/usr");
    push(&mut a, "/usr");
    push(&mut a, "--ro-bind");
    push(&mut a, "/etc/ssl");
    push(&mut a, "/etc/ssl");
    push(&mut a, "--ro-bind");
    push(&mut a, "/etc/resolv.conf");
    push(&mut a, "/etc/resolv.conf");

    // Symlinks /lib, /lib64, /bin, /sbin → /usr/*
    push(&mut a, "--symlink");
    push(&mut a, "/usr/lib");
    push(&mut a, "/lib");
    push(&mut a, "--symlink");
    push(&mut a, "/usr/lib64");
    push(&mut a, "/lib64");
    push(&mut a, "--symlink");
    push(&mut a, "/usr/bin");
    push(&mut a, "/bin");
    push(&mut a, "--symlink");
    push(&mut a, "/usr/sbin");
    push(&mut a, "/sbin");

    // Mount the tenant's session root read-write.
    push(&mut a, "--bind");
    a.push(spec.session_root.clone().into_os_string());
    a.push(OsString::from(format!("/sites/{}", spec.tenant)));

    // /tmp as a private tmpfs.
    push(&mut a, "--tmpfs");
    push(&mut a, "/tmp");

    // /dev + /proc minimal mounts.
    push(&mut a, "--dev");
    push(&mut a, "/dev");
    push(&mut a, "--proc");
    push(&mut a, "/proc");

    // Network: --unshare-all dropped the net namespace. If the
    // spec has any egress allowlist, restore the net namespace so
    // the firewall (separate per-tenant nftables ruleset, applied
    // by the executor) can filter. If the allowlist is empty, leave
    // the network unshared = total isolation.
    if !spec.egress_allowlist.is_empty() {
        push(&mut a, "--share-net");
    }

    // Subprocess control: bwrap's --new-session creates a new
    // PID namespace, which blocks ptrace into the parent's PID
    // tree. Default-on; turn off only if spec.allow_ptrace.
    if !spec.allow_ptrace {
        push(&mut a, "--new-session");
    }

    // pids.max ceiling is applied via cgroup (separate writer),
    // but bwrap can pre-restrict to a hard floor too. Skip here —
    // the cgroup is the source of truth.
    let _ = ceilings.pids_max;

    // Drop all capabilities except none.
    push(&mut a, "--cap-drop");
    push(&mut a, "ALL");

    // Hostname + chdir.
    push(&mut a, "--hostname");
    a.push(OsString::from(format!("loom-{}", spec.tenant)));
    push(&mut a, "--chdir");
    a.push(OsString::from(format!("/sites/{}", spec.tenant)));

    // Env: pass through only the bare minimum — no env leakage
    // from the parent process.
    push(&mut a, "--clearenv");
    push(&mut a, "--setenv");
    push(&mut a, "HOME");
    a.push(OsString::from(format!("/sites/{}", spec.tenant)));
    push(&mut a, "--setenv");
    push(&mut a, "PATH");
    push(&mut a, "/usr/local/bin:/usr/bin:/bin");
    push(&mut a, "--setenv");
    push(&mut a, "TENANT_ID");
    a.push(OsString::from(spec.tenant.as_str()));

    a
}

/// Convenience: render the argv as a single shell-quoted line for
/// the audit log. NOT to be used to actually exec — pass the argv
/// directly to `Command::args` to avoid shell-injection class bugs.
#[must_use]
pub fn render_bwrap_audit_line(
    spec: &SandboxSpec,
    ceilings: &ResourceCeilings,
    child_argv0: &str,
) -> String {
    let mut out = String::from("bwrap");
    for a in render_bwrap_argv(spec, ceilings) {
        out.push(' ');
        out.push_str(&shell_quote(&a.to_string_lossy()));
    }
    out.push_str(" -- ");
    out.push_str(&shell_quote(child_argv0));
    out
}

/// Conservative shell quoter — wraps in single quotes unless the
/// string is plain alphanumeric+`/`+`.`+`-`+`_`+`:`+`=`. Inside
/// single quotes nothing is interpreted except the closing quote,
/// which we escape via `'\''`.
fn shell_quote(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "/.-_:=".contains(c))
    {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for ch in s.chars() {
        if ch == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tenant::TenantId;
    use std::path::PathBuf;

    fn spec(tenant: &str) -> SandboxSpec {
        SandboxSpec::minimum_privilege(
            TenantId::new(tenant).unwrap(),
            PathBuf::from(format!("/srv/loom/{tenant}")),
        )
    }

    fn ceilings() -> ResourceCeilings {
        ResourceCeilings::default()
    }

    /// Helper: argv contains a flag (single token).
    fn has(argv: &[OsString], flag: &str) -> bool {
        argv.iter().any(|a| a == flag)
    }

    /// Helper: argv has a flag immediately followed by `value`.
    fn has_pair(argv: &[OsString], flag: &str, value: &str) -> bool {
        argv.windows(2).any(|w| w[0] == flag && w[1] == value)
    }

    #[test]
    fn renders_unshare_all() {
        let argv = render_bwrap_argv(&spec("acme"), &ceilings());
        assert!(has(&argv, "--unshare-all"));
        assert!(has(&argv, "--die-with-parent"));
    }

    #[test]
    fn mounts_session_root_at_sites_tenant() {
        let argv = render_bwrap_argv(&spec("acme"), &ceilings());
        assert!(has_pair(&argv, "--bind", "/srv/loom/acme"));
        assert!(argv.iter().any(|a| a == "/sites/acme"));
    }

    #[test]
    fn share_net_when_egress_allowlist_nonempty() {
        // minimum_privilege has anthropic+github → egress allowed
        let argv = render_bwrap_argv(&spec("acme"), &ceilings());
        assert!(has(&argv, "--share-net"));
    }

    #[test]
    fn no_share_net_when_egress_allowlist_empty() {
        let mut s = spec("acme");
        s.egress_allowlist.clear();
        let argv = render_bwrap_argv(&s, &ceilings());
        assert!(!has(&argv, "--share-net"));
    }

    #[test]
    fn new_session_blocks_ptrace_by_default() {
        let argv = render_bwrap_argv(&spec("acme"), &ceilings());
        assert!(has(&argv, "--new-session"));
    }

    #[test]
    fn allow_ptrace_omits_new_session() {
        let mut s = spec("acme");
        s.allow_ptrace = true;
        let argv = render_bwrap_argv(&s, &ceilings());
        assert!(!has(&argv, "--new-session"));
    }

    #[test]
    fn caps_dropped_always() {
        let argv = render_bwrap_argv(&spec("acme"), &ceilings());
        assert!(has_pair(&argv, "--cap-drop", "ALL"));
    }

    #[test]
    fn clearenv_then_setenv_minimal() {
        let argv = render_bwrap_argv(&spec("acme"), &ceilings());
        assert!(has(&argv, "--clearenv"));
        assert!(has_pair(&argv, "--setenv", "HOME"));
        assert!(has_pair(&argv, "--setenv", "PATH"));
        assert!(has_pair(&argv, "--setenv", "TENANT_ID"));
    }

    #[test]
    fn hostname_uses_tenant_id() {
        let argv = render_bwrap_argv(&spec("widgets-co"), &ceilings());
        assert!(argv.iter().any(|a| a == "loom-widgets-co"));
    }

    #[test]
    fn audit_line_is_shell_safe() {
        let line = render_bwrap_audit_line(&spec("acme"), &ceilings(), "claude");
        assert!(line.starts_with("bwrap"));
        assert!(line.contains("--cap-drop"));
        assert!(line.ends_with(" -- claude"));
        // No raw unescaped path special chars
        assert!(!line.contains("--bind /srv/loom/acme\""));
    }

    #[test]
    fn audit_line_quotes_unusual_paths() {
        let mut s = spec("acme");
        s.session_root = PathBuf::from("/srv/it's risky/acme");
        let line = render_bwrap_audit_line(&s, &ceilings(), "claude");
        // single-quote-escape: 'it'\''s risky'
        assert!(line.contains("'\\''"));
    }

    #[test]
    fn shell_quote_passes_simple_strings_through() {
        assert_eq!(shell_quote("hello"), "hello");
        assert_eq!(shell_quote("/usr/bin/claude"), "/usr/bin/claude");
        assert_eq!(shell_quote("TENANT_ID=acme"), "TENANT_ID=acme");
    }

    #[test]
    fn shell_quote_wraps_spaces() {
        assert_eq!(shell_quote("hello world"), "'hello world'");
    }
}
