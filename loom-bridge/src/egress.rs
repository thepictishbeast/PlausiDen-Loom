//! T46.7 — egress allowlist nftables ruleset renderer.
//!
//! Renders a per-tenant nftables ruleset that pins the bridge-jailed
//! `claude --resume` process to a fixed allowlist of outbound
//! destinations. Mirrors the same render-vs-execute split used by
//! `bwrap.rs` and `cgroup.rs`: this module produces the ruleset
//! text + the `nft -f -` argv, and a cycle 5h executor (queued)
//! pipes it into nft on Linux.
//!
//! nftables layout (per tenant):
//!
//! ```nft
//! table inet loom-bridge-<tenant> {
//!     chain output {
//!         type filter hook output priority 0;
//!         policy drop;
//!         oifname "lo" accept;
//!         meta l4proto { tcp, udp } th dport 53 accept;   # DNS
//!         tcp dport 443 ip daddr @loom_<tenant>_allow accept;
//!         tcp dport 443 ip6 daddr @loom_<tenant>_allow6 accept;
//!     }
//!     set loom_<tenant>_allow  { type ipv4_addr; flags interval; elements = { <ips> }; }
//!     set loom_<tenant>_allow6 { type ipv6_addr; flags interval; elements = { <ips> }; }
//! }
//! ```
//!
//! The allowlist resolves hostnames to IPs at executor-time, not
//! render-time — the rendered ruleset's element sets are populated
//! by the executor's resolver pass (queued in cycle 5h). Render-time
//! just produces the table+chain skeleton + the set declarations.
//!
//! SECURITY:
//!   * Policy is DROP-by-default at the OUTPUT chain — every
//!     connection out of the jail is denied unless on the allowlist.
//!   * DNS (UDP 53) is allowed unconditionally so the resolver works;
//!     the application-layer allowlist still gates which IPs we
//!     accept connections to.
//!   * Only TCP/443 is allowed for application traffic — no plaintext
//!     egress. Combined with the bridge's TLS-1.3-only client config,
//!     end-to-end traffic is encrypted.
//!   * `lo` interface is allowed for in-process loopback (e.g., a
//!     local agent talking to a sidecar over a unix socket would also
//!     work, but loopback IP is required for some HTTP clients).
//!
//! AVP-2 invariants: `unsafe_code = "deny"`, pure renderer, no I/O.

use crate::sandbox::SandboxSpec;
use crate::tenant::TenantId;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::process::{Command, Stdio};

/// One rendered nftables ruleset for a tenant + companion `nft`
/// invocation argv. The executor pipes the ruleset into stdin while
/// running the argv.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct NftablesRuleset {
    /// nftables ruleset text, ready to be piped to `nft -f -`.
    pub ruleset: String,
    /// Per-tenant table name (= `loom-bridge-<tenant>`).
    pub table_name: String,
    /// ipv4 allowlist set name.
    pub set4_name: String,
    /// ipv6 allowlist set name.
    pub set6_name: String,
    /// The allowlist hostnames the executor must resolve to IPs at
    /// apply-time. Cloned from the SandboxSpec for forensic-trail
    /// completeness.
    pub allowlist_hosts: Vec<String>,
}

