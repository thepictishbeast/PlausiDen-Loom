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
}
