//! Russh transport — feature-gated SSH server scaffold.
//!
//! T46 cycle 3 (advances #598). Behind the `russh-transport`
//! feature so the default workspace build stays tokio-free.
//!
//! Cycle 2 landed the typed tenant + resource-ceiling model.
//! Cycle 3 (this commit) lands:
//!   * [`BridgeServerConfig`] — what the server needs to run
//!   * [`BridgeHandler`] — `russh::server::Handler` impl that
//!     enforces ed25519-ONLY publickey auth and looks the offered
//!     key up in the [`crate::tenant::TenantRegistry`]
//!   * [`BridgeServer`] — `russh::server::Server` impl that mints
//!     a fresh handler per inbound connection
//!   * [`AuthOutcome`] — typed audit record of every auth attempt
//!   * [`pubkey_to_base64_no_pad`] — helper translating a russh
//!     `PublicKey` to the wire-shape the tenant registry stores
//!
//! Cycle 4 will land: ed25519 host-key loading + validation, the
//! `BridgeServer::listen` async entry point, and channel-open →
//! `claude --resume` exec under the resolved tenant's uid.
//!
//! ## Defence-in-depth note (Marvin Attack / RUSTSEC-2023-0071)
//!
//! `russh-keys` 0.45 ships with the `rsa` crate as an unconditional
//! transitive dep; we've accepted the residual risk in deny.toml +
//! `.cargo/audit.toml` *on the basis that no RSA codepath is ever
//! exercised at runtime*. The ed25519-only enforcement here is the
//! technical bind that backs that SHIP-DECISION: any non-ed25519
//! key offered for auth is rejected at the russh `Handler` layer
//! BEFORE russh attempts any RSA verify. The check belongs here,
//! not in deny.toml, because a future cycle that loosens key
//! requirements must FIRST be reviewed against the timing-attack
//! threat model.

#![cfg(feature = "russh-transport")]

use crate::bridge_session::spawn_claude_session;
use crate::exec_spec::ClaudeSessionId;
use crate::host_key::BridgeHostKey;
use crate::resolver::SharedResolver;
use crate::sandbox_params::BridgeSandboxParams;
use crate::spawn_async::{PrepareAsyncError, prepare_blocking_async};
use crate::tenant::{TenantId, TenantRegistry};
use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD_NO_PAD;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

/// Configuration the bridge needs to bring an SSH server up.
///
/// Fields kept on the public surface (vs hidden behind a builder)
/// because every one of them is required and there is no sensible
/// default the loom-cli could supply without the operator's input.
///
/// BUG ASSUMPTION: `registry` is a fully-populated snapshot; mid-flight
/// mutations require an `Arc<RwLock<TenantRegistry>>` swap, which is
/// cycle-4 scope.
#[derive(Debug)]
#[non_exhaustive]
pub struct BridgeServerConfig {
    /// Tenants authorised to connect, keyed by id. Cloned into each
    /// per-connection handler via `Arc` so updates aren't visible
    /// mid-session (deliberate — admin portal must restart to swap
    /// the registry, no auth ambiguity).
    pub registry: Arc<TenantRegistry>,
    /// Address the server should bind.
    pub listen_addr: SocketAddr,
    /// Ed25519 host key. The transport layer is the SOLE consumer;
    /// other crates that need only the tenant model never see it.
    pub host_key: BridgeHostKey,
    /// Resolver from authenticated TenantId → ClaudeExecSpec. The
    /// bridge consults this on every channel-open to know what to
    /// run + as which uid. Trait-object so deployments can swap in
    /// DB-backed / cookie-bridge-backed resolvers without touching
    /// transport.rs.
    pub resolver: SharedResolver,
    /// T46 cycle 5q: sandbox-template parameters. `None` keeps the
    /// pre-cycle-5q behaviour (banner only, no `prepare` call) so
    /// existing tests + the loom-cli's smoke-server keep working.
    /// `Some(params)` enables the per-session `BridgeLaunch::prepare`
    /// call on channel-open + surfaces the audit_argv via the banner.
    pub sandbox_params: Option<BridgeSandboxParams>,
}

impl BridgeServerConfig {
    /// Construct a config from its components.
    ///
    /// BUG ASSUMPTION: caller has already validated `listen_addr`
    /// against any deployment-level allowlist (only-bind-loopback,
    /// only-bind-VPN-range, etc.). The bridge intentionally does
    /// not second-guess the socket choice.
    #[must_use]
    pub fn new(
        registry: Arc<TenantRegistry>,
        listen_addr: SocketAddr,
        host_key: BridgeHostKey,
        resolver: SharedResolver,
    ) -> Self {
        Self {
            registry,
            listen_addr,
            host_key,
            resolver,
            sandbox_params: None,
        }
    }

    /// Attach sandbox-template parameters. With this set, the
    /// bridge will invoke `BridgeLaunch::prepare` on every
    /// channel-open (after resolve) and surface the audit_argv via
    /// the channel banner.
    #[must_use]
    pub fn with_sandbox_params(mut self, params: BridgeSandboxParams) -> Self {
        self.sandbox_params = Some(params);
        self
    }
}

/// Bridge-layer errors. Distinct from `russh::Error` so the loom-cli
/// can match on bridge-specific conditions without depending on
/// russh's types.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BridgeError {
    /// Russh internal error during session / connection setup.
    #[error("russh error: {0}")]
    Russh(String),
    /// Could not bind the configured listen address.
    #[error("bind failed on {addr}: {source}")]
    Bind {
        /// The address we tried to bind.
        addr: SocketAddr,
        /// Underlying I/O failure.
        #[source]
        source: std::io::Error,
    },
}