/// Render the nftables ruleset for one tenant from a [`SandboxSpec`].
/// Pure (no I/O); the executor (cycle 5h) writes / pipes it.
///
/// BUG ASSUMPTION: an empty allowlist still emits a table + DROP
/// policy. The result: ZERO egress connectivity for the tenant.
/// Cycle 5h will refuse to apply such a ruleset by default (operator
/// must explicit-opt-in via a flag) because it almost always means
/// the operator forgot to configure the allowlist.
#[must_use]
pub fn render_nftables_ruleset(spec: &SandboxSpec) -> NftablesRuleset {
    let table_name = format!("loom-bridge-{}", spec.tenant);
    let set4_name = format!("loom_{}_allow", sanitize_set(&spec.tenant));
    let set6_name = format!("loom_{}_allow6", sanitize_set(&spec.tenant));

    let mut buf = String::with_capacity(512);
    // table line — `inet` family handles both IPv4 + IPv6 in one
    // chain (vs separate `ip` + `ip6` tables) for simpler reasoning.
    buf.push_str(&format!("table inet {table_name} {{\n"));

    // ipv4 + ipv6 sets — declared first so the chain can reference.
    buf.push_str(&format!(
        "    set {set4_name} {{\n        type ipv4_addr;\n        flags interval;\n    }}\n"
    ));
    buf.push_str(&format!(
        "    set {set6_name} {{\n        type ipv6_addr;\n        flags interval;\n    }}\n"
    ));

    // output chain.
    buf.push_str("    chain output {\n");
    buf.push_str("        type filter hook output priority 0;\n");
    buf.push_str("        policy drop;\n");
    buf.push_str("        oifname \"lo\" accept;\n");
    buf.push_str("        meta l4proto { tcp, udp } th dport 53 accept;\n");
    buf.push_str(&format!(
        "        tcp dport 443 ip daddr @{set4_name} accept;\n"
    ));
    buf.push_str(&format!(
        "        tcp dport 443 ip6 daddr @{set6_name} accept;\n"
    ));
    buf.push_str("    }\n");

    buf.push_str("}\n");

    NftablesRuleset {
        ruleset: buf,
        table_name,
        set4_name,
        set6_name,
        allowlist_hosts: spec.egress_allowlist.clone(),
    }
}

/// The `nft -f -` argv that pairs with the rendered ruleset.
/// Caller spawns `Command::new(argv[0]).args(&argv[1..])` with
/// `stdin=piped` and writes the ruleset to stdin.
#[must_use]
pub fn nft_apply_argv() -> Vec<&'static str> {
    vec!["nft", "-f", "-"]
}

/// Errors raised by the egress executor.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum EgressApplyError {
    /// Could not spawn the configured `nft` (or substitute) binary.
    /// Usually means nftables isn't installed or the binary isn't on PATH.
    #[error("spawn {binary}: {source}")]
    Spawn {
        /// The binary we tried to spawn.
        binary: String,
        /// Underlying I/O failure.
        #[source]
        source: std::io::Error,
    },
    /// Could not write the ruleset bytes to the child's stdin.
    #[error("write ruleset to {binary} stdin: {source}")]
    StdinWrite {
        /// The binary stdin was being piped to.
        binary: String,
        /// Underlying I/O failure.
        #[source]
        source: std::io::Error,
    },
    /// `wait()` on the child process failed.
    #[error("wait on {binary}: {source}")]
    Wait {
        /// The binary we waited on.
        binary: String,
        /// Underlying I/O failure.
        #[source]
        source: std::io::Error,
    },
    /// Child process exited non-zero. nft prints parse errors to
    /// stderr — operators must read the captured stderr to debug.
    #[error("{binary} exited with status {status} (stderr: {stderr_excerpt:?})")]
    NonZeroExit {
        /// The binary that failed.
        binary: String,
        /// Exit status code (None for signal-terminated).
        status: String,
        /// First 200 chars of stderr for log readability.
        stderr_excerpt: String,
    },
}

