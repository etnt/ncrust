//! NSO/ConfD IPC-over-TCP transport (connection preamble, then raw bytes).
//!
//! May depend on: config, error.

use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::time::Duration;

use super::Transport;
use crate::error::Error;
use crate::transport::connect::connect;

/// The line a NSO/ConfD IPC port expects first, describing the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preamble {
    pub user: String,
    pub local_ip: String,
    pub uid: u32,
    pub gid: u32,
    /// Comma separated supplementary group ids.
    pub sup_gids: String,
    pub cwd: String,
    /// Comma separated group names.
    pub groups: String,
}

impl Preamble {
    pub fn to_line(&self) -> String {
        format!(
            "[{};{};tcp;{};{};{};{};{};]\n",
            self.user, self.local_ip, self.uid, self.gid, self.sup_gids, self.cwd, self.groups
        )
    }
}

pub struct TcpTransport {
    stream: TcpStream,
}

impl TcpTransport {
    /// Connects and sends the preamble for the current process user.
    pub fn connect(
        host: &str,
        port: u16,
        user: &str,
        groups: &str,
        sup_gids: &str,
    ) -> Result<Self, Error> {
        let mut stream = connect(host, port)?;
        let preamble = Preamble {
            user: user.to_string(),
            local_ip: stream.local_addr()?.ip().to_string(),
            uid: rustix::process::getuid().as_raw(),
            gid: rustix::process::getgid().as_raw(),
            sup_gids: sup_gids.to_string(),
            cwd: std::env::var("PWD").unwrap_or_else(|_| "/tmp".to_string()),
            groups: groups.to_string(),
        };
        stream.write_all(preamble.to_line().as_bytes())?;
        Ok(TcpTransport { stream })
    }
}

impl Transport for TcpTransport {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stream.read(buf)
    }

    fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        self.stream.write_all(data)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }

    fn set_read_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        self.stream.set_read_timeout(timeout)
    }

    fn close(&mut self) -> io::Result<()> {
        match self.stream.shutdown(Shutdown::Both) {
            Err(e) if e.kind() == io::ErrorKind::NotConnected => Ok(()),
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;

    #[test]
    fn preamble_line_matches_the_bytes_python_sends() {
        // Captured from NetconfTCP.connect() in bin/netconf-console.py.
        let p = Preamble {
            user: "bob".into(),
            local_ip: "127.0.0.1".into(),
            uid: 501,
            gid: 20,
            sup_gids: "7,8".into(),
            cwd: "/home/u".into(),
            groups: "g1,g2".into(),
        };
        assert_eq!(
            p.to_line(),
            "[bob;127.0.0.1;tcp;501;20;7,8;/home/u;g1,g2;]\n"
        );
    }

    #[test]
    fn connect_sends_the_preamble_first() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut transport =
            TcpTransport::connect("127.0.0.1", port, "bob", "admin", "7,8").unwrap();
        let (server, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(server).read_line(&mut line).unwrap();

        assert!(line.starts_with("[bob;127.0.0.1;tcp;"), "{line}");
        assert!(line.contains(";7,8;"), "{line}");
        assert!(line.ends_with(";admin;]\n"), "{line}");
        transport.close().unwrap();
    }
}
