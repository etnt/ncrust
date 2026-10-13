//! The `SshBackend` trait that hides the SSH library.
//!
//! May depend on: config, error, transport::ssh::{algorithms, auth, hostkey}.

use std::fmt;
use std::net::TcpStream;

use super::algorithms::{ResolvedAlgorithms, Supported};
use super::auth::AuthMethod;
use crate::config::HostKeyCheck;
use crate::error::Error;
use crate::transport::Transport;

/// Everything a back end needs to set up an SSH session on a connected socket.
#[derive(Debug, Clone)]
pub struct SshOptions {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: AuthMethod,
    pub algorithms: ResolvedAlgorithms,
    pub host_key: HostKeyCheck,
}

/// The algorithms that were agreed on with the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Negotiated {
    pub kex: String,
    pub host_key: String,
    pub cipher: String,
    pub mac: String,
}

impl fmt::Display for Negotiated {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "kex={} host-key={} cipher={} mac={}",
            self.kex, self.host_key, self.cipher, self.mac
        )
    }
}

/// A session ready for NETCONF: the `netconf` subsystem is open.
pub struct SshConnection {
    pub transport: Box<dyn Transport>,
    /// Things the user should know that are not errors.
    pub warnings: Vec<String>,
    pub negotiated: Option<Negotiated>,
}

/// What a back end supports, so unsupported requests fail before any connection is made.
#[derive(Debug, Clone, Default)]
pub struct BackendCaps {
    pub supported: Supported,
}

/// An SSH client implementation. Everything cryptographic is behind this trait.
pub trait SshBackend {
    fn caps(&self) -> BackendCaps;

    /// Runs the SSH handshake and authentication on `stream` and opens the `netconf` subsystem.
    fn connect(&self, stream: TcpStream, options: &SshOptions) -> Result<SshConnection, Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negotiated_algorithms_are_shown_on_one_line() {
        let n = Negotiated {
            kex: "mlkem768x25519-sha256".into(),
            host_key: "ssh-ed25519".into(),
            cipher: "aes256-gcm@openssh.com".into(),
            mac: "none".into(),
        };
        assert_eq!(
            n.to_string(),
            "kex=mlkem768x25519-sha256 host-key=ssh-ed25519 cipher=aes256-gcm@openssh.com mac=none"
        );
    }
}
