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

    // 7. Open a session channel + drain ALL messages until the
    //    server-side close. The cycle-5d handler sends three
    //    frames in order:
    //      a. hello banner    ("loom-bridge cycle-5a: hello, tenant=acme.…")
    //      b. resolve line    ("exec-spec: uid=1042, argv=[…]")
    //      c. close
    //    We accumulate everything until ChannelMsg::Close (or the
    //    overall deadline) so a single assertion block can pin all
    //    three pieces.
    let mut channel = session.channel_open_session().await.expect("open channel");
    let mut accumulated = Vec::<u8>::new();
    let mut saw_close = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(200), channel.wait()).await {
            Ok(Some(ChannelMsg::Data { data })) => accumulated.extend_from_slice(&data),
            Ok(Some(ChannelMsg::Close)) | Ok(Some(ChannelMsg::Eof)) => {
                saw_close = true;
                break;
            }
            Ok(Some(_)) => {}
            Ok(None) => {
                // Channel torn down without an explicit Close/Eof
                // message — russh may surface the server-side close
                // as `None`. Either signals the channel is over.
                saw_close = true;
                break;
            }
            Err(_) => continue,
        }
    }
    let banner = String::from_utf8_lossy(&accumulated);
    // cycle-5a hello banner
    assert!(
        banner.contains("acme"),
        "hello banner must mention the tenant id; got {banner:?}"
    );
    assert!(
        banner.contains("cycle-5a"),
        "hello banner must include the cycle marker; got {banner:?}"
    );
    // cycle-5d resolve line
    assert!(
        banner.contains("exec-spec:"),
        "resolve line must surface the exec-spec marker; got {banner:?}"
    );
    assert!(
        banner.contains("uid=1042"),
        "resolve line must surface the resolved uid; got {banner:?}"
    );
    // cycle-5a channel close (server-side)
    assert!(
        saw_close,
        "server should close the channel after the cycle-5d resolve line; \
         accumulated bytes={banner:?}"
    );

    // 8. Clean up: disconnect + abort the server task.
    let _ = session
        .disconnect(russh::Disconnect::ByApplication, "", "")
        .await;
    server_task.abort();
}

// ---------- cycle 6: sandbox_params=Some end-to-end ----------

