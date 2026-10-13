//! Host resolution, connecting to all addresses, and the call-home listener.
//!
//! May depend on: error.

use std::io::{self, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};

use crate::error::Error;

/// Waits for one incoming connection (NETCONF call home) and returns it. Progress is announced
/// on `announce` in the same words as the Python script, because scripts may wait for them.
pub fn listen(address: &str, port: u16, announce: &mut dyn Write) -> Result<TcpStream, Error> {
    let listener = TcpListener::bind((address, port))
        .map_err(|e| Error::Stdout(format!("Failed to listen on {address}:{port}: {e}")))?;
    accept_call_home(&listener, address, port, announce)
}

fn accept_call_home(
    listener: &TcpListener,
    address: &str,
    port: u16,
    announce: &mut dyn Write,
) -> Result<TcpStream, Error> {
    // Python prints its arguments separated by spaces, so the text has two of them here.
    writeln!(announce, "** listening on  {address} {port}")?;
    announce.flush()?;
    let (stream, peer) = listener.accept()?;
    writeln!(announce, "** call home from {}", python_address(peer))?;
    announce.flush()?;
    Ok(stream)
}

/// A peer address the way Python prints a socket address tuple.
fn python_address(addr: SocketAddr) -> String {
    format!("('{}', {})", addr.ip(), addr.port())
}

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
    fn call_home_announces_itself_and_returns_the_incoming_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let caller = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let mut announced = Vec::new();
        let stream = accept_call_home(&listener, "127.0.0.1", port, &mut announced).unwrap();
        let text = String::from_utf8(announced).unwrap();
        assert_eq!(
            text,
            format!(
                "** listening on  127.0.0.1 {port}\n** call home from ('127.0.0.1', {})\n",
                caller.local_addr().unwrap().port()
            )
        );
        assert_eq!(stream.peer_addr().unwrap(), caller.local_addr().unwrap());
    }

    #[test]
    fn listening_on_a_taken_port_is_an_error() {
        let taken = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = taken.local_addr().unwrap().port();
        let err = listen("127.0.0.1", port, &mut Vec::new()).unwrap_err();
        assert!(matches!(&err, Error::Stdout(m) if m.starts_with("Failed to listen on ")));
    }

    #[test]
    fn refused_connection_is_reported_on_stdout_with_the_host() {
        // Parallel tests may grab the freed port, so look for one that really is closed.
        let err = (0..20)
            .find_map(|_| {
                let port = TcpListener::bind("127.0.0.1:0")
                    .unwrap()
                    .local_addr()
                    .unwrap()
                    .port();
                connect("127.0.0.1", port).err()
            })
            .expect("no closed port found");
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
