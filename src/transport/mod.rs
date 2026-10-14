//! Blocking byte-stream transports (SSH, and with `tailf-compat` the NSO/ConfD IPC over TCP).
//! Knows nothing about NETCONF messages.
//!
//! May depend on: config, error.

pub mod connect;
pub mod ssh;
#[cfg(feature = "tailf-compat")]
pub mod tcp;

use std::io;
use std::time::Duration;

/// A blocking, bidirectional byte stream to a NETCONF server.
pub trait Transport {
    /// Reads at most `buf.len()` bytes; `Ok(0)` means the peer closed the connection.
    /// With a read timeout set, expiry is reported as `WouldBlock` or `TimedOut`.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;

    fn write_all(&mut self, data: &[u8]) -> io::Result<()>;

    fn flush(&mut self) -> io::Result<()>;

    /// `None` blocks forever.
    fn set_read_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()>;

    fn close(&mut self) -> io::Result<()>;
}

impl<T: Transport + ?Sized> Transport for Box<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (**self).read(buf)
    }

    fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        (**self).write_all(data)
    }

    fn flush(&mut self) -> io::Result<()> {
        (**self).flush()
    }

    fn set_read_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        (**self).set_read_timeout(timeout)
    }

    fn close(&mut self) -> io::Result<()> {
        (**self).close()
    }
}