/// T46 cycle 6 (2026-05-17): wires the WHOLE cycle 5t/5u pipeline
/// through a real ssh round-trip:
///   russh client → BridgeServer → prepare → spawn → stdout pump
///   → channel data → client read.
///
/// Uses sh-wrappers for `nft` and `bwrap` (via the cycle-5z
/// bwrap_binary override) so the test runs on hosts without
/// either binary. The cgroup writes go to a tempdir.
///
/// Asserts the bwrap-replacement's stdout reaches the russh client
/// — proves the cycle 5t spawn + stdout pump chain works end-to-end.
#[tokio::test]
async fn end_to_end_sandbox_params_some_spawn_round_trip() {
    if !std::path::Path::new("/bin/sh").exists() {
        return;
    }
    use loom_bridge::BridgeSandboxParams;
    use loom_bridge::ResourceCeilings;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    // --- 1. Tenant identity + key ---
    let tenant_signing = SigningKey::generate(&mut OsRng);
    let tenant_pubkey_b64 = STANDARD_NO_PAD.encode(tenant_signing.verifying_key().as_bytes());
    let mut tenant = Tenant::new(TenantId::new("acme").expect("tenant id"));
    tenant.keys.push(TenantKey::new(
        tenant_pubkey_b64,
        Some("integration-test-cycle-6".to_owned()),
    ));
    let mut registry = TenantRegistry::empty();
    registry.tenants.insert(tenant.id.clone(), tenant);

    // --- 2. Resolver pointing at a wrapper script ---
    //     Doesn't matter what the resolver returns for the
    //     binary path — the bwrap-wrapper ignores all its args.
    let mut resolver = StaticTenantResolver::empty();
    resolver.upsert(
        TenantId::new("acme").expect("tenant id"),
        StaticTenantEntry {
            uid: TenantUid::new(1042).expect("uid"),
            claude_binary_path: PathBuf::from("/bin/echo"),
            workdir: PathBuf::from("/tmp"),
        },
    );

    // --- 3. Sandbox-template params: tempdir cgroup +
    //        sh-wrapper nft + sh-wrapper bwrap ---
    let tmp = tempfile::tempdir().expect("tempdir");

    let nft_wrapper = tmp.path().join("nft-stub");
    {
        let mut f = std::fs::File::create(&nft_wrapper).expect("create nft-stub");
        writeln!(f, "#!/bin/sh\ncat >/dev/null\nexit 0").expect("write nft-stub");
        let mut perms = std::fs::metadata(&nft_wrapper)
            .expect("stat nft-stub")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&nft_wrapper, perms).expect("chmod nft-stub");
    }

    let bwrap_wrapper = tmp.path().join("bwrap-stub");
    {
        let mut f = std::fs::File::create(&bwrap_wrapper).expect("create bwrap-stub");
        // Ignore all bwrap-args + the claude tail; just print a
        // recognisable marker + exit. The stdout pump should
        // forward this to the russh channel as Channel::data().
        writeln!(f, "#!/bin/sh\necho 'sandbox-stub: hello from $0'\nexit 0")
            .expect("write bwrap-stub");
        let mut perms = std::fs::metadata(&bwrap_wrapper)
            .expect("stat bwrap-stub")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&bwrap_wrapper, perms).expect("chmod bwrap-stub");
    }

    let cgroup_root = tmp.path().join("cgroup");
    std::fs::create_dir_all(&cgroup_root).expect("mkdir cgroup tempdir");

    let sandbox_root = tmp.path().join("sandbox");
    std::fs::create_dir_all(&sandbox_root).expect("mkdir sandbox tempdir");

    let params = BridgeSandboxParams::new(sandbox_root, ResourceCeilings::default())
        .expect("build params")
        .with_cgroup_root(cgroup_root.to_string_lossy().into_owned())
        .expect("override cgroup")
        .with_nft_binary(nft_wrapper.to_string_lossy().into_owned())
        .expect("override nft")
        .with_bwrap_binary(bwrap_wrapper.to_string_lossy().into_owned())
        .expect("override bwrap");

    // --- 4. Bind + spawn server ---
    let listener = TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
        .await
        .expect("bind");
    let server_addr = listener.local_addr().expect("local_addr");
    let server_host_key = BridgeHostKey::from_signing_key(SigningKey::generate(&mut OsRng));
    let config = loom_bridge::transport::BridgeServerConfig::new(
        Arc::new(registry),
        server_addr,
        server_host_key,
        Arc::new(resolver),
    )
    .with_sandbox_params(params);
    let mut server = loom_bridge::transport::BridgeServer::new(config);
    let russh_cfg = Arc::new(server.build_russh_config());
    let server_task = tokio::spawn(async move {
        let _ = server.run_on_socket(russh_cfg, &listener).await;
    });
    tokio::time::sleep(Duration::from_millis(50)).await;

    // --- 5. Connect + auth ---
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
        .expect("authenticate runs");
    assert!(auth_ok, "auth must succeed");

    // --- 6. Open channel + drain ---
    let mut channel = session.channel_open_session().await.expect("open channel");
    let mut accumulated = Vec::<u8>::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(200), channel.wait()).await {
            Ok(Some(ChannelMsg::Data { data }))
            | Ok(Some(ChannelMsg::ExtendedData { data, .. })) => {
                accumulated.extend_from_slice(&data);
                if accumulated.windows(13).any(|w| w == b"sandbox-stub:") {
                    // The bwrap-wrapper's output landed; the spawn
                    // + pump chain works. We can stop early.
                    break;
                }
            }
            Ok(Some(ChannelMsg::Close)) | Ok(Some(ChannelMsg::Eof)) => break,
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(_) => continue,
        }
    }
    let banner = String::from_utf8_lossy(&accumulated);

    // The cycle 5q prepare banner should appear regardless of
    // whether the spawn worked.
    assert!(
        banner.contains("launch: prepared"),
        "cycle 5q prepare banner must appear; got {banner:?}"
    );
    // The cycle 5t spawn + stdout pump should forward the
    // bwrap-stub's output to the channel.
    assert!(
        banner.contains("sandbox-stub:"),
        "cycle 5t stdout pump must forward bwrap-stub output to the channel; got {banner:?}"
    );

    // --- 7. Cleanup ---
    let _ = session
        .disconnect(russh::Disconnect::ByApplication, "", "")
        .await;
    server_task.abort();
}
