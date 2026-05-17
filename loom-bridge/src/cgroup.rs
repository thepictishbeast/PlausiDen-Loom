//! cgroup-v2 write spec for per-tenant resource ceilings.
//!
//! T46 cycle 5 (advances #598). Renders [`ResourceCeilings`] into
//! the list of sysfs file writes that pin a tenant's cgroup to
//! the spec, but does NOT execute the writes. The Linux-only
//! executor lives behind the `russh-transport` feature.
//!
//! cgroup-v2 layout (Linux ≥4.5, unified hierarchy):
//!
//!   /sys/fs/cgroup/loom-bridge/<tenant>/
//!     cpu.max         "<quota> <period>"   200000 100000 = 2 cores
//!     memory.max      "<bytes>"            1073741824 = 1 GiB
//!     pids.max        "<count>"            64
//!     cgroup.procs    "<pid>"              join the cgroup
//!
//! Splitting render ↔ write means:
//!   - the spec is testable on every platform
//!   - dry-run mode prints the exact writes that would happen
//!   - audit log records the cgroup config per session

use crate::{ResourceCeilings, tenant::TenantId};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// One sysfs file write the executor must perform to apply the
/// per-tenant cgroup limits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CgroupWrite {
    /// Absolute path under /sys/fs/cgroup/<bridge>/<tenant>/.
    pub path: PathBuf,
    /// Raw bytes to write. Always a trailing newline included
    /// because the kernel's cgroup files expect line-oriented input.
    pub value: String,
}

/// cgroup parent directory used for loom-bridge. The executor
/// creates `/sys/fs/cgroup/<this>/<tenant>/` per tenant.
pub const CGROUP_PARENT: &str = "loom-bridge";

/// Default cgroup-v2 mount root on a standard Linux system.
pub const CGROUP_ROOT: &str = "/sys/fs/cgroup";

/// CPU period used to derive the quota from the percent ceiling.
/// 100ms = standard cfs_period. Quota = `percent / 100 * period`,
/// so `cpu_percent=200` (two cores) → quota `200_000` µs.
const CPU_PERIOD_US: u64 = 100_000;

/// Render the cgroup write list for one tenant. Caller chooses
/// the cgroup root (default: [`CGROUP_ROOT`]) so tests can pin a
/// tempdir.
#[must_use]
pub fn render_cgroup_writes(
    tenant: &TenantId,
    ceilings: &ResourceCeilings,
    cgroup_root: &str,
) -> Vec<CgroupWrite> {
    let base = PathBuf::from(cgroup_root)
        .join(CGROUP_PARENT)
        .join(tenant.as_str());

    // cpu.max: "<quota> <period>".
    let quota = (u64::from(ceilings.cpu_percent) * CPU_PERIOD_US) / 100;
    let cpu_max = format!("{quota} {CPU_PERIOD_US}\n");

    // memory.max: <bytes>.
    let memory_bytes = u64::from(ceilings.memory_mib) * 1024 * 1024;
    let memory_max = format!("{memory_bytes}\n");

    // pids.max: <count>.
    let pids_max = format!("{}\n", ceilings.pids_max);

    vec![
        CgroupWrite {
            path: base.join("cpu.max"),
            value: cpu_max,
        },
        CgroupWrite {
            path: base.join("memory.max"),
            value: memory_max,
        },
        CgroupWrite {
            path: base.join("pids.max"),
            value: pids_max,
        },
    ]
}

