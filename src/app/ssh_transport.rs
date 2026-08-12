//! SSH transport: bridges russh's async client to a synchronous,
//! PTY-shaped byte-stream interface so an SSH pane can feed the same
//! `app::terminal`/`app::vte_processor` pipeline a local PTY pane uses
//! (see `app::pty::Pty` for the local-PTY equivalent of this pattern).

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use russh::keys::agent::client::AgentClient;
use russh::keys::{load_secret_key, PrivateKeyWithHashAlg, PublicKey};
use russh::{client, ChannelMsg, Disconnect};

use super::ssh::{AuthMethod, SshConfig};
use crate::support::error::{CastermError, Result};

/// Messages from the SSH session's background worker thread, mirroring
/// `app::pty`'s `PtyMsg` so an SSH pane can be drained the same way a local
/// PTY pane is.
pub enum SshMsg {
    Data(Vec<u8>),
    Exit,
}

/// A live SSH connection. A dedicated background thread runs a
/// single-threaded tokio runtime driving the async `russh` client and
/// exposes a synchronous channel-based interface — the same
/// reader-thread-plus-channel shape `app::pty::Pty` uses — to the rest of
/// the (thread-based, not async) application.
pub struct SshTransport {
    msg_rx: mpsc::Receiver<SshMsg>,
    write_tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    resize_tx: tokio::sync::mpsc::UnboundedSender<(u16, u16)>,
    _worker: thread::JoinHandle<()>,
}

impl SshTransport {
    /// Connect and open an interactive shell over a PTY-request+shell
    /// channel, blocking the calling thread until the connection either
    /// succeeds or fails (matching `Pty::spawn`'s synchronous contract).
    pub fn connect(config: &SshConfig, cols: u16, rows: u16) -> Result<Self> {
        let (ready_tx, ready_rx) = mpsc::channel::<std::result::Result<(), String>>();
        let (msg_tx, msg_rx) = mpsc::channel::<SshMsg>();
        let (write_tx, write_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        let (resize_tx, resize_rx) = tokio::sync::mpsc::unbounded_channel::<(u16, u16)>();

        let config = config.clone();
        let worker = thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = ready_tx.send(Err(e.to_string()));
                    return;
                }
            };
            rt.block_on(run_session(
                config, cols, rows, ready_tx, msg_tx, write_rx, resize_rx,
            ));
        });

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                msg_rx,
                write_tx,
                resize_tx,
                _worker: worker,
            }),
            Ok(Err(e)) => Err(CastermError::Ssh(e)),
            Err(_) => Err(CastermError::Ssh(
                "SSH worker thread exited before connecting".to_string(),
            )),
        }
    }

    /// Non-blocking poll for the next message, mirroring how
    /// `ui::tui`'s pane-drain loop drains a local PTY's `mpsc::Receiver`.
    pub fn try_recv(&self) -> Option<SshMsg> {
        self.msg_rx.try_recv().ok()
    }

    /// Write keystrokes/input to the remote shell.
    pub fn write(&self, data: &[u8]) -> Result<()> {
        self.write_tx
            .send(data.to_vec())
            .map_err(|_| CastermError::Ssh("SSH session has closed".to_string()))
    }

    /// Notify the remote PTY of a terminal resize.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        self.resize_tx
            .send((cols, rows))
            .map_err(|_| CastermError::Ssh("SSH session has closed".to_string()))
    }
}

/// Handler for a single `russh` client session. Only host-key verification
/// is overridden — channel data is read directly off `Channel::wait()` in
/// `run_session` rather than through `Handler::data`.
struct SshClientHandler {
    host_label: String,
}

impl client::Handler for SshClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        Ok(verify_known_host(&self.host_label, server_public_key).unwrap_or(false))
    }
}

fn known_hosts_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ssh")
        .join("known_hosts")
}

