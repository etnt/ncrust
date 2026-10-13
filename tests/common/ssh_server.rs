//! In-process SSH server for tests. It accepts the `netconf` subsystem and bridges it to a
//! `MockServer` TCP port, so the same scripts serve both SSH and TCP tests.
//!
//! The keys in `tests/fixtures/ssh` are throwaway keys made for these tests; they protect nothing.

use std::borrow::Cow;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use russh::keys::{HashAlg, PrivateKey, PublicKey, load_secret_key};
use russh::server::{Auth, Config, Handler, Msg, Session};
use russh::{Channel, ChannelId, Preferred, kex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::{Builder, Runtime};
use tokio::sync::mpsc;

pub const USER: &str = "admin";
pub const PASSWORD: &str = "admin";
pub const KEY_PASSPHRASE: &str = "secret";

/// Path of a file in `tests/fixtures/ssh`.
pub fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/ssh")
        .join(name)
}

fn public_key_of(name: &str) -> PublicKey {
    load_secret_key(fixture(name), None)
        .or_else(|_| load_secret_key(fixture(name), Some(KEY_PASSPHRASE)))
        .expect("fixture key")
        .public_key()
        .clone()
}

/// Fingerprint of the server's host key as `SHA256:...`.
pub fn host_key_fingerprint(name: &str) -> String {
    public_key_of(name).fingerprint(HashAlg::Sha256).to_string()
}

/// The host key as an OpenSSH public key line without comment (`ssh-ed25519 AAAA...`).
pub fn host_public_key_line(name: &str) -> String {
    public_key_of(name).to_openssh().expect("encode public key")
}

pub struct ServerOptions {
    /// File name in `tests/fixtures/ssh`.
    pub host_key: &'static str,
    /// Users with these keys (file names in `tests/fixtures/ssh`) may log in.
    pub authorized_keys: Vec<&'static str>,
    /// The password of `USER`; `None` turns password login off.
    pub password: Option<&'static str>,
    /// Only these key exchange algorithms are offered.
    pub kex: Option<Vec<kex::Name>>,
}

impl Default for ServerOptions {
    fn default() -> Self {
        ServerOptions {
            host_key: "host_ed25519",
            authorized_keys: vec![
                "user_ed25519",
                "user_ecdsa",
                "user_rsa",
                "user_ed25519_encrypted",
            ],
            password: Some(PASSWORD),
            kex: None,
        }
    }
}

#[derive(Default)]
pub struct Stats {
    /// Number of data messages the server received on the channel.
    pub data_events: AtomicUsize,
    pub failed_logins: AtomicUsize,
}

pub struct SshServer {
    pub port: u16,
    pub stats: Arc<Stats>,
    // Kept alive while the server is in use.
    _runtime: Runtime,
}

impl SshServer {
    /// Listens on a free local port and serves each connection, bridged to `backend_port`.
    pub fn start(options: ServerOptions, backend_port: u16) -> Self {
        let runtime = runtime();
        let listener = runtime
            .block_on(TcpListener::bind((Ipv4Addr::LOCALHOST, 0)))
            .expect("bind ssh server");
        let port = listener.local_addr().unwrap().port();
        let (config, shared) = shared(options, backend_port);
        let stats = shared.stats.clone();
        runtime.spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                serve(config.clone(), shared.clone(), socket);
            }
        });
        SshServer {
            port,
            stats,
            _runtime: runtime,
        }
    }

    /// Call home: connects to `target_port` (where the client listens) and then acts as the
    /// SSH server on that connection.
    pub fn call_home(options: ServerOptions, backend_port: u16, target_port: u16) -> Self {
        let runtime = runtime();
        let (config, shared) = shared(options, backend_port);
        let stats = shared.stats.clone();
        runtime.spawn(async move {
            // The client may not be listening yet.
            let socket = loop {
                match TcpStream::connect((Ipv4Addr::LOCALHOST, target_port)).await {
                    Ok(socket) => break socket,
                    Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
                }
            };
            serve(config, shared, socket);
        });
        SshServer {
            port: 0,
            stats,
            _runtime: runtime,
        }
    }
}

