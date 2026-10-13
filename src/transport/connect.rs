//! Host resolution, connecting to all addresses, and the call-home listener.
//!
//! May depend on: error.

use std::io;
use std::net::{TcpStream, ToSocketAddrs};

use crate::error::Error;

/// Connects to `host:port`, trying every resolved address until one accepts.
pub fn connect(host: &str, port: u16) -> Result<TcpStream, Error> {
    let failed = |reason: String| Error::Stdout(format!("Failed to connect to {host}: {reason}"));
    let addrs = (host, port)
        .to_socket_addrs()
        .map_err(|e| failed(e.to_string()))?;

    let mut last_error: Option<io::Error> = None;
    for addr in addrs {
        match TcpStream::connect(addr) {
            Ok(stream) => return Ok(stream),
            Err(e) => last_error = Some(e),
        }
    }
    Err(failed(
        last_error.map(|e| e.to_string()).unwrap_or_default(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn connects_to_a_listening_port() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        connect("127.0.0.1", port).unwrap();
    }

    #[test]
    fn refused_connection_is_reported_on_stdout_with_the_host() {
        let port = {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().port()
        };
        let err = connect("127.0.0.1", port).unwrap_err();
        assert!(
            matches!(&err, Error::Stdout(m) if m.starts_with("Failed to connect to 127.0.0.1: "))
        );
    }

    #[test]
    fn unresolvable_host_is_reported_the_same_way() {
        let err = connect("no-such-host.invalid", 2023).unwrap_err();
        assert!(
            matches!(&err, Error::Stdout(m) if m.starts_with("Failed to connect to no-such-host.invalid: "))
        );
    }
}
