//! Default SSH backend (russh). Owns the private async runtime and exposes blocking I/O.
//!
//! May depend on: config, error, transport::ssh::{algorithms, auth, backend, hostkey, keys}.

use std::borrow::Cow;
use std::io;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, Handle, Handler};
use russh::keys::{self, Algorithm, HashAlg, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh::{Channel, ChannelMsg, Disconnect, Names, Preferred, cipher, kex, mac};
use tokio::runtime::{Builder, Runtime};

use super::algorithms::{AlgorithmSet, ResolvedAlgorithms, Supported};
use super::auth::AuthMethod;
use super::backend::{BackendCaps, Negotiated, SshBackend, SshConnection, SshOptions};
use super::hostkey::check_fingerprint;
use super::keys::type_hint_warning;
use crate::config::HostKeyCheck;
use crate::error::Error;
use crate::transport::Transport;

/// The SSH client built on `russh`. It runs on a private tokio runtime and hides it behind
/// blocking `Transport` calls.
pub struct RusshBackend;

fn names<N: AsRef<str>>(list: impl IntoIterator<Item = N>) -> Vec<String> {
    list.into_iter().map(|n| n.as_ref().to_string()).collect()
}

/// Pseudo algorithms that negotiate protocol extensions; they are added automatically.
fn is_extension(name: &str) -> bool {
    name.starts_with("ext-info-") || name.starts_with("kex-strict-")
}

fn host_key_algorithms() -> Vec<(String, Algorithm)> {
    Preferred::DEFAULT
        .key
        .iter()
        .map(|a| (a.to_string(), a.clone()))
        .collect()
}

fn supported() -> Supported {
    let usable = |name: &String| name != "none" && !is_extension(name);
    let kex_supported: Vec<String> = names(kex::ALL_KEX_ALGORITHMS.iter().copied())
        .into_iter()
        .filter(usable)
        .collect();
    let kex_defaults = names(Preferred::DEFAULT.kex.iter())
        .into_iter()
        .filter(usable)
        .collect();
    let host_key: Vec<String> = host_key_algorithms().into_iter().map(|(n, _)| n).collect();
    let ciphers: Vec<String> = names(cipher::ALL_CIPHERS.iter().copied())
        .into_iter()
        .filter(usable)
        .collect();
    let macs: Vec<String> = names(mac::ALL_MAC_ALGORITHMS.iter().copied())
        .into_iter()
        .filter(usable)
        .collect();
    Supported {
        kex: AlgorithmSet {
            supported: kex_supported,
            defaults: kex_defaults,
        },
        host_key: AlgorithmSet {
            supported: host_key.clone(),
            defaults: host_key,
        },
        ciphers: AlgorithmSet {
            supported: ciphers,
            defaults: names(Preferred::DEFAULT.cipher.iter())
                .into_iter()
                .filter(usable)
                .collect(),
        },
        macs: AlgorithmSet {
            supported: macs,
            defaults: names(Preferred::DEFAULT.mac.iter())
                .into_iter()
                .filter(usable)
                .collect(),
        },
    }
}

impl SshBackend for RusshBackend {
    fn caps(&self) -> BackendCaps {
        BackendCaps {
            supported: supported(),
        }
    }

    fn connect(&self, stream: TcpStream, options: &SshOptions) -> Result<SshConnection, Error> {
        let preferred = preferred(&options.algorithms)?;
        let runtime = Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()?;
        let shared = Arc::new(Shared::default());
        let handler = ClientHandler {
            host: options.host.clone(),
            port: options.port,
            check: options.host_key.clone(),
            shared: shared.clone(),
        };

        let opened = runtime.block_on(open(stream, options, preferred, handler));
        let (handle, channel, warnings) = match opened {
            Ok(opened) => opened,
            // A refused host key is reported by the handler; the library only says "unknown key".
            Err(error) => return Err(take(&shared.rejection).map_or(error, Error::Stdout)),
        };
        Ok(SshConnection {
            transport: Box::new(SshTransport {
                runtime,
                handle,
                channel,
                pending: Vec::new(),
                offset: 0,
                eof: false,
                timeout: None,
            }),
            warnings,
            negotiated: take(&shared.negotiated),
        })
    }
}

fn take<T>(slot: &Mutex<Option<T>>) -> Option<T> {
    slot.lock().ok().and_then(|mut s| s.take())
}

fn preferred(algorithms: &ResolvedAlgorithms) -> Result<Preferred, Error> {
    let unsupported = |name: &str| Error::Stdout(format!("Unsupported algorithm '{name}'"));
    let mut kex_names = algorithms
        .kex
        .iter()
        .map(|n| kex::Name::try_from(n.as_str()).map_err(|_| unsupported(n)))
        .collect::<Result<Vec<_>, _>>()?;
    kex_names.push(kex::EXTENSION_SUPPORT_AS_CLIENT);
    kex_names.push(kex::EXTENSION_OPENSSH_STRICT_KEX_AS_CLIENT);

    let known = host_key_algorithms();
    let key = algorithms
        .host_key
        .iter()
        .map(|n| {
            known
                .iter()
                .find(|(name, _)| name == n)
                .map(|(_, a)| a.clone())
                .ok_or_else(|| unsupported(n))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let ciphers = algorithms
        .ciphers
        .iter()
        .map(|n| cipher::Name::try_from(n.as_str()).map_err(|_| unsupported(n)))
        .collect::<Result<Vec<_>, _>>()?;
    let macs = algorithms
        .macs
        .iter()
        .map(|n| mac::Name::try_from(n.as_str()).map_err(|_| unsupported(n)))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Preferred {
        kex: Cow::Owned(kex_names),
        key: Cow::Owned(key),
        cipher: Cow::Owned(ciphers),
        mac: Cow::Owned(macs),
        ..Preferred::default()
    })
}

#[derive(Default)]
struct Shared {
    rejection: Mutex<Option<String>>,
    negotiated: Mutex<Option<Negotiated>>,
}

struct ClientHandler {
    host: String,
    port: u16,
    check: HostKeyCheck,
    shared: Arc<Shared>,
}

impl ClientHandler {
    fn reject(&self, reason: String) -> Result<bool, russh::Error> {
        if let Ok(mut slot) = self.shared.rejection.lock() {
            *slot = Some(reason);
        }
        Ok(false)
    }
}

impl Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let PublicKeyOrCertificate::PublicKey { key, .. } = server_key else {
            return self
                .reject("Host key verification failed: certificates are not supported".into());
        };
        match &self.check {
            HostKeyCheck::AcceptAny => Ok(true),
            HostKeyCheck::Fingerprint(_) => {
                let fingerprint = key.fingerprint(HashAlg::Sha256).to_string();
                match check_fingerprint(&self.check, &fingerprint) {
                    Ok(()) => Ok(true),
                    Err(reason) => self.reject(reason),
                }
            }
            HostKeyCheck::KnownHosts(path) => {
                match keys::check_known_hosts_path(&self.host, self.port, key, path) {
                    Ok(true) => Ok(true),
                    Ok(false) => self.reject(format!(
                        "Host key verification failed: {}:{} is not in {}",
                        self.host,
                        self.port,
                        path.display()
                    )),
                    Err(e) => self.reject(format!(
                        "Host key verification failed for {}:{}: {e}",
                        self.host, self.port
                    )),
                }
            }
        }
    }

    async fn kex_done(
        &mut self,
        _shared_secret: Option<&[u8]>,
        negotiated: &Names,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        if let Ok(mut slot) = self.shared.negotiated.lock() {
            *slot = Some(Negotiated {
                kex: negotiated.kex.as_ref().to_string(),
                host_key: negotiated.key.to_string(),
                cipher: negotiated.cipher.as_ref().to_string(),
                mac: negotiated.client_mac.as_ref().to_string(),
            });
        }
        Ok(())
    }
}