fn runtime() -> Runtime {
    Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("runtime")
}

struct Shared {
    backend: SocketAddr,
    authorized: Vec<PublicKey>,
    password: Option<&'static str>,
    stats: Arc<Stats>,
}

fn shared(options: ServerOptions, backend_port: u16) -> (Arc<Config>, Arc<Shared>) {
    let host_key: PrivateKey = load_secret_key(fixture(options.host_key), None).expect("host key");
    let mut preferred = Preferred::default();
    if let Some(names) = options.kex {
        let mut names = names;
        names.push(kex::EXTENSION_SUPPORT_AS_SERVER);
        names.push(kex::EXTENSION_OPENSSH_STRICT_KEX_AS_SERVER);
        preferred.kex = Cow::Owned(names);
    }
    let config = Arc::new(Config {
        keys: vec![host_key],
        preferred,
        // Failed logins are quick in tests.
        auth_rejection_time: Duration::from_millis(5),
        auth_rejection_time_initial: Some(Duration::ZERO),
        ..Config::default()
    });
    let shared = Arc::new(Shared {
        backend: SocketAddr::from((Ipv4Addr::LOCALHOST, backend_port)),
        authorized: options
            .authorized_keys
            .iter()
            .map(|n| public_key_of(n))
            .collect(),
        password: options.password,
        stats: Arc::new(Stats::default()),
    });
    (config, shared)
}

fn serve(config: Arc<Config>, shared: Arc<Shared>, socket: TcpStream) {
    tokio::spawn(async move {
        let handler = SessionHandler {
            shared,
            to_backend: None,
        };
        if let Ok(session) = russh::server::run_stream(config, socket, handler).await {
            let _ = session.await;
        }
    });
}

struct SessionHandler {
    shared: Arc<Shared>,
    to_backend: Option<mpsc::UnboundedSender<Vec<u8>>>,
}

impl SessionHandler {
    fn judge(&self, accepted: bool) -> Auth {
        if accepted {
            Auth::Accept
        } else {
            self.shared
                .stats
                .failed_logins
                .fetch_add(1, Ordering::SeqCst);
            Auth::reject()
        }
    }
}

impl Handler for SessionHandler {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        let accepted = user == USER && self.shared.password == Some(password);
        Ok(self.judge(accepted))
    }

    async fn auth_publickey(&mut self, user: &str, key: &PublicKey) -> Result<Auth, Self::Error> {
        let accepted = user == USER
            && self
                .shared
                .authorized
                .iter()
                .any(|k| k.key_data() == key.key_data());
        Ok(self.judge(accepted))
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        reply: russh::server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        channel: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let backend = if name == "netconf" {
            TcpStream::connect(self.shared.backend).await.ok()
        } else {
            None
        };
        let Some(backend) = backend else {
            session.channel_failure(channel)?;
            return Ok(());
        };
        session.channel_success(channel)?;

        let (mut from_backend, mut into_backend) = backend.into_split();
        let (sender, mut receiver) = mpsc::unbounded_channel::<Vec<u8>>();
        self.to_backend = Some(sender);
        tokio::spawn(async move {
            while let Some(chunk) = receiver.recv().await {
                if into_backend.write_all(&chunk).await.is_err() {
                    return;
                }
            }
            let _ = into_backend.shutdown().await;
        });
        let handle = session.handle();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 8192];
            loop {
                match from_backend.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if handle.data(channel, buf[..n].to_vec()).await.is_err() {
                            return;
                        }
                    }
                }
            }
            let _ = handle.eof(channel).await;
            let _ = handle.close(channel).await;
        });
        Ok(())
    }

    async fn data(
        &mut self,
        _channel: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.shared.stats.data_events.fetch_add(1, Ordering::SeqCst);
        if let Some(sender) = &self.to_backend {
            let _ = sender.send(data.to_vec());
        }
        Ok(())
    }

    async fn channel_eof(
        &mut self,
        _channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.to_backend = None;
        Ok(())
    }
}
