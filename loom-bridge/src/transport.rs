//! Russh transport — feature-gated SSH server impl.
//!
//! T46 cycle 3+ (advances #598). Behind the `russh-transport`
//! feature so the default workspace build stays tokio-free.
//!
//! Cycle 2 (this commit) lands only this placeholder so cargo fmt
//! / cargo doc don't complain about the mod declaration in lib.rs.
//! Cycle 3 fills in the `russh::server::Server` impl that:
//!   1. accepts inbound SSH
//!   2. authenticates via the [`crate::tenant::TenantRegistry`]
//!   3. forks a `claude --resume <session>` under the tenant's uid
//!   4. proxies stdin/stdout/stderr over the SSH channel
//!   5. applies the [`crate::resource::ResourceCeilings`] cgroup

#![cfg(feature = "russh-transport")]

/// Marker type for the as-yet-unimplemented transport.
///
/// Owning a `BridgeServer` is meaningless until cycle 3 — exists so
/// callers can wire imports in advance of the implementation.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct BridgeServer;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_type_constructs() {
        let _ = BridgeServer::default();
    }
}