fn ssh_error(e: russh::Error) -> Error {
    Error::Stdout(format!("SSH error: {e}"))
}

type Opened = (Handle<ClientHandler>, Channel<client::Msg>, Vec<String>);

async fn open(
    stream: TcpStream,
    options: &SshOptions,
    preferred: Preferred,
    handler: ClientHandler,
) -> Result<Opened, Error> {
    stream.set_nonblocking(true)?;
    let socket = tokio::net::TcpStream::from_std(stream)?;
    let config = Arc::new(client::Config {
        preferred,
        // A subscription can be quiet for a long time.
        inactivity_timeout: None,
        ..client::Config::default()
    });
    let mut handle = client::connect_stream(config, socket, handler)
        .await
        .map_err(ssh_error)?;

    let mut warnings = Vec::new();
    let authenticated = match &options.auth {
        AuthMethod::Password(password) => handle
            .authenticate_password(options.user.clone(), password.clone())
            .await
            .map_err(ssh_error)?
            .success(),
        AuthMethod::PrivateKeyFile { path, type_hint } => {
            let key = keys::load_secret_key(path, None).map_err(|e| {
                Error::Stdout(format!(
                    "Cannot use the private key {}: {e}",
                    path.display()
                ))
            })?;
            let algorithm = key.algorithm();
            warnings.extend(type_hint_warning(type_hint.as_deref(), algorithm.as_ref()));
            let hash = if algorithm.is_rsa() {
                handle
                    .best_supported_rsa_hash()
                    .await
                    .map_err(ssh_error)?
                    .flatten()
            } else {
                None
            };
            handle
                .authenticate_publickey(
                    options.user.clone(),
                    PrivateKeyWithHashAlg::new(Arc::new(key), hash),
                )
                .await
                .map_err(ssh_error)?
                .success()
        }
    };
    if !authenticated {
        return Err(Error::Stdout("Authentication failed.".into()));
    }

    let mut channel = handle.channel_open_session().await.map_err(ssh_error)?;
    let refused = || Error::Stdout("The server refused the netconf subsystem.".into());
    channel
        .request_subsystem(true, "netconf")
        .await
        .map_err(|_| refused())?;
    // The answer comes as a channel message; anything else before it is not interesting.
    loop {
        match channel.wait().await {
            Some(ChannelMsg::Success) => break,
            Some(ChannelMsg::Failure) | None => return Err(refused()),
            Some(_) => {}
        }
    }
    Ok((handle, channel, warnings))
}

