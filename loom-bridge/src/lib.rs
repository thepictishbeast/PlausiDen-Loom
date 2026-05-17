//! `loom-bridge` — sandboxed per-tenant Claude Code SSH bridge.
//!
//! Task #598 / Loom T46. This crate owns the **identity + capability**
//! half of the bridge: per-tenant ed25519 keys, the routing table from
//! a connecting SSH user to a sandboxed `claude --resume` invocation,
//! and the per-tenant resource ceilings. The **transport** half (the
//! actual `russh::server::Server` impl) lives behind the
//! `russh-transport` feature flag so the base crate stays lean and
//! the dep graph doesn't pull tokio for callers that only need the
//! type model (`loom-cli` ssh-key management today).
//!
//! ## Architecture
//!
//! ```text
//!   web admin chat panel  ─┐
//!                          │ HTTP cookie-session (T43)
//!   loom edit-serve  ──────┤
//!                          │ unix-socket per tenant
//!   loom-bridge daemon  ───┘──→ russh server (T46.3)
//!                                ├─ ed25519 auth (T46.1)
//!                                ├─ tenant cookie ↔ unix uid (T46.4)
//!                                ├─ bubblewrap / systemd-nspawn jail
//!                                ├─ cgroup CPU + memory ceilings
//!                                ├─ egress allowlist: anthropic + GitHub
//!                                └─ exec: `claude --resume <session-id>`
//! ```
//!
//! AVP-2: every public function tested; no `unwrap`/`expect` outside
//! SAFETY-annotated paths; per-tenant secrets zeroized.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod bwrap;
pub mod cgroup;
pub mod exec_spec;
pub mod host_key;
pub mod resource;
pub mod sandbox;
pub mod tenant;

#[cfg(feature = "russh-transport")]
pub mod transport;

pub use exec_spec::{ClaudeExecSpec, ClaudeSessionId, ExecSpecError, TenantUid};
pub use host_key::{BridgeHostKey, HostKeyError};
pub use resource::{ResourceCeilings, ResourceCeilingsBuilder, ResourceCeilingsError};
pub use sandbox::{SandboxLint, SandboxSpec};
pub use tenant::{Tenant, TenantError, TenantId, TenantRegistry};

/// Crate-wide error type at the public surface.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BridgeError {
    /// Tenant-layer failure (unknown tenant, malformed key, etc.).
    #[error("tenant: {0}")]
    Tenant(#[from] TenantError),
    /// Resource-ceiling validation failure.
    #[error("resource ceilings: {0}")]
    Resource(#[from] ResourceCeilingsError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_chain_reaches_through_thiserror() {
        let e: BridgeError = TenantError::UnknownTenant("xyz".into()).into();
        assert!(e.to_string().starts_with("tenant: "));
    }
}