/// T46.7 / cycle 5i (2026-05-17): apply a rendered nftables
/// ruleset via `nft -f -`. Spawns nft, pipes the ruleset to its
/// stdin, waits for exit. Sync (uses std::process not tokio) so
/// the executor stays in the default-features tree and matches
/// the cgroup executor's shape.
///
/// `binary` is configurable so tests can substitute a stand-in
/// (`/bin/cat` echoes the ruleset to stdout; non-zero exit codes
/// can be exercised via `/bin/false`). Production callers pass
/// `"nft"`.
///
/// BUG ASSUMPTION: caller has CAP_NET_ADMIN, OR the bridge is
/// running under sudo policy that lets it apply nftables tables
/// for its tenants. The function does NOT shell-out — argv-mode
/// only, so a malicious binary path can't inject extra args.
///
/// # Errors
///
/// Returns one of the [`EgressApplyError`] variants. nft parse
/// errors (most common operator-side failure) materialise as
/// `NonZeroExit` with the parser's stderr in `stderr_excerpt`.
pub fn apply_nftables_ruleset(
    binary: &str,
    ruleset: &NftablesRuleset,
) -> Result<(), EgressApplyError> {
    let mut child = Command::new(binary)
        .arg("-f")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| EgressApplyError::Spawn {
            binary: binary.to_owned(),
            source,
        })?;
    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| EgressApplyError::Spawn {
                binary: binary.to_owned(),
                source: std::io::Error::other("piped stdin handle missing"),
            })?;
        stdin
            .write_all(ruleset.ruleset.as_bytes())
            .map_err(|source| EgressApplyError::StdinWrite {
                binary: binary.to_owned(),
                source,
            })?;
    }
    let output = child
        .wait_with_output()
        .map_err(|source| EgressApplyError::Wait {
            binary: binary.to_owned(),
            source,
        })?;
    if !output.status.success() {
        let stderr_full = String::from_utf8_lossy(&output.stderr);
        let stderr_excerpt: String = stderr_full.chars().take(200).collect();
        return Err(EgressApplyError::NonZeroExit {
            binary: binary.to_owned(),
            status: output.status.to_string(),
            stderr_excerpt,
        });
    }
    Ok(())
}