/// Trust-on-first-use host-key verification against `~/.ssh/known_hosts`:
/// an unknown host's key is recorded and accepted, a known host's key must
/// match exactly (a mismatch means the host key changed and the connection
/// is rejected), matching `StrictHostKeyChecking=accept-new` behavior.
fn verify_known_host(host_label: &str, key: &PublicKey) -> std::io::Result<bool> {
    let openssh = key
        .to_openssh()
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    let mut fields = openssh.splitn(2, ' ');
    let keytype = fields.next().unwrap_or_default();
    let keydata = fields.next().unwrap_or_default();

    let path = known_hosts_path();
    if let Ok(contents) = std::fs::read_to_string(&path) {
        for line in contents.lines() {
            let mut parts = line.split_whitespace();
            let Some(hosts_field) = parts.next() else {
                continue;
            };
            if !hosts_field.split(',').any(|h| h == host_label) {
                continue;
            }
            let (Some(entry_type), Some(entry_key)) = (parts.next(), parts.next()) else {
                continue;
            };
            return Ok(entry_type == keytype && entry_key == keydata);
        }
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    writeln!(file, "{host_label} {keytype} {keydata}")?;
    Ok(true)
}

fn host_label(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_string()
    } else {
        format!("[{host}]:{port}")
    }
}

fn ssh_err(e: russh::Error) -> CastermError {
    CastermError::Ssh(e.to_string())
}

/// Authenticate `handle` against `auth`, returning `Ok(())` on success.
/// Password, public-key (file), and ssh-agent are the three most common of
/// the five `AuthMethod` variants and are implemented; keyboard-interactive
/// and GSSAPI are explicitly out of MVP scope (logged in `TODO.AI.md`) and
/// fail with a clear error rather than silently no-opping.
async fn authenticate(
    handle: &mut client::Handle<SshClientHandler>,
    username: &str,
    auth: &AuthMethod,
) -> Result<()> {
    let success = match auth {
        AuthMethod::Password(password) => handle
            .authenticate_password(username, password)
            .await
            .map_err(ssh_err)?
            .success(),
        AuthMethod::PublicKey(path) => {
            let key_pair = load_secret_key(path, None).map_err(|e| {
                CastermError::Ssh(format!(
                    "failed to load private key {}: {e}",
                    path.display()
                ))
            })?;
            let hash_alg = handle
                .best_supported_rsa_hash()
                .await
                .map_err(ssh_err)?
                .flatten();
            handle
                .authenticate_publickey(
                    username,
                    PrivateKeyWithHashAlg::new(Arc::new(key_pair), hash_alg),
                )
                .await
                .map_err(ssh_err)?
                .success()
        }
        AuthMethod::Agent => authenticate_with_agent(handle, username).await?,
        AuthMethod::KeyboardInteractive => {
            return Err(CastermError::Ssh(
                "keyboard-interactive authentication is not yet supported".to_string(),
            ));
        }
        AuthMethod::Gssapi => {
            return Err(CastermError::Ssh(
                "GSSAPI authentication is not yet supported".to_string(),
            ));
        }
    };

    if success {
        Ok(())
    } else {
        Err(CastermError::Ssh(format!(
            "authentication failed for {username}"
        )))
    }
}