/// Render the cgroup-attach write for one process. Pairs with
/// [`render_cgroup_writes`]; the executor calls this AFTER the
/// cgroup is created + limits applied to move a pid into it.
#[must_use]
pub fn render_cgroup_attach(tenant: &TenantId, pid: u32, cgroup_root: &str) -> CgroupWrite {
    let path = PathBuf::from(cgroup_root)
        .join(CGROUP_PARENT)
        .join(tenant.as_str())
        .join("cgroup.procs");
    CgroupWrite {
        path,
        value: format!("{pid}\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tid() -> TenantId {
        TenantId::new("acme").unwrap()
    }

    #[test]
    fn default_ceilings_render_one_core_1_gig_64_pids() {
        let writes = render_cgroup_writes(&tid(), &ResourceCeilings::default(), CGROUP_ROOT);
        assert_eq!(writes.len(), 3);
        assert_eq!(
            writes[0].path,
            PathBuf::from("/sys/fs/cgroup/loom-bridge/acme/cpu.max")
        );
        // 100 percent → 100_000 quota
        assert_eq!(writes[0].value, "100000 100000\n");
        assert_eq!(writes[1].value, format!("{}\n", 1024 * 1024 * 1024));
        assert_eq!(writes[2].value, "64\n");
    }

    #[test]
    fn two_cores_doubles_quota() {
        let ceilings = crate::ResourceCeilingsBuilder::new()
            .cpu_percent(200)
            .unwrap()
            .build();
        let writes = render_cgroup_writes(&tid(), &ceilings, CGROUP_ROOT);
        assert_eq!(writes[0].value, "200000 100000\n");
    }

    #[test]
    fn quarter_core_renders_25_quota() {
        let ceilings = crate::ResourceCeilingsBuilder::new()
            .cpu_percent(25)
            .unwrap()
            .build();
        let writes = render_cgroup_writes(&tid(), &ceilings, CGROUP_ROOT);
        assert_eq!(writes[0].value, "25000 100000\n");
    }

    #[test]
    fn memory_renders_as_bytes() {
        let ceilings = crate::ResourceCeilingsBuilder::new()
            .memory_mib(512)
            .unwrap()
            .build();
        let writes = render_cgroup_writes(&tid(), &ceilings, CGROUP_ROOT);
        let memory = &writes[1];
        assert_eq!(memory.value, format!("{}\n", 512 * 1024 * 1024));
    }

    #[test]
    fn pids_renders_as_count() {
        let ceilings = crate::ResourceCeilingsBuilder::new()
            .pids_max(256)
            .unwrap()
            .build();
        let writes = render_cgroup_writes(&tid(), &ceilings, CGROUP_ROOT);
        assert_eq!(writes[2].value, "256\n");
    }

    #[test]
    fn paths_use_tenant_id_subdir() {
        let widgets = TenantId::new("widgets-co").unwrap();
        let writes = render_cgroup_writes(&widgets, &ResourceCeilings::default(), CGROUP_ROOT);
        for w in &writes {
            assert!(
                w.path
                    .to_string_lossy()
                    .contains("/loom-bridge/widgets-co/")
            );
        }
    }

    #[test]
    fn custom_root_is_honored_for_test_isolation() {
        let writes = render_cgroup_writes(&tid(), &ResourceCeilings::default(), "/tmp/test-cg");
        assert_eq!(
            writes[0].path,
            PathBuf::from("/tmp/test-cg/loom-bridge/acme/cpu.max")
        );
    }

    #[test]
    fn attach_renders_cgroup_procs_with_pid() {
        let w = render_cgroup_attach(&tid(), 12345, CGROUP_ROOT);
        assert_eq!(
            w.path,
            PathBuf::from("/sys/fs/cgroup/loom-bridge/acme/cgroup.procs")
        );
        assert_eq!(w.value, "12345\n");
    }

    #[test]
    fn every_value_ends_with_newline() {
        let writes = render_cgroup_writes(&tid(), &ResourceCeilings::default(), CGROUP_ROOT);
        for w in &writes {
            assert!(
                w.value.ends_with('\n'),
                "{} missing trailing newline",
                w.value
            );
        }
    }

    #[test]
    fn writes_round_trip_through_json() {
        let writes = render_cgroup_writes(&tid(), &ResourceCeilings::default(), CGROUP_ROOT);
        let j = serde_json::to_string(&writes).unwrap();
        let back: Vec<CgroupWrite> = serde_json::from_str(&j).unwrap();
        assert_eq!(back, writes);
    }
}