/// Mapping from `russh::Error` is lossy by design — the bridge does
/// not surface russh internals to its callers; the string form is
/// enough for the loom-cli to log + the operator to grep.
impl From<russh::Error> for BridgeError {
    fn from(e: russh::Error) -> Self {
        Self::Russh(e.to_string())
    }
}

/// Outcome of one publickey auth attempt. Returned to the russh
/// layer indirectly (mapped to `russh::server::Auth`) and also fed
/// to the audit log so we keep a forensic trail of every attempt.
///
/// BUG ASSUMPTION: callers MUST log every outcome. Silent rejects
/// defeat the audit-trail purpose of having this type at all.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuthOutcome {
    /// Key is on the authorised list. Carries the resolved tenant
    /// so downstream stages (channel-open, exec) can pick it up
    /// from the handler state without re-doing the lookup.
    Accept(TenantId),
    /// Auth attempt rejected. `reason` is a stable string the
    /// audit log can group by (e.g., `"non-ed25519-key"`).
    Reject {
        /// Why we rejected (machine-grepable).
        reason: &'static str,
    },
}

/// Convert a russh `PublicKey` to the no-padding base64 wire form
/// the `TenantRegistry` stores its authorised keys as.
///
/// Returns `None` for any non-ed25519 key — the bridge refuses to
/// reason about RSA / ECDSA keys at all (see Marvin Attack note).
///
/// BUG ASSUMPTION: the format is stable across russh-keys 0.45.x.
/// A breaking change in the upstream `ed25519_dalek::VerifyingKey`
/// byte layout would silently invalidate every entry in the on-disk
/// registry. The `pubkey_to_b64_ed25519_round_trips` test below is
/// the canary.
#[must_use]
pub fn pubkey_to_base64_no_pad(pk: &russh::keys::key::PublicKey) -> Option<String> {
    use russh::keys::key::PublicKey;
    match pk {
        PublicKey::Ed25519(vk) => Some(STANDARD_NO_PAD.encode(vk.as_bytes())),
        // Defence in depth: do not even ATTEMPT to b64 anything else.
        // The auth layer must short-circuit before it gets here.
        _ => None,
    }
}

/// Per-connection handler. One `BridgeHandler` per inbound client.
/// Owns an `Arc` to the immutable-for-this-session registry snapshot
/// + remembers which tenant authenticated so cycle-4's channel-open
/// can read it without a second registry walk.
#[derive(Debug)]
pub struct BridgeHandler {
    registry: Arc<TenantRegistry>,
    /// `None` until publickey auth succeeds.
    authenticated_as: Option<TenantId>,
    /// Resolver for tenant → exec-spec lookups at channel-open time.
    resolver: SharedResolver,
    /// T46 cycle 5q: sandbox-template parameters from the server
    /// config. `None` preserves cycle-5d behaviour; `Some(_)` triggers
    /// the `prepare_blocking_async` call on channel-open.
    sandbox_params: Option<BridgeSandboxParams>,
    /// T46 cycle 5u: live child stdin. Populated when
    /// `spawn_session_for_channel` succeeds; consumed by the russh
    /// `data()` handler to forward inbound bytes to the sandboxed
    /// claude. `Arc<Mutex<_>>` because the russh framework calls
    /// `data()` with `&mut self` but our pump tasks (spawned in
    /// cycle 5t) need to coexist with potential future stdin-pump
    /// patterns.
    active_stdin: Option<Arc<tokio::sync::Mutex<tokio::process::ChildStdin>>>,
}

impl BridgeHandler {
    /// Construct a fresh handler for one client.
    #[must_use]
    fn new(
        registry: Arc<TenantRegistry>,
        resolver: SharedResolver,
        sandbox_params: Option<BridgeSandboxParams>,
    ) -> Self {
        Self {
            registry,
            authenticated_as: None,
            resolver,
            sandbox_params,
            active_stdin: None,
        }
    }

    /// Classify an offered key against the registry. Public so unit
    /// tests can exercise it without standing a real russh session.
    ///
    /// BUG ASSUMPTION: the registry is queried as-is; no
    /// retry-on-Arc-update path. Cycle-4 swap behaviour requires
    /// caller-side rebind.
    #[must_use]
    pub fn classify(&self, key: &russh::keys::key::PublicKey) -> AuthOutcome {
        let Some(b64) = pubkey_to_base64_no_pad(key) else {
            return AuthOutcome::Reject {
                reason: "non-ed25519-key",
            };
        };
        match self.registry.tenant_for_key(&b64) {
            Some(t) => AuthOutcome::Accept(t.id.clone()),
            None => AuthOutcome::Reject {
                reason: "unknown-key",
            },
        }
    }

    /// Once cycle 4 lands channel-open + exec, this is what the
    /// downstream stages read to know whose uid to switch to. Public
    /// so callers can assert post-auth invariants in tests.
    #[must_use]
    pub fn tenant(&self) -> Option<&TenantId> {
        self.authenticated_as.as_ref()
    }