/// Sanitize tenant id for set-name use. nft set names must match
/// `[a-zA-Z_][a-zA-Z0-9_]*`. TenantId is already `[a-z0-9-]` after
/// validation, so we replace `-` with `_`.
fn sanitize_set(id: &TenantId) -> String {
    id.as_str().replace('-', "_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::SandboxSpec;
    use std::path::PathBuf;

    fn spec(tenant: &str) -> SandboxSpec {
        SandboxSpec::minimum_privilege(
            TenantId::new(tenant).unwrap(),
            PathBuf::from(format!("/srv/loom/{tenant}")),
        )
    }

    #[test]
    fn renders_per_tenant_table_name() {
        let r = render_nftables_ruleset(&spec("acme"));
        assert_eq!(r.table_name, "loom-bridge-acme");
        assert!(r.ruleset.contains("table inet loom-bridge-acme {"));
    }

    #[test]
    fn renders_drop_by_default_policy() {
        let r = render_nftables_ruleset(&spec("acme"));
        assert!(
            r.ruleset.contains("policy drop;"),
            "DROP-by-default is the security baseline"
        );
    }

    #[test]
    fn allows_loopback_unconditionally() {
        let r = render_nftables_ruleset(&spec("acme"));
        assert!(r.ruleset.contains("oifname \"lo\" accept;"));
    }

    #[test]
    fn allows_dns_unconditionally() {
        let r = render_nftables_ruleset(&spec("acme"));
        assert!(r.ruleset.contains("dport 53 accept;"));
    }

    #[test]
    fn allows_only_https_for_app_traffic() {
        let r = render_nftables_ruleset(&spec("acme"));
        assert!(
            r.ruleset
                .contains("tcp dport 443 ip daddr @loom_acme_allow accept;")
        );
        assert!(
            r.ruleset
                .contains("tcp dport 443 ip6 daddr @loom_acme_allow6 accept;")
        );
        // No plaintext.
        assert!(!r.ruleset.contains("tcp dport 80"));
    }

    #[test]
    fn renders_both_ipv4_and_ipv6_sets() {
        let r = render_nftables_ruleset(&spec("acme"));
        assert_eq!(r.set4_name, "loom_acme_allow");
        assert_eq!(r.set6_name, "loom_acme_allow6");
        assert!(r.ruleset.contains("set loom_acme_allow {"));
        assert!(r.ruleset.contains("set loom_acme_allow6 {"));
        assert!(r.ruleset.contains("type ipv4_addr;"));
        assert!(r.ruleset.contains("type ipv6_addr;"));
    }

    #[test]
    fn sanitizes_hyphenated_tenant_in_set_names() {
        // nft set names can't contain '-'; tenant 'widgets-co' must
        // become 'widgets_co' in the set name even though the table
        // name preserves it (inet table names allow '-').
        let r = render_nftables_ruleset(&spec("widgets-co"));
        assert_eq!(r.set4_name, "loom_widgets_co_allow");
        assert_eq!(r.set6_name, "loom_widgets_co_allow6");
        assert_eq!(r.table_name, "loom-bridge-widgets-co");
    }

    #[test]
    fn captures_allowlist_hosts_in_struct() {
        let r = render_nftables_ruleset(&spec("acme"));
        // SandboxSpec::minimum_privilege default
        assert!(r.allowlist_hosts.iter().any(|h| h == "api.anthropic.com"));
        assert!(r.allowlist_hosts.iter().any(|h| h == "github.com"));
    }

    #[test]
    fn empty_allowlist_still_renders_drop_policy() {
        let mut s = spec("acme");
        s.egress_allowlist.clear();
        let r = render_nftables_ruleset(&s);
        assert!(r.ruleset.contains("policy drop;"));
        assert_eq!(r.allowlist_hosts.len(), 0);
        // The rendered chain still has the @set4/@set6 accept lines,
        // but the sets are empty → no IP matches → effective deny.
        // The executor (cycle 5h) will warn before applying.
    }

    #[test]
    fn argv_is_nft_pipe_form() {
        assert_eq!(nft_apply_argv(), vec!["nft", "-f", "-"]);
    }

    #[test]
    fn ruleset_serde_round_trips() {
        let r = render_nftables_ruleset(&spec("acme"));
        let j = serde_json::to_string(&r).expect("ser");
        let back: NftablesRuleset = serde_json::from_str(&j).expect("de");
        assert_eq!(back, r);
    }

    #[test]
    fn no_plaintext_egress_anywhere_in_ruleset() {
        // SUPERSOCIETY pin: any future refactor that accidentally
        // adds `dport 80` or `dport 8080` etc. for application
        // traffic BREAKS this test. Keep the no-plaintext invariant
        // grep-able.
        let r = render_nftables_ruleset(&spec("acme"));
        for bad in [
            "dport 80 ",
            "dport 8080 ",
            "dport 3000 ",
            "dport 8000 ",
            "dport 5000 ",
        ] {
            assert!(
                !r.ruleset.contains(bad),
                "ruleset contains plaintext-port allow rule: {bad}"
            );
        }
    }

    #[test]
    fn output_chain_is_filter_hook_priority_zero() {
        // Reading priority 0 explicitly so a future refactor that
        // changes priority (e.g., to -100 to run before another
        // table) gets caught at test time.
        let r = render_nftables_ruleset(&spec("acme"));
        assert!(r.ruleset.contains("type filter hook output priority 0;"));
    }

    // ---------- T46.7 / cycle 5i executor tests ----------

    // Note: the happy-path 'spawn succeeds, stdin pipes, wait exits 0'
    // is covered by `apply_pipes_full_ruleset_bytes` below, which uses
    // a sh wrapper that captures stdin to a file then exits zero. (An
    // earlier cat-based test was removed because `cat -f -` errors —
    // cat doesn't accept the -f flag; the apply_nftables_ruleset
    // function always passes -f, so any stand-in must accept that.)

    #[test]
    fn apply_non_zero_exit_returns_non_zero_exit_error() {
        // REGRESSION-GUARD (2026-05-17): an earlier version of this
        // test pointed at `/bin/false`, which doesn't read stdin and
        // exits IMMEDIATELY. The kernel buffers our 500-byte stdin
        // write differently on different boxes — on the CI runner
        // (cgroup-pressured) the child closed its end of the pipe
        // before our write completed, yielding EPIPE (StdinWrite
        // error) instead of the documented NonZeroExit. Both are
        // legitimate "this command failed to apply the ruleset"
        // outcomes, but the test was over-specified to NonZeroExit.
        //
        // Fix: use a sh wrapper that explicitly drains stdin THEN
        // exits non-zero. No race — the child consumes everything
        // we write before reaching `exit 2`.
        if !std::path::Path::new("/bin/sh").exists() {
            return;
        }
        let tmp = tempfile::tempdir().expect("tmp");
        let wrapper = fresh_non_zero_exit_wrapper_local(&tmp);
        let r = render_nftables_ruleset(&spec("acme"));
        let err = apply_nftables_ruleset(wrapper.to_str().unwrap(), &r)
            .expect_err("wrapper exits non-zero");
        match err {
            EgressApplyError::NonZeroExit { .. } => { /* expected */ }
            other => panic!("expected NonZeroExit, got {other:?}"),
        }
    }

    fn fresh_non_zero_exit_wrapper_local(tmp: &tempfile::TempDir) -> std::path::PathBuf {
        let path = tmp.path().join("nft-fail.sh");
        std::fs::write(&path, "#!/bin/sh\ncat >/dev/null; exit 2\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
        path
    }

    #[test]
    fn apply_with_missing_binary_returns_spawn_error() {
        let r = render_nftables_ruleset(&spec("acme"));
        let err =
            apply_nftables_ruleset("/this/does/not/exist/at/all/nft-deliberately-missing", &r)
                .expect_err("missing binary fails to spawn");
        assert!(matches!(err, EgressApplyError::Spawn { .. }));
    }

    #[test]
    fn apply_stderr_excerpt_capped_at_200_chars() {
        // Use /bin/sh -c 'echo <long stderr>; exit 1' shape... but
        // apply_nftables_ruleset doesn't take arbitrary args. Inline:
        // write a small wrapper to a tempfile that emits long stderr.
        if !std::path::Path::new("/bin/sh").exists() {
            return;
        }
        let tmp = tempfile::tempdir().expect("tmp");
        let wrapper = tmp.path().join("emit_long_stderr.sh");
        // -f - causes sh to read script from stdin → ignore. Spit
        // a long stderr message and exit 1.
        std::fs::write(
            &wrapper,
            "#!/bin/sh\ncat >/dev/null; echo \"$(yes 'x' | head -c 500)\" 1>&2; exit 1\n",
        )
        .expect("write wrapper");
        let mut perms = std::fs::metadata(&wrapper).unwrap().permissions();
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o755);
        std::fs::set_permissions(&wrapper, perms).expect("chmod");
        let r = render_nftables_ruleset(&spec("acme"));
        let err = apply_nftables_ruleset(wrapper.to_str().unwrap(), &r)
            .expect_err("wrapper exits non-zero");
        if let EgressApplyError::NonZeroExit { stderr_excerpt, .. } = err {
            assert!(
                stderr_excerpt.chars().count() <= 200,
                "stderr_excerpt not capped, got {} chars",
                stderr_excerpt.chars().count()
            );
        } else {
            panic!("expected NonZeroExit");
        }
    }

    #[test]
    fn apply_pipes_full_ruleset_bytes() {
        // Verify the ruleset actually reaches stdin by sending it
        // through `tee` to a known file, then reading it back.
        if !std::path::Path::new("/usr/bin/tee").exists()
            && !std::path::Path::new("/bin/tee").exists()
        {
            return;
        }
        let tee = if std::path::Path::new("/usr/bin/tee").exists() {
            "/usr/bin/tee"
        } else {
            "/bin/tee"
        };
        let tmp = tempfile::tempdir().expect("tmp");
        let _out = tmp.path().join("nft.stdin.captured");
        // tee with -f - would write to "-f -" path; not what we want.
        // Easier: use a sh wrapper that captures stdin to a known file.
        if !std::path::Path::new("/bin/sh").exists() {
            return;
        }
        let cap = tmp.path().join("captured");
        let wrapper = tmp.path().join("capture.sh");
        std::fs::write(
            &wrapper,
            format!("#!/bin/sh\ncat > {} ; exit 0\n", cap.to_string_lossy()),
        )
        .expect("write wrapper");
        let mut perms = std::fs::metadata(&wrapper).unwrap().permissions();
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o755);
        std::fs::set_permissions(&wrapper, perms).expect("chmod");
        let r = render_nftables_ruleset(&spec("acme"));
        apply_nftables_ruleset(wrapper.to_str().unwrap(), &r).expect("wrapper exits 0");
        let got = std::fs::read_to_string(&cap).expect("captured");
        assert_eq!(got, r.ruleset, "byte-mismatch on piped stdin");
        let _ = tee;
    }
}