/// Try every identity offered by a running ssh-agent (`SSH_AUTH_SOCK`)
/// until one is accepted.
async fn authenticate_with_agent(
    handle: &mut client::Handle<SshClientHandler>,
    username: &str,
) -> Result<bool> {
    let mut agent = AgentClient::connect_env()
        .await
        .map_err(|e| CastermError::Ssh(format!("failed to connect to ssh-agent: {e}")))?;
    let identities = agent
        .request_identities()
        .await
        .map_err(|e| CastermError::Ssh(format!("failed to list ssh-agent identities: {e}")))?;

    for key in identities {
        let hash_alg = handle
            .best_supported_rsa_hash()
            .await
            .map_err(ssh_err)?
            .flatten();
        if let Ok(res) = handle
            .authenticate_publickey_with(username, key, hash_alg, &mut agent)
            .await
        {
            if res.success() {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

#[allow(clippy::too_many_arguments)]
async fn run_session(
    config: SshConfig,
    cols: u16,
    rows: u16,
    ready_tx: mpsc::Sender<std::result::Result<(), String>>,
    msg_tx: mpsc::Sender<SshMsg>,
    mut write_rx: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
    mut resize_rx: tokio::sync::mpsc::UnboundedReceiver<(u16, u16)>,
) {
    let client_config = Arc::new(client::Config {
        inactivity_timeout: (config.timeout > 0)
            .then(|| std::time::Duration::from_secs(config.timeout as u64)),
        keepalive_interval: (config.keepalive_interval > 0)
            .then(|| std::time::Duration::from_secs(config.keepalive_interval as u64)),
        ..Default::default()
    });

    let handler = SshClientHandler {
        host_label: host_label(&config.host, config.port),
    };

    // Single jump-host hop: connect and authenticate to the jump host
    // first, then tunnel a direct-tcpip channel to the real target and
    // hand that stream to `connect_stream` instead of opening a fresh TCP
    // connection. The jump host's `Handle` (`_jump_handle`) is kept alive
    // for the lifetime of the tunnel.
    let mut handle = if let Some(jump) = config.jump_hosts.first() {
        let jump_config = Arc::new(client::Config::default());
        let jump_handler = SshClientHandler {
            host_label: host_label(&jump.host, jump.port),
        };
        let mut jump_handle =
            match client::connect(jump_config, (jump.host.as_str(), jump.port), jump_handler).await
            {
                Ok(h) => h,
                Err(e) => {
                    let _ = ready_tx.send(Err(format!(
                        "failed to connect to jump host {}: {e}",
                        jump.host
                    )));
                    return;
                }
            };
        if let Err(e) = authenticate(&mut jump_handle, &jump.username, &jump.auth).await {
            let _ = ready_tx.send(Err(format!("jump host authentication failed: {e}")));
            return;
        }
        let tunnel = match jump_handle
            .channel_open_direct_tcpip(config.host.as_str(), config.port as u32, "127.0.0.1", 0)
            .await
        {
            Ok(c) => c,
            Err(e) => {
                let _ = ready_tx.send(Err(format!("failed to open jump-host tunnel: {e}")));
                return;
            }
        };
        match client::connect_stream(client_config, tunnel.into_stream(), handler).await {
            Ok(h) => h,
            Err(e) => {
                let _ = ready_tx.send(Err(format!(
                    "failed to establish SSH session over jump host: {e}"
                )));
                return;
            }
        }
    } else {
        match client::connect(client_config, (config.host.as_str(), config.port), handler).await {
            Ok(h) => h,
            Err(e) => {
                let _ = ready_tx.send(Err(format!("failed to connect to {}: {e}", config.host)));
                return;
            }
        }
    };

    if let Err(e) = authenticate(&mut handle, &config.username, &config.auth).await {
        let _ = ready_tx.send(Err(e.to_string()));
        return;
    }

    let mut channel = match handle.channel_open_session().await {
        Ok(c) => c,
        Err(e) => {
            let _ = ready_tx.send(Err(format!("failed to open session channel: {e}")));
            return;
        }
    };

    let term = std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_string());
    if let Err(e) = channel
        .request_pty(true, &term, cols as u32, rows as u32, 0, 0, &[])
        .await
    {
        let _ = ready_tx.send(Err(format!("failed to request a PTY: {e}")));
        return;
    }
    if let Err(e) = channel.request_shell(true).await {
        let _ = ready_tx.send(Err(format!("failed to start a remote shell: {e}")));
        return;
    }

    let _ = ready_tx.send(Ok(()));

    loop {
        tokio::select! {
            data = write_rx.recv() => {
                match data {
                    Some(data) => {
                        if channel.data(&data[..]).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            resize = resize_rx.recv() => {
                if let Some((cols, rows)) = resize {
                    let _ = channel.window_change(cols as u32, rows as u32, 0, 0).await;
                }
            }
            msg = channel.wait() => {
                match msg {
                    Some(ChannelMsg::Data { data }) => {
                        if msg_tx.send(SshMsg::Data(data.to_vec())).is_err() {
                            break;
                        }
                    }
                    Some(ChannelMsg::ExtendedData { data, .. }) => {
                        if msg_tx.send(SshMsg::Data(data.to_vec())).is_err() {
                            break;
                        }
                    }
                    Some(ChannelMsg::ExitStatus { .. }) | Some(ChannelMsg::Close) | None => {
                        let _ = msg_tx.send(SshMsg::Exit);
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    let _ = handle
        .disconnect(Disconnect::ByApplication, "", "English")
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ssh::ConnectionId;
    use russh::keys::{Algorithm, PrivateKey};
    use russh::server::{
        Auth, Config as ServerConfig, Handler as ServerHandler, Msg, Server as ServerTrait,
        Session as ServerSession,
    };
    use std::net::SocketAddr;
    use std::time::{Duration, Instant};

    /// In-process test server: accepts any password, opens a shell, sends a
    /// banner, and echoes back whatever the client writes — enough to
    /// exercise `SshTransport::connect`/`write`/`try_recv` end to end without
    /// a real network host, per the Phase 4 plan's required integration
    /// test ("run an in-process russh test server ... assert connect()
    /// completes, data written to the channel round-trips").
    struct TestServer;

    impl ServerTrait for TestServer {
        type Handler = TestHandler;

        fn new_client(&mut self, _peer_addr: Option<SocketAddr>) -> TestHandler {
            TestHandler
        }
    }

    struct TestHandler;

    impl ServerHandler for TestHandler {
        type Error = russh::Error;

        async fn auth_password(
            &mut self,
            _user: &str,
            _password: &str,
        ) -> std::result::Result<Auth, Self::Error> {
            Ok(Auth::Accept)
        }

        async fn channel_open_session(
            &mut self,
            _channel: russh::Channel<Msg>,
            _session: &mut ServerSession,
        ) -> std::result::Result<bool, Self::Error> {
            Ok(true)
        }

        async fn pty_request(
            &mut self,
            channel: russh::ChannelId,
            _term: &str,
            _col_width: u32,
            _row_height: u32,
            _pix_width: u32,
            _pix_height: u32,
            _modes: &[(russh::Pty, u32)],
            session: &mut ServerSession,
        ) -> std::result::Result<(), Self::Error> {
            session.channel_success(channel)?;
            Ok(())
        }

        async fn shell_request(
            &mut self,
            channel: russh::ChannelId,
            session: &mut ServerSession,
        ) -> std::result::Result<(), Self::Error> {
            session.channel_success(channel)?;
            session.data(channel, russh::CryptoVec::from_slice(b"banner\n"))?;
            Ok(())
        }

        async fn data(
            &mut self,
            channel: russh::ChannelId,
            data: &[u8],
            session: &mut ServerSession,
        ) -> std::result::Result<(), Self::Error> {
            session.data(channel, russh::CryptoVec::from_slice(data))?;
            Ok(())
        }
    }

    fn recv_data_with_timeout(transport: &SshTransport, timeout: Duration) -> Vec<u8> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(SshMsg::Data(data)) = transport.try_recv() {
                return data;
            }
            if Instant::now() > deadline {
                panic!("timed out waiting for data from the SSH transport");
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn ssh_transport_round_trips_data_through_an_in_process_server() {
        let (addr_tx, addr_rx) = mpsc::channel::<SocketAddr>();
        let _server_thread = thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("build test-server tokio runtime");
            rt.block_on(async move {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                    .await
                    .expect("bind ephemeral test-server port");
                let addr = listener.local_addr().expect("read bound test-server addr");
                let _ = addr_tx.send(addr);

                let key = PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519)
                    .expect("generate test-server host key");
                let config = Arc::new(ServerConfig {
                    keys: vec![key],
                    ..Default::default()
                });
                let mut server = TestServer;
                let _ = server.run_on_socket(config, &listener).await;
            });
        });

        let addr = addr_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("test server never reported its bound address");

        let config = SshConfig {
            id: ConnectionId::next(),
            name: "test".to_string(),
            host: addr.ip().to_string(),
            port: addr.port(),
            username: "tester".to_string(),
            auth: AuthMethod::Password("anything".to_string()),
            ..Default::default()
        };

        let transport = SshTransport::connect(&config, 80, 24)
            .expect("SshTransport::connect should succeed against the in-process test server");

        let banner = recv_data_with_timeout(&transport, Duration::from_secs(5));
        assert_eq!(banner, b"banner\n");

        transport
            .write(b"ping")
            .expect("write to the SSH transport");
        let echoed = recv_data_with_timeout(&transport, Duration::from_secs(5));
        assert_eq!(echoed, b"ping");
    }
}