    /// T46 cycle 5t (2026-05-17): take a [`crate::spawn::PreparedLaunch`]
    /// and bring up a live bridge session backed by it. Spawns the
    /// sandboxed claude child + two background pump tasks that
    /// forward stdout/stderr to the russh channel, plus a reaper
    /// task that closes the channel when the child exits.
    ///
    /// SECURITY: by the time this returns, the child PID is alive
    /// inside the bwrap+cgroup+nftables sandbox. The reaper task
    /// is the ONLY thing that closes the channel — if it dies
    /// without closing, russh will eventually GC the channel on
    /// connection-tear-down; defence-in-depth says always have a
    /// channel-close path.
    ///
    /// Stdin (inbound channel `data()` → child stdin) lands in
    /// cycle 5u. For now stdin is dropped post-spawn, child sees
    /// EOF immediately, and most claude-like sessions exit quickly.
    async fn spawn_session_for_channel(
        &mut self,
        channel: russh::Channel<russh::server::Msg>,
        session: &mut russh::server::Session,
        prepared: crate::spawn::PreparedLaunch,
        tenant: &TenantId,
    ) -> Result<(), BridgeError> {
        let chan_id = channel.id();
        let handle = session.handle();
        let tenant_log = tenant.clone();
        match spawn_claude_session(prepared) {
            Ok(mut bridge_session) => {
                tracing::info!(tenant = %tenant_log, "session spawned");
                // stdout pump → channel data
                if let Some(mut stdout) = bridge_session.stdout.take() {
                    let h = handle.clone();
                    tokio::spawn(async move {
                        use tokio::io::AsyncReadExt;
                        let mut buf = vec![0u8; 4096];
                        loop {
                            match stdout.read(&mut buf).await {
                                Ok(0) => break,
                                Ok(n) => {
                                    if h.data(chan_id, russh::CryptoVec::from_slice(&buf[..n]))
                                        .await
                                        .is_err()
                                    {
                                        break;
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                    });
                }
                // stderr pump → channel extended_data (ext=1 = SSH stderr)
                if let Some(mut stderr) = bridge_session.stderr.take() {
                    let h = handle.clone();
                    tokio::spawn(async move {
                        use tokio::io::AsyncReadExt;
                        let mut buf = vec![0u8; 4096];
                        loop {
                            match stderr.read(&mut buf).await {
                                Ok(0) => break,
                                Ok(n) => {
                                    if h.extended_data(
                                        chan_id,
                                        1,
                                        russh::CryptoVec::from_slice(&buf[..n]),
                                    )
                                    .await
                                    .is_err()
                                    {
                                        break;
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                    });
                }
                // Cycle 5u (2026-05-17): stash stdin so the russh
                // `data()` handler can forward inbound channel
                // bytes to it. Arc/Mutex because Rust's borrow
                // checker insists `data()` cannot borrow self while
                // an inflight write is pending.
                if let Some(stdin) = bridge_session.stdin.take() {
                    self.active_stdin = Some(Arc::new(tokio::sync::Mutex::new(stdin)));
                }
                // Reaper task: wait + EOF + close.
                if let Some(mut child) = bridge_session.child.take() {
                    let h = handle.clone();
                    let tenant_reap = tenant_log.clone();
                    tokio::spawn(async move {
                        let exit = child.wait().await;
                        tracing::info!(
                            tenant = %tenant_reap,
                            exit = ?exit,
                            "session child exited"
                        );
                        let _ = h.eof(chan_id).await;
                        let _ = h.close(chan_id).await;
                    });
                }
            }
            Err(e) => {
                tracing::warn!(tenant = %tenant_log, error = %e, "session spawn failed");
                let banner = format!("session: SPAWN-FAILED ({e})\n");
                channel
                    .data(banner.as_bytes())
                    .await
                    .map_err(|e| BridgeError::Russh(format!("spawn-fail banner: {e}")))?;
                channel
                    .close()
                    .await
                    .map_err(|e| BridgeError::Russh(format!("spawn-fail close: {e}")))?;
            }
        }
        Ok(())
    }
}

/// T46 cycle 5w (2026-05-17): render the prepare-line banner
/// sent on the channel after `BridgeLaunch::prepare` succeeds.
/// Pure so unit tests can pin the format without standing a real
/// russh session + prepared command pipeline.
///
/// Format (load-bearing for operator log greppability):
///   `launch: prepared, argv_len=N, allowlist=[…], audit_argv=[…]\n`
///
/// `allowlist=` is either `EMPTY (drop-by-default)` (no hosts in
/// the sandbox spec) or `ipv4=A, ipv6=B, failed_hosts=C` from the
/// per-session resolver pass.
#[must_use]
pub fn format_prepare_banner(
    audit_argv: &[String],
    resolved: Option<&crate::egress::ResolvedAllowlist>,
) -> String {
    let allowlist_summary = match resolved {
        Some(r) => format!(
            "ipv4={}, ipv6={}, failed_hosts={}",
            r.ipv4.len(),
            r.ipv6.len(),
            r.failed.len()
        ),
        None => "EMPTY (drop-by-default)".to_owned(),
    };
    format!(
        "launch: prepared, argv_len={}, allowlist=[{}], audit_argv={:?}\n",
        audit_argv.len(),
        allowlist_summary,
        audit_argv
    )
}

/// Render the cycle-5a hello banner sent to a newly opened session
/// channel. Pure function so unit tests can pin the exact bytes
/// without standing a real russh session.
///
/// BUG ASSUMPTION: the banner is a courtesy / forensic marker, not
/// load-bearing for any client-side protocol. Operators reading SSH
/// session logs use it to confirm the bridge is reachable and the
/// tenant resolved correctly.
#[must_use]
pub fn format_hello_banner(tenant: &TenantId) -> Vec<u8> {
    format!(
        "loom-bridge cycle-5a: hello, tenant={tenant}.\n\
         This channel is a minimum-viable session; cycle-5b wires \
         `claude --resume <session-id>` exec under your uid.\n"
    )
    .into_bytes()
}

#[async_trait]
impl russh::server::Handler for BridgeHandler {
    type Error = BridgeError;

    async fn auth_publickey(
        &mut self,
        _user: &str,
        public_key: &russh::keys::key::PublicKey,
    ) -> Result<russh::server::Auth, Self::Error> {
        // SECURITY: enforce ed25519-only at this exact line. Russh
        // would otherwise call into the RSA verify path before we
        // ever see the key — but `classify` returns Reject for
        // non-ed25519 BEFORE any algorithm-specific work runs in
        // our handler. The deny.toml RUSTSEC-2023-0071 ignore is
        // load-bearing on this gate.
        let outcome = self.classify(public_key);
        match outcome {
            AuthOutcome::Accept(tenant) => {
                tracing::info!(tenant = %tenant, "ssh auth accept");
                self.authenticated_as = Some(tenant);
                Ok(russh::server::Auth::Accept)
            }
            AuthOutcome::Reject { reason } => {
                tracing::warn!(reason, "ssh auth reject");
                Ok(russh::server::Auth::Reject {
                    proceed_with_methods: None,
                })
            }
        }
    }

    /// T46 cycle 5a (advances #598): minimal viable session channel
    /// handler. The auth flow + channel flow is end-to-end exercised
    /// here without yet wiring claude / bwrap / cgroup. A session
    /// open from an authenticated tenant returns a hello banner +
    /// closes the channel cleanly; an unauthenticated request is
    /// rejected (defence in depth — russh should never get here
    /// without auth, but the handler enforces the invariant anyway).
    ///
    /// Cycle 5d (this commit): the banner now ALSO reports the
    /// resolved ExecSpec (if the resolver has a mapping) — proves
    /// the cycle-5c TenantResolver wiring is end-to-end live.
    /// Cycle 5e replaces banner+close with the actual
    /// `claude --resume <session-id>` exec (tokio::process::Command).
    async fn channel_open_session(
        &mut self,
        channel: russh::Channel<russh::server::Msg>,
        session: &mut russh::server::Session,
    ) -> Result<bool, Self::Error> {
        let Some(tenant) = self.authenticated_as.clone() else {
            // SECURITY: russh's auth gate runs BEFORE channel_open,
            // so this should be unreachable. Belt + suspenders.
            tracing::error!("channel_open_session without prior publickey auth — refusing");
            return Ok(false);
        };
        tracing::info!(
            tenant = %tenant,
            channel_id = ?channel.id(),
            "ssh channel open"
        );
        // T46 cycle 5d: resolve the tenant against the SharedResolver
        // to confirm a per-tenant ExecSpec is registered. Cycle 5d
        // uses a fixed placeholder session id ("bridge-default") so
        // the resolver path is exercised end-to-end; T46.4 wires
        // the real session id from the admin-portal cookie via a
        // unix-socket lookup.
        let placeholder_sid = ClaudeSessionId::new("bridge-default")
            .map_err(|e| BridgeError::Russh(format!("placeholder session id: {e}")))?;
        let resolve = self.resolver.resolve(&tenant, placeholder_sid);
        let banner = format_hello_banner(&tenant);
        // Send the hello banner first (forensic-trail), then the
        // resolver outcome on a second data frame, then close.
        channel
            .data(banner.as_slice())
            .await
            .map_err(|e| BridgeError::Russh(format!("hello-banner write: {e}")))?;
        let resolve_line = match &resolve {
            Ok(spec) => {
                tracing::info!(
                    tenant = %tenant,
                    uid = spec.uid.as_u32(),
                    "exec spec resolved"
                );
                format!(
                    "exec-spec: uid={}, argv={:?}\n",
                    spec.uid.as_u32(),
                    spec.argv()
                )
            }
            Err(e) => {
                // The tenant authenticated but the operator forgot to
                // register an exec mapping. Surface to both the log
                // AND the channel so the user knows their session
                // can't proceed.
                tracing::warn!(tenant = %tenant, error = %e, "exec resolve failed");
                format!("exec-spec: UNAVAILABLE ({e})\n")
            }
        };
        channel
            .data(resolve_line.as_bytes())
            .await
            .map_err(|e| BridgeError::Russh(format!("resolve-line write: {e}")))?;

        // T46 cycle 5q (advances #598): if the server config has
        // sandbox-template params + the resolve produced a valid
        // ExecSpec, compose the BridgeLaunch + invoke prepare()
        // on the blocking pool. Surface the audit_argv via a
        // third data frame so the operator's session log captures
        // the EXACT argv the bwrap+claude exec would run.
        //
        // Still close the channel (no actual spawn yet) — cycle
        // 5r will replace the close with the real
        // tokio::process::Command::from(prepared.command).spawn()
        // + bidirectional stdio bridging.
        if let (Some(params), Ok(spec)) = (&self.sandbox_params, &resolve) {
            let launch = params.build_launch(spec.clone());
            match prepare_blocking_async(launch).await {
                Ok(prepared) => {
                    tracing::info!(
                        tenant = %tenant,
                        argv_len = prepared.audit_argv.len(),
                        "launch prepared"
                    );
                    // Cycle 5s/5w: allowlist summary + audit_argv
                    // in the channel banner, via the pure helper so
                    // tests can pin the exact format.
                    let banner = format_prepare_banner(
                        &prepared.audit_argv,
                        prepared.resolved_allowlist.as_ref(),
                    );
                    channel
                        .data(banner.as_bytes())
                        .await
                        .map_err(|e| BridgeError::Russh(format!("launch banner: {e}")))?;

                    // Cycle 5t (2026-05-17): actually spawn the
                    // sandboxed claude. Stdout/stderr pump to
                    // channel data/extended_data; reaper closes
                    // the channel when child exits. Stdin pump
                    // (inbound `data()` → child stdin) lands in
                    // cycle 5u — for now stdin is dropped, child
                    // sees EOF immediately and exits quickly.
                    self.spawn_session_for_channel(channel, session, prepared, &tenant)
                        .await?;
                    return Ok(true);
                }
                Err(PrepareAsyncError::Launch(e)) => {
                    tracing::warn!(tenant = %tenant, error = %e, "launch prepare failed");
                    let banner = format!("launch: FAILED ({e})\n");
                    channel
                        .data(banner.as_bytes())
                        .await
                        .map_err(|e| BridgeError::Russh(format!("fail banner: {e}")))?;
                }
                Err(PrepareAsyncError::JoinPanic(msg)) => {
                    tracing::error!(tenant = %tenant, panic_msg = %msg, "launch prepare panicked");
                    let banner = format!("launch: PANICKED ({msg})\n");
                    channel
                        .data(banner.as_bytes())
                        .await
                        .map_err(|e| BridgeError::Russh(format!("panic banner: {e}")))?;
                }
            }
        }

        channel
            .close()
            .await
            .map_err(|e| BridgeError::Russh(format!("hello-banner close: {e}")))?;
        Ok(true)
    }

    /// T46 cycle 5u (2026-05-17): forward inbound channel data to
    /// the sandboxed child's stdin.
    ///
    /// Russh calls this hook for every chunk the client sends after
    /// the channel is open. When `active_stdin` is populated (i.e.,
    /// cycle 5t's spawn_session_for_channel succeeded), each chunk
    /// is written to the child's stdin pipe. A write error is logged
    /// + the stdin handle dropped so subsequent chunks short-circuit.
    ///
    /// When `active_stdin` is None (no sandbox_params, or prepare/
    /// spawn failed), inbound data is silently dropped — preserves
    /// the cycle-5d / cycle-5q dry-run behaviour.
    async fn data(
        &mut self,
        _channel: russh::ChannelId,
        data: &[u8],
        _session: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        if let Some(stdin_arc) = self.active_stdin.clone() {
            use tokio::io::AsyncWriteExt;
            let mut stdin = stdin_arc.lock().await;
            if let Err(e) = stdin.write_all(data).await {
                tracing::warn!(
                    error = %e,
                    bytes = data.len(),
                    "child stdin write failed; dropping handle"
                );
                drop(stdin);
                self.active_stdin = None;
            }
        }
        Ok(())
    }

    /// T46 cycle 5u: client signalled EOF on the channel — drop
    /// our stdin handle so the child sees EOF and can shut down
    /// gracefully (claude --resume will flush + exit).
    async fn channel_eof(
        &mut self,
        _channel: russh::ChannelId,
        _session: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        if self.active_stdin.is_some() {
            tracing::info!("channel eof; dropping child stdin so child sees EOF");
            self.active_stdin = None;
        }
        Ok(())
    }
}

/// Top-level russh `Server`. Hands out fresh `BridgeHandler`s.
///
/// BUG ASSUMPTION: `new_client` runs synchronously on the russh
/// accept loop, so it must stay allocation-cheap. Wrapping the
/// registry in `Arc` is the whole point — clone is O(1).
#[derive(Debug)]
pub struct BridgeServer {
    registry: Arc<TenantRegistry>,
    listen_addr: SocketAddr,
    host_key: BridgeHostKey,
    resolver: SharedResolver,
    sandbox_params: Option<BridgeSandboxParams>,
}

impl BridgeServer {
    /// Construct from a config.
    #[must_use]
    pub fn new(config: BridgeServerConfig) -> Self {
        Self {
            registry: config.registry,
            listen_addr: config.listen_addr,
            host_key: config.host_key,
            resolver: config.resolver,
            sandbox_params: config.sandbox_params,
        }
    }

    /// Borrow the configured listen address (handy for tests + logs).
    #[must_use]
    pub fn listen_addr(&self) -> SocketAddr {
        self.listen_addr
    }

    /// Build a `russh::server::Config` from the host key + bridge
    /// security defaults. Pure (no I/O); the listen() entry consumes
    /// the result.
    ///
    /// Security defaults enforced here:
    ///   * `methods = PUBLICKEY` only — disables password + keyboard-
    ///     interactive at the protocol layer
    ///   * `auth_rejection_time = 2s` — uniform reject delay so
    ///     known/unknown-key timing leaks stay bounded
    ///   * `max_auth_attempts = 3` — defeats trivial enumeration
    ///   * `inactivity_timeout = 60s` — idle sessions reaped fast;
    ///     cycle-5 will pair this with the cgroup CPU ceiling
    ///   * `keys = [Ed25519]` ONLY — no RSA / ECDSA keypair ever
    ///     installed (load-bearing on the Marvin Attack SHIP-DECISION)
    #[must_use]
    pub fn build_russh_config(&self) -> russh::server::Config {
        let mut cfg = russh::server::Config::default();
        cfg.methods = russh::MethodSet::PUBLICKEY;
        cfg.auth_rejection_time = Duration::from_secs(2);
        cfg.auth_rejection_time_initial = Some(Duration::from_secs(2));
        cfg.max_auth_attempts = 3;
        cfg.inactivity_timeout = Some(Duration::from_secs(60));
        cfg.keys = vec![russh::keys::key::KeyPair::Ed25519(
            self.host_key.signing_key().clone(),
        )];
        cfg
    }

    /// Async entry point. Binds the configured `listen_addr` and runs
    /// the russh accept loop indefinitely. Returns when the underlying
    /// `tokio::net::TcpListener` accept loop terminates (typically on
    /// signal-driven shutdown).
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError::Bind`] when the socket can't be bound,
    /// or [`BridgeError::Russh`] when the russh accept loop errors.
    ///
    /// BUG ASSUMPTION: caller has already cap-dropped / chrooted /
    /// applied the privilege-drop they wanted BEFORE invoking
    /// listen() — the bridge doesn't run as root and doesn't try to
    /// shed capabilities itself (that's the runner's responsibility,
    /// e.g., systemd unit `CapabilityBoundingSet=`).
    pub async fn listen(mut self) -> Result<(), BridgeError> {
        use russh::server::Server as _;
        let addr = self.listen_addr;
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|source| BridgeError::Bind { addr, source })?;
        let cfg = Arc::new(self.build_russh_config());
        // run_on_socket borrows the listener and drives the accept
        // loop until the listener errors or the program exits.
        self.run_on_socket(cfg, &listener)
            .await
            .map_err(|e: std::io::Error| BridgeError::Russh(e.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl russh::server::Server for BridgeServer {
    type Handler = BridgeHandler;

    fn new_client(&mut self, _peer_addr: Option<SocketAddr>) -> Self::Handler {
        BridgeHandler::new(
            Arc::clone(&self.registry),
            Arc::clone(&self.resolver),
            self.sandbox_params.clone(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tenant::{Tenant, TenantKey};
    use ed25519_dalek::SigningKey;
    use rand_core::OsRng;
    use russh::server::Server as _;
    use std::net::{IpAddr, Ipv4Addr};
    use subtle::ConstantTimeEq;

    fn fresh_ed25519() -> (
        ed25519_dalek::SigningKey,
        ed25519_dalek::VerifyingKey,
        String,
    ) {
        let sk = SigningKey::generate(&mut OsRng);
        let vk = sk.verifying_key();
        let b64 = STANDARD_NO_PAD.encode(vk.as_bytes());
        (sk, vk, b64)
    }

    fn fresh_host_key() -> BridgeHostKey {
        BridgeHostKey::from_signing_key(SigningKey::generate(&mut OsRng))
    }

    fn empty_resolver() -> SharedResolver {
        Arc::new(crate::resolver::StaticTenantResolver::empty())
    }

    fn registry_with(tenant: &str, key_b64: &str) -> Arc<TenantRegistry> {
        let mut r = TenantRegistry::empty();
        let id = TenantId::new(tenant).unwrap();
        let mut t = Tenant::new(id);
        t.keys.push(TenantKey {
            b64: key_b64.to_owned(),
            label: None,
        });
        r.upsert(t);
        Arc::new(r)
    }

    #[test]
    fn config_constructs() {
        let r = Arc::new(TenantRegistry::empty());
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let cfg = BridgeServerConfig::new(r, addr, fresh_host_key(), empty_resolver());
        assert_eq!(cfg.listen_addr.port(), 0);
    }

    #[test]
    fn pubkey_to_b64_ed25519_round_trips() {
        let (_sk, vk, b64_direct) = fresh_ed25519();
        let pk = russh::keys::key::PublicKey::Ed25519(vk);
        let b64_via = pubkey_to_base64_no_pad(&pk).expect("ed25519 → b64");
        assert_eq!(b64_via, b64_direct);
    }

    #[test]
    fn classify_accepts_known_tenant() {
        let (_sk, vk, b64) = fresh_ed25519();
        let r = registry_with("acme", &b64);
        let h = BridgeHandler::new(r, empty_resolver(), None);
        let pk = russh::keys::key::PublicKey::Ed25519(vk);
        match h.classify(&pk) {
            AuthOutcome::Accept(id) => assert_eq!(id.as_str(), "acme"),
            AuthOutcome::Reject { reason } => panic!("expected accept, got reject: {reason}"),
        }
    }

    #[test]
    fn classify_rejects_unknown_key() {
        let (_sk_a, _vk_a, b64_a) = fresh_ed25519();
        let (_sk_b, vk_b, _b64_b) = fresh_ed25519();
        let r = registry_with("acme", &b64_a);
        let h = BridgeHandler::new(r, empty_resolver(), None);
        let pk = russh::keys::key::PublicKey::Ed25519(vk_b);
        assert_eq!(
            h.classify(&pk),
            AuthOutcome::Reject {
                reason: "unknown-key"
            }
        );
    }

    #[test]
    fn classify_empty_registry_rejects() {
        let (_sk, vk, _b64) = fresh_ed25519();
        let r = Arc::new(TenantRegistry::empty());
        let h = BridgeHandler::new(r, empty_resolver(), None);
        let pk = russh::keys::key::PublicKey::Ed25519(vk);
        assert_eq!(
            h.classify(&pk),
            AuthOutcome::Reject {
                reason: "unknown-key"
            }
        );
    }

    #[test]
    fn handler_tenant_unset_until_auth() {
        let (_sk, _vk, b64) = fresh_ed25519();
        let r = registry_with("acme", &b64);
        let h = BridgeHandler::new(r, empty_resolver(), None);
        assert!(h.tenant().is_none());
    }

    #[test]
    fn server_mints_fresh_handler_per_client() {
        let r = Arc::new(TenantRegistry::empty());
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let mut s = BridgeServer::new(BridgeServerConfig::new(
            r,
            addr,
            fresh_host_key(),
            empty_resolver(),
        ));
        let h1 = s.new_client(None);
        let h2 = s.new_client(None);
        // Each handler is independent — auth state on one doesn't
        // bleed to the other.
        assert!(h1.tenant().is_none());
        assert!(h2.tenant().is_none());
    }

    #[test]
    fn auth_outcome_eq_is_value_based() {
        let id = TenantId::new("acme").unwrap();
        assert_eq!(AuthOutcome::Accept(id.clone()), AuthOutcome::Accept(id));
        assert_ne!(
            AuthOutcome::Reject {
                reason: "unknown-key"
            },
            AuthOutcome::Reject {
                reason: "non-ed25519-key"
            }
        );
    }

    #[test]
    fn subtle_constant_time_eq_for_key_bytes() {
        // SUPERSOCIETY: backstops the registry's plain-eq lookup in
        // case a future refactor swaps in raw byte compare on a key
        // material slice. subtle::ConstantTimeEq is the right
        // primitive for any post-Marvin equality check on auth-
        // adjacent bytes.
        let (_sk, vk, _b64) = fresh_ed25519();
        let bytes = vk.as_bytes();
        assert!(bool::from(bytes.ct_eq(bytes)));
        let mut tampered = *bytes;
        tampered[0] ^= 1;
        assert!(!bool::from(bytes.ct_eq(&tampered)));
    }

    #[test]
    fn bridge_error_from_russh_preserves_message() {
        let e: BridgeError = russh::Error::Disconnect.into();
        let msg = e.to_string();
        assert!(msg.contains("russh error"));
    }

    #[test]
    fn build_russh_config_has_single_ed25519_keypair() {
        let r = Arc::new(TenantRegistry::empty());
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let hk = fresh_host_key();
        let hk_bytes = hk.verifying_key_bytes();
        let s = BridgeServer::new(BridgeServerConfig::new(r, addr, hk, empty_resolver()));
        let cfg = s.build_russh_config();
        assert_eq!(cfg.keys.len(), 1, "exactly one host keypair");
        match &cfg.keys[0] {
            russh::keys::key::KeyPair::Ed25519(kp) => {
                assert_eq!(
                    kp.verifying_key().to_bytes(),
                    hk_bytes,
                    "russh keypair must wrap the BridgeHostKey signing key"
                );
            }
            #[allow(unreachable_patterns)]
            _ => panic!("russh keys::KeyPair must be Ed25519"),
        }
    }

    #[test]
    fn build_russh_config_methods_is_publickey_only() {
        // SECURITY: backstops the Marvin Attack SHIP-DECISION. If
        // methods ever includes PASSWORD or KEYBOARD_INTERACTIVE we
        // open a class of credential-stuffing + timing attacks that
        // the cycle-3 ed25519-only handler can't catch — those
        // methods never call into auth_publickey.
        let r = Arc::new(TenantRegistry::empty());
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let s = BridgeServer::new(BridgeServerConfig::new(
            r,
            addr,
            fresh_host_key(),
            empty_resolver(),
        ));
        let cfg = s.build_russh_config();
        assert_eq!(
            cfg.methods,
            russh::MethodSet::PUBLICKEY,
            "auth methods must be PUBLICKEY only"
        );
    }

    #[test]
    fn build_russh_config_uniform_reject_time() {
        // SECURITY: reject time should be the SAME for initial vs
        // subsequent attempts so a timing observer can't differentiate
        // a known-unknown-user from a known-known-user without-key.
        let r = Arc::new(TenantRegistry::empty());
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let s = BridgeServer::new(BridgeServerConfig::new(
            r,
            addr,
            fresh_host_key(),
            empty_resolver(),
        ));
        let cfg = s.build_russh_config();
        assert_eq!(cfg.auth_rejection_time, Duration::from_secs(2));
        assert_eq!(
            cfg.auth_rejection_time_initial,
            Some(Duration::from_secs(2))
        );
    }

    #[test]
    fn build_russh_config_caps_auth_attempts() {
        let r = Arc::new(TenantRegistry::empty());
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let s = BridgeServer::new(BridgeServerConfig::new(
            r,
            addr,
            fresh_host_key(),
            empty_resolver(),
        ));
        let cfg = s.build_russh_config();
        assert!(
            cfg.max_auth_attempts <= 3,
            "bridge should cap auth attempts low to defeat enumeration; got {}",
            cfg.max_auth_attempts
        );
    }

    #[test]
    fn server_listen_addr_accessor_returns_config_addr() {
        let r = Arc::new(TenantRegistry::empty());
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 4242);
        let s = BridgeServer::new(BridgeServerConfig::new(
            r,
            addr,
            fresh_host_key(),
            empty_resolver(),
        ));
        assert_eq!(s.listen_addr(), addr);
    }

    // ---------- cycle 5a: hello-banner helper ----------

    #[test]
    fn format_hello_banner_includes_tenant() {
        let id = TenantId::new("acme-corp").unwrap();
        let bytes = format_hello_banner(&id);
        let s = std::str::from_utf8(&bytes).expect("utf-8");
        assert!(s.contains("tenant=acme-corp"), "banner missing tenant: {s}");
    }

    #[test]
    fn format_hello_banner_includes_cycle_marker() {
        // Forensic-trail aid: the banner identifies WHICH cycle of the
        // bridge produced it. When cycle-5b replaces this with the
        // claude exec, the marker disappears — log greppers know.
        let id = TenantId::new("widgets-co").unwrap();
        let bytes = format_hello_banner(&id);
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.contains("cycle-5a"), "banner missing cycle marker: {s}");
    }

    #[test]
    fn format_hello_banner_ends_with_newline() {
        let id = TenantId::new("acme").unwrap();
        let bytes = format_hello_banner(&id);
        assert_eq!(bytes.last(), Some(&b'\n'));
    }

    #[test]
    fn format_hello_banner_is_pure_ascii() {
        // Audit-log friendliness: keep the banner ASCII so log
        // pipelines that don't UTF-8-normalise don't garble it.
        let id = TenantId::new("acme").unwrap();
        let bytes = format_hello_banner(&id);
        for &b in &bytes {
            assert!(b.is_ascii(), "non-ascii byte: {b}");
        }
    }

    // ---------- cycle 5w: format_prepare_banner ----------

    #[test]
    fn format_prepare_banner_with_no_allowlist_says_drop_by_default() {
        let argv = vec!["bwrap".to_owned(), "--".to_owned(), "claude".to_owned()];
        let s = format_prepare_banner(&argv, None);
        assert!(s.starts_with("launch: prepared, argv_len=3, "));
        assert!(s.contains("allowlist=[EMPTY (drop-by-default)]"));
        assert!(s.ends_with("\n"));
    }

    #[test]
    fn format_prepare_banner_with_resolved_allowlist_surfaces_counts() {
        use crate::egress::ResolvedAllowlist;
        use std::net::{Ipv4Addr, Ipv6Addr};
        let argv = vec!["bwrap".to_owned(), "claude".to_owned()];
        let resolved = ResolvedAllowlist {
            ipv4: vec![Ipv4Addr::new(1, 1, 1, 1), Ipv4Addr::new(8, 8, 8, 8)],
            ipv6: vec![Ipv6Addr::LOCALHOST],
            failed: vec!["unreachable.example".to_owned()],
        };
        let s = format_prepare_banner(&argv, Some(&resolved));
        assert!(
            s.contains("ipv4=2, ipv6=1, failed_hosts=1"),
            "should surface resolved counts: {s}"
        );
        assert!(s.contains("argv_len=2"));
    }

    #[test]
    fn format_prepare_banner_audit_argv_format_is_debug_array() {
        // {:?} format → operator can paste into a `let argv = …;`
        // for reproduction.
        let argv = vec!["bwrap".to_owned(), "/sites/acme".to_owned()];
        let s = format_prepare_banner(&argv, None);
        assert!(s.contains(r#"audit_argv=["bwrap", "/sites/acme"]"#));
    }

    #[test]
    fn format_hello_banner_does_not_leak_secrets() {
        // SECURITY: the banner is sent over an authenticated channel,
        // but the banner content itself should still NOT contain
        // anything sensitive (e.g. internal paths, registry contents,
        // host-key fingerprints). Tenant id is the only identifier.
        let id = TenantId::new("acme").unwrap();
        let bytes = format_hello_banner(&id);
        let s = std::str::from_utf8(&bytes).unwrap();
        for forbidden in ["sha256", "ed25519", "/home", "/etc", "host_key", "signing"] {
            assert!(!s.contains(forbidden), "banner leaks '{forbidden}': {s}");
        }
    }

    // ---------- cycle 5q: sandbox_params plumbing ----------

    #[test]
    fn config_new_leaves_sandbox_params_none_for_backwards_compat() {
        let r = Arc::new(TenantRegistry::default());
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let cfg = BridgeServerConfig::new(r, addr, fresh_host_key(), empty_resolver());
        assert!(
            cfg.sandbox_params.is_none(),
            "default config preserves cycle-5d behaviour"
        );
    }

    #[test]
    fn config_with_sandbox_params_attaches_the_bundle() {
        use crate::resource::ResourceCeilings;
        use crate::sandbox_params::BridgeSandboxParams;
        use std::path::PathBuf;
        let r = Arc::new(TenantRegistry::default());
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let params =
            BridgeSandboxParams::new(PathBuf::from("/srv/loom"), ResourceCeilings::default())
                .expect("valid params");
        let cfg = BridgeServerConfig::new(r, addr, fresh_host_key(), empty_resolver())
            .with_sandbox_params(params);
        assert!(
            cfg.sandbox_params.is_some(),
            "with_sandbox_params attaches the bundle"
        );
        let attached = cfg.sandbox_params.expect("must be Some");
        assert_eq!(attached.sandbox_root, PathBuf::from("/srv/loom"));
        assert_eq!(attached.cgroup_root, "/sys/fs/cgroup");
        assert_eq!(attached.nft_binary, "nft");
    }

    #[test]
    fn handler_carries_sandbox_params_through_constructor() {
        use crate::resource::ResourceCeilings;
        use crate::sandbox_params::BridgeSandboxParams;
        use std::path::PathBuf;
        let r = Arc::new(TenantRegistry::default());
        let params =
            BridgeSandboxParams::new(PathBuf::from("/srv/loom"), ResourceCeilings::default())
                .expect("valid params");
        let handler = BridgeHandler::new(r.clone(), empty_resolver(), Some(params));
        assert!(
            handler.sandbox_params.is_some(),
            "handler retains the sandbox_params bundle from the server config"
        );
        let handler_none = BridgeHandler::new(r, empty_resolver(), None);
        assert!(
            handler_none.sandbox_params.is_none(),
            "None means cycle-5d behaviour"
        );
    }
}
