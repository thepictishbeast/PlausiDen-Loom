//! End-to-end integration test: russh client → BridgeServer.
//!
//! Stands a `BridgeServer` on `127.0.0.1:0`, connects a real russh
//! client with an ed25519 keypair the tenant registry knows about,
//! authenticates, opens a session channel, and asserts the
//! cycle-5a hello banner bytes arrive on the client side.
//!
//! Runs without bwrap/cgroup/nft/claude — the server is configured
//! with `sandbox_params = None` so it follows the cycle-5d dry-run
//! path (banner + resolve line + close). What this test PROVES:
//!
//!   * ed25519 publickey auth wiring (tenant registry → handler)
//!   * russh server bring-up via `run_on_socket` (the same code
//!     path `BridgeServer::listen` exercises in production)
//!   * channel-open hello banner data arrives intact on the client
//!
//! Not covered (lands in a separate cycle): sandbox_params=Some +
//! prepare/spawn round-trip; that needs a working bwrap+nft on
//! the test host or extensive mocking.

#![cfg(feature = "russh-transport")]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD_NO_PAD;
use ed25519_dalek::SigningKey;
use loom_bridge::resolver::{StaticTenantEntry, StaticTenantResolver};
use loom_bridge::tenant::{Tenant, TenantKey};
use loom_bridge::{BridgeHostKey, TenantId, TenantRegistry, TenantUid};
use rand_core::OsRng;
use russh::ChannelMsg;
use russh::client;
use russh::keys::key::KeyPair;
use russh::server::Server as _;
use tokio::net::TcpListener;

struct AcceptAllServerKeys;

#[async_trait]
impl client::Handler for AcceptAllServerKeys {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::key::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

#[tokio::test]
async fn end_to_end_auth_and_channel_open_hello_banner() {
    // 1. Generate tenant ed25519 keypair.
    let tenant_signing = SigningKey::generate(&mut OsRng);
    let tenant_pubkey_b64 = STANDARD_NO_PAD.encode(tenant_signing.verifying_key().as_bytes());

    // 2. Build the tenant registry with this key bound to "acme".
    let mut tenant = Tenant::new(TenantId::new("acme").expect("tenant id"));
    tenant.keys.push(TenantKey::new(
        tenant_pubkey_b64,
        Some("integration-test".to_owned()),
    ));
    let mut registry = TenantRegistry::empty();
    registry.tenants.insert(tenant.id.clone(), tenant);

    // 3. Build the resolver with an entry pointing at /bin/sh —
    //    the resolver only needs to return SOME ExecSpec for the
    //    cycle-5d resolve line; the test never spawns the binary.
    let mut resolver = StaticTenantResolver::empty();
    resolver.upsert(
        TenantId::new("acme").expect("tenant id"),
        StaticTenantEntry {
            uid: TenantUid::new(1042).expect("uid"),
            claude_binary_path: PathBuf::from("/bin/sh"),
            workdir: PathBuf::from("/tmp"),
        },
    );

    // 4. Bind a TcpListener on a random port; bypass
    //    BridgeServer::listen so the test can capture the bound
    //    addr before spawning the accept loop.
    let listener = TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
        .await
        .expect("bind 127.0.0.1:0");
    let server_addr = listener.local_addr().expect("local_addr");

    // 5. Build the BridgeServer + spawn the accept loop.
    let server_host_key = BridgeHostKey::from_signing_key(SigningKey::generate(&mut OsRng));
    let config = loom_bridge::transport::BridgeServerConfig::new(
        Arc::new(registry),
        server_addr,
        server_host_key,
        Arc::new(resolver),
    );
    let mut server = loom_bridge::transport::BridgeServer::new(config);
    let russh_cfg = Arc::new(server.build_russh_config());
    let server_task = tokio::spawn(async move {
        // Drives the russh accept loop on the bound listener.
        let _ = server.run_on_socket(russh_cfg, &listener).await;
    });

    // Tiny pause so the server's accept loop registers before
    // the client connects. Not strictly necessary on Linux (the
    // listener's backlog catches the SYN) but bounds the flake
    // surface on contended hosts.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 6. Connect via russh client + authenticate as "acme".
    let client_cfg = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(10)),
        ..Default::default()
    });
    let mut session = client::connect(client_cfg, server_addr, AcceptAllServerKeys)
        .await
        .expect("client connect");
    let auth_ok = session
        .authenticate_publickey("acme", Arc::new(KeyPair::Ed25519(tenant_signing)))
        .await
        .expect("authenticate_publickey runs");
    assert!(auth_ok, "ed25519 auth must succeed for a registered key");

    // 7. Open a session channel + drain messages until we see the
    //    hello banner, then disconnect.
    let mut channel = session.channel_open_session().await.expect("open channel");
    let mut accumulated = Vec::<u8>::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(200), channel.wait()).await {
            Ok(Some(ChannelMsg::Data { data })) => {
                accumulated.extend_from_slice(&data);
                if accumulated.windows(4).any(|w| w == b"acme") {
                    break;
                }
            }
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(_) => continue,
        }
    }
    let banner = String::from_utf8_lossy(&accumulated);
    assert!(
        banner.contains("acme"),
        "hello banner must mention the tenant id; got {banner:?}"
    );
    assert!(
        banner.contains("cycle-5a"),
        "hello banner must include the cycle marker; got {banner:?}"
    );

    // 8. Clean up: disconnect + abort the server task.
    let _ = session
        .disconnect(russh::Disconnect::ByApplication, "", "")
        .await;
    server_task.abort();
}
