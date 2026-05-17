//! Per-tenant resource ceilings.
//!
//! Each tenant gets a cgroup with hard ceilings on CPU and memory.
//! This module owns the typed representation + validation; the actual
//! cgroup write lands in the `russh-transport` feature behind a Linux
//! cgroup-v2 backend. The split keeps the type model unit-testable on
//! every platform (CI runs on ubuntu-latest but the lean tests run
//! even on macOS / Windows developer machines).

use serde::{Deserialize, Serialize};

/// Hard ceilings for one tenant's cgroup.
///
/// Built via [`ResourceCeilingsBuilder`] which validates on every
/// setter. Construction never fails silently — invariants live in
/// the type system, not in runtime asserts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ResourceCeilings {
    /// CPU shares as a percentage (1..=400). 100 = one core. The
    /// cgroup-v2 backend translates to `cpu.max <quota> <period>`.
    pub cpu_percent: u16,
    /// Memory ceiling in MiB (16..=8192). The cgroup-v2 backend
    /// writes `memory.max <bytes>`.
    pub memory_mib: u32,
    /// Max processes/threads (1..=512). Writes `pids.max`.
    pub pids_max: u16,
}

impl Default for ResourceCeilings {
    /// Sane starter ceilings for a small per-tenant Claude session:
    /// 100% of one CPU, 1 GiB RAM, 64 processes.
    fn default() -> Self {
        Self {
            cpu_percent: 100,
            memory_mib: 1024,
            pids_max: 64,
        }
    }
}

/// Fluent builder with per-setter validation.
#[derive(Debug, Default, Clone, Copy)]
pub struct ResourceCeilingsBuilder {
    inner: ResourceCeilings,
}

impl ResourceCeilingsBuilder {
    /// Start from defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: ResourceCeilings::default(),
        }
    }

    /// Set CPU percent ceiling.
    ///
    /// # Errors
    ///
    /// Returns [`ResourceCeilingsError::CpuOutOfRange`] when the value
    /// is outside `1..=400`.
    pub fn cpu_percent(mut self, n: u16) -> Result<Self, ResourceCeilingsError> {
        if !(1..=400).contains(&n) {
            return Err(ResourceCeilingsError::CpuOutOfRange(n));
        }
        self.inner.cpu_percent = n;
        Ok(self)
    }

    /// Set memory ceiling (MiB).
    ///
    /// # Errors
    ///
    /// Returns [`ResourceCeilingsError::MemoryOutOfRange`] when the
    /// value is outside `16..=8192`.
    pub fn memory_mib(mut self, n: u32) -> Result<Self, ResourceCeilingsError> {
        if !(16..=8192).contains(&n) {
            return Err(ResourceCeilingsError::MemoryOutOfRange(n));
        }
        self.inner.memory_mib = n;
        Ok(self)
    }

    /// Set pids ceiling.
    ///
    /// # Errors
    ///
    /// Returns [`ResourceCeilingsError::PidsOutOfRange`] when the
    /// value is outside `1..=512`.
    pub fn pids_max(mut self, n: u16) -> Result<Self, ResourceCeilingsError> {
        if !(1..=512).contains(&n) {
            return Err(ResourceCeilingsError::PidsOutOfRange(n));
        }
        self.inner.pids_max = n;
        Ok(self)
    }

    /// Finalize.
    #[must_use]
    pub fn build(self) -> ResourceCeilings {
        self.inner
    }
}

/// Validation errors for resource ceilings.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ResourceCeilingsError {
    /// cpu_percent outside 1..=400.
    #[error("cpu_percent must be 1..=400 (1=under-share, 100=one core, 400=four cores), got {0}")]
    CpuOutOfRange(u16),
    /// memory_mib outside 16..=8192.
    #[error("memory_mib must be 16..=8192 MiB, got {0}")]
    MemoryOutOfRange(u32),
    /// pids_max outside 1..=512.
    #[error("pids_max must be 1..=512, got {0}")]
    PidsOutOfRange(u16),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_one_core_one_gig() {
        let r = ResourceCeilings::default();
        assert_eq!(r.cpu_percent, 100);
        assert_eq!(r.memory_mib, 1024);
        assert_eq!(r.pids_max, 64);
    }

    #[test]
    fn builder_accepts_in_range_values() {
        let r = ResourceCeilingsBuilder::new()
            .cpu_percent(200)
            .unwrap()
            .memory_mib(2048)
            .unwrap()
            .pids_max(128)
            .unwrap()
            .build();
        assert_eq!(r.cpu_percent, 200);
        assert_eq!(r.memory_mib, 2048);
        assert_eq!(r.pids_max, 128);
    }

    #[test]
    fn builder_rejects_zero_cpu() {
        assert!(matches!(
            ResourceCeilingsBuilder::new().cpu_percent(0),
            Err(ResourceCeilingsError::CpuOutOfRange(0))
        ));
    }

    #[test]
    fn builder_rejects_excess_cpu() {
        assert!(matches!(
            ResourceCeilingsBuilder::new().cpu_percent(401),
            Err(ResourceCeilingsError::CpuOutOfRange(401))
        ));
    }

    #[test]
    fn builder_rejects_tiny_memory() {
        assert!(matches!(
            ResourceCeilingsBuilder::new().memory_mib(8),
            Err(ResourceCeilingsError::MemoryOutOfRange(8))
        ));
    }

    #[test]
    fn builder_rejects_excess_memory() {
        assert!(matches!(
            ResourceCeilingsBuilder::new().memory_mib(16_384),
            Err(ResourceCeilingsError::MemoryOutOfRange(16_384))
        ));
    }

    #[test]
    fn builder_rejects_zero_pids() {
        assert!(matches!(
            ResourceCeilingsBuilder::new().pids_max(0),
            Err(ResourceCeilingsError::PidsOutOfRange(0))
        ));
    }

    #[test]
    fn ceilings_serialize_round_trip() {
        let r = ResourceCeilings::default();
        let j = serde_json::to_string(&r).unwrap();
        let back: ResourceCeilings = serde_json::from_str(&j).unwrap();
        assert_eq!(r, back);
    }
}