/// A NETCONF subsystem channel as a blocking byte stream.
struct SshTransport {
    runtime: Runtime,
    handle: Handle<ClientHandler>,
    channel: Channel<client::Msg>,
    /// Received data that has not been handed out yet.
    pending: Vec<u8>,
    offset: usize,
    eof: bool,
    timeout: Option<Duration>,
}

impl Transport for SshTransport {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            if self.offset < self.pending.len() {
                let n = buf.len().min(self.pending.len() - self.offset);
                buf[..n].copy_from_slice(&self.pending[self.offset..self.offset + n]);
                self.offset += n;
                return Ok(n);
            }
            if self.eof {
                return Ok(0);
            }
            let timeout = self.timeout;
            let received = self.runtime.block_on(async {
                match timeout {
                    Some(limit) => tokio::time::timeout(limit, self.channel.wait()).await.ok(),
                    None => Some(self.channel.wait().await),
                }
            });
            match received {
                None => return Err(io::ErrorKind::TimedOut.into()),
                Some(Some(ChannelMsg::Data { data })) => {
                    self.pending = data.to_vec();
                    self.offset = 0;
                }
                Some(None | Some(ChannelMsg::Eof | ChannelMsg::Close)) => self.eof = true,
                // Error output, exit status and the like are of no use to a NETCONF client.
                Some(Some(_)) => {}
            }
        }
    }

    fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        // `data_bytes` makes one packet per call (up to the peer's limit); `data` would copy
        // through an 8 KiB buffer and cut the batches the session has carefully put together.
        self.runtime
            .block_on(self.channel.data_bytes(data.to_vec()))
            .map_err(io::Error::other)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn set_read_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        self.timeout = timeout;
        Ok(())
    }

    fn close(&mut self) -> io::Result<()> {
        let (channel, handle) = (&self.channel, &self.handle);
        self.runtime.block_on(async {
            // The peer may already be gone; there is nothing to do about failures here.
            let _ = channel.eof().await;
            let _ = channel.close().await;
            let _ = handle.disconnect(Disconnect::ByApplication, "", "en").await;
        });
        Ok(())
    }
}
