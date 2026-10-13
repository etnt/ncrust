//! A NETCONF session over a transport: hello exchange, framing switch, message send/receive, close.
//!
//! May depend on: config, error, framing, protocol, transport.

use std::io;
use std::time::Duration;

use crate::config::NetconfVersion;
use crate::error::Error;
use crate::framing::{Decoder, Frame, Framing, FramingError};
use crate::protocol::hello;
use crate::protocol::ops::{rpc, simple::CloseSession};
use crate::transport::Transport;

/// Size of one read from the transport, and the point at which buffered output is written.
pub const BUFFER_SIZE: usize = 16 * 1024;

/// One step of reading a message from the server.
#[derive(Debug, PartialEq, Eq)]
pub enum Recv {
    Data(Vec<u8>),
    EndOfMessage,
    /// The read timeout expired without a full step.
    Timeout,
    /// The server closed the connection; holds bytes received but not delivered.
    Eof(Vec<u8>),
    /// Invalid framing; holds bytes received but not delivered.
    Framing(FramingError, Vec<u8>),
}

#[derive(Debug)]
pub struct ServerHello {
    /// The hello message as received (without framing).
    pub raw: Vec<u8>,
    pub capabilities: Vec<String>,
}

pub struct Session<T: Transport> {
    transport: T,
    decoder: Decoder,
    out: Vec<u8>,
    /// Wire bytes received since the last `take_trace`, if tracing is on.
    trace: Option<Vec<u8>>,
}

impl<T: Transport> Session<T> {
    /// A new session; framing is base:1.0 until `hello_exchange` negotiates otherwise.
    pub fn new(transport: T) -> Self {
        Session {
            transport,
            decoder: Decoder::new(Framing::Eom),
            out: Vec::new(),
            trace: None,
        }
    }

    pub fn framing(&self) -> Framing {
        self.decoder.framing()
    }

    /// Records every byte read from the transport, framing included (output style `raw`).
    pub fn set_trace(&mut self, on: bool) {
        self.trace = on.then(Vec::new);
    }

    pub fn take_trace(&mut self) -> Vec<u8> {
        self.trace.as_mut().map(std::mem::take).unwrap_or_default()
    }

    /// Sends our hello (unless `send_client_hello` is false), reads the server's, and
    /// switches to chunked framing if both sides support base:1.1.
    pub fn hello_exchange(
        &mut self,
        versions: &[NetconfVersion],
        send_client_hello: bool,
    ) -> Result<ServerHello, Error> {
        if send_client_hello {
            self.send_message(hello::client_hello(versions).as_bytes())?;
        }
        let raw = self.recv_message()?;
        let capabilities = hello::parse_server_hello(&raw)?;
        if hello::negotiate(versions, &capabilities) == NetconfVersion::V1_1 {
            self.decoder.set_framing(Framing::Chunked);
        }
        Ok(ServerHello { raw, capabilities })
    }

    /// Frames `payload` as one complete message and sends it.
    pub fn send_message(&mut self, payload: &[u8]) -> Result<(), Error> {
        self.send_chunk(payload)?;
        self.send_end_of_message()
    }

    /// Sends part of a message; output is buffered until `send_end_of_message` or `flush`.
    pub fn send_chunk(&mut self, payload: &[u8]) -> Result<(), Error> {
        let mut framed = Vec::with_capacity(payload.len() + 16);
        self.framing().encode_chunk(payload, &mut framed);
        self.buffer(&framed)
    }

    pub fn send_end_of_message(&mut self) -> Result<(), Error> {
        let eom = self.framing().end_of_message();
        self.buffer(eom)?;
        self.flush()
    }

    /// Sends bytes without any framing (a whole session script from a file).
    pub fn send_raw(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.buffer(bytes)
    }

    pub fn flush(&mut self) -> Result<(), Error> {
        self.write_out()?;
        self.transport.flush()?;
        Ok(())
    }

    /// Reads until one step of a message is available. `timeout` of `None` blocks.
    pub fn recv(&mut self, timeout: Option<Duration>) -> Result<Recv, Error> {
        self.transport.set_read_timeout(timeout)?;
        let mut buf = vec![0u8; BUFFER_SIZE];
        loop {
            match self.decoder.next_frame() {
                Ok(Some(Frame::Data(data))) => return Ok(Recv::Data(data)),
                Ok(Some(Frame::EndOfMessage)) => return Ok(Recv::EndOfMessage),
                Ok(None) => {}
                Err(e) => return Ok(Recv::Framing(e, self.decoder.take_pending())),
            }
            match self.transport.read(&mut buf) {
                Ok(0) => return Ok(Recv::Eof(self.decoder.take_pending())),
                Ok(n) => {
                    if let Some(trace) = &mut self.trace {
                        trace.extend_from_slice(&buf[..n]);
                    }
                    self.decoder.feed(&buf[..n]);
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    return Ok(Recv::Timeout);
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e.into()),
            }
        }
    }

    /// Reads one complete message; EOF or bad framing before its end is an error.
    pub fn recv_message(&mut self) -> Result<Vec<u8>, Error> {
        let mut message = Vec::new();
        loop {
            match self.recv(None)? {
                Recv::Data(data) => message.extend(data),
                Recv::EndOfMessage => return Ok(message),
                Recv::Timeout => {}
                Recv::Eof(_) => return Err(FramingError::UnexpectedEof.into()),
                Recv::Framing(e, _) => return Err(e.into()),
            }
        }
    }

    /// Ends the session, asking the server to close it first if `send_close_session`.
    /// Best effort: the server may already be gone, which is not an error at this point.
    pub fn close(&mut self, send_close_session: bool) -> Result<(), Error> {
        if send_close_session
            && self
                .send_message(rpc(&CloseSession, 0, "").as_bytes())
                .is_ok()
        {
            let _ = self.recv_message();
        }
        self.transport.close()?;
        Ok(())
    }

    fn buffer(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.out.extend_from_slice(bytes);
        if self.out.len() >= BUFFER_SIZE {
            self.write_out()?;
        }
        Ok(())
    }

    fn write_out(&mut self) -> io::Result<()> {
        if !self.out.is_empty() {
            self.transport.write_all(&self.out)?;
            self.out.clear();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;
    use crate::protocol::ns::{BASE_1_0, BASE_1_1, NC_NS};
    use NetconfVersion::{V1_0, V1_1};

    enum Item {
        Bytes(Vec<u8>),
        Timeout,
    }

    #[derive(Default)]
    struct Fake {
        incoming: VecDeque<Item>,
        written: Vec<u8>,
        write_calls: usize,
        closed: bool,
    }

    impl Fake {
        /// Delivers `data` one byte per read, the hardest case for the decoder.
        fn bytewise(data: &[u8]) -> Self {
            Fake {
                incoming: data.iter().map(|b| Item::Bytes(vec![*b])).collect(),
                ..Fake::default()
            }
        }
    }

    impl Transport for Fake {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            match self.incoming.pop_front() {
                Some(Item::Bytes(b)) => {
                    let n = b.len().min(buf.len());
                    buf[..n].copy_from_slice(&b[..n]);
                    Ok(n)
                }
                Some(Item::Timeout) => Err(io::ErrorKind::WouldBlock.into()),
                None => Ok(0),
            }
        }

        fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
            self.written.extend_from_slice(data);
            self.write_calls += 1;
            Ok(())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }

        fn set_read_timeout(&mut self, _: Option<Duration>) -> io::Result<()> {
            Ok(())
        }

        fn close(&mut self) -> io::Result<()> {
            self.closed = true;
            Ok(())
        }
    }

    fn server_hello(caps: &[&str]) -> Vec<u8> {
        let items: String = caps
            .iter()
            .map(|c| format!("<capability>{c}</capability>"))
            .collect();
        format!("<hello xmlns=\"{NC_NS}\"><capabilities>{items}</capabilities></hello>]]>]]>")
            .into_bytes()
    }

    #[test]
    fn hello_exchange_switches_to_chunked_when_both_support_it() {
        let mut incoming = server_hello(&[BASE_1_0, BASE_1_1]);
        incoming.extend(b"\n#2\nok\n##\n");
        let mut s = Session::new(Fake::bytewise(&incoming));

        let hello = s.hello_exchange(&[V1_0, V1_1], true).unwrap();
        assert_eq!(hello.capabilities, [BASE_1_0, BASE_1_1]);
        assert_eq!(s.framing(), Framing::Chunked);
        assert!(s.transport.written.ends_with(b"</hello>]]>]]>"));

        s.transport.written.clear();
        s.send_message(b"<x/>").unwrap();
        assert_eq!(s.transport.written, b"\n#4\n<x/>\n##\n");
        assert_eq!(s.recv_message().unwrap(), b"ok");
    }

    #[test]
    fn hello_exchange_stays_on_1_0_if_server_lacks_1_1() {
        let mut s = Session::new(Fake::bytewise(&server_hello(&[BASE_1_0])));
        s.hello_exchange(&[V1_0, V1_1], true).unwrap();
        assert_eq!(s.framing(), Framing::Eom);
    }

    #[test]
    fn file_sessions_send_no_client_hello() {
        let mut s = Session::new(Fake::bytewise(&server_hello(&[BASE_1_0])));
        s.hello_exchange(&[V1_0], false).unwrap();
        assert!(s.transport.written.is_empty());
    }

    #[test]
    fn timeouts_are_reported_and_reading_can_continue() {
        let mut fake = Fake::default();
        fake.incoming.push_back(Item::Timeout);
        fake.incoming.push_back(Item::Bytes(b"abc]]>]]>".to_vec()));
        let mut s = Session::new(fake);
        assert_eq!(s.recv(Some(Duration::from_secs(1))).unwrap(), Recv::Timeout);
        assert_eq!(s.recv(None).unwrap(), Recv::Data(b"abc".to_vec()));
        assert_eq!(s.recv(None).unwrap(), Recv::EndOfMessage);
    }

    #[test]
    fn eof_reports_undelivered_bytes() {
        let mut s = Session::new(Fake::bytewise(b"abc]]"));
        assert_eq!(s.recv(None).unwrap(), Recv::Data(b"a".to_vec()));
        assert_eq!(s.recv(None).unwrap(), Recv::Data(b"b".to_vec()));
        assert_eq!(s.recv(None).unwrap(), Recv::Data(b"c".to_vec()));
        assert_eq!(s.recv(None).unwrap(), Recv::Eof(b"]]".to_vec()));
    }

    #[test]
    fn recv_message_fails_on_eof_and_on_framing_errors() {
        let mut s = Session::new(Fake::bytewise(b"partial"));
        assert_eq!(
            s.recv_message().unwrap_err().to_string(),
            "unexpected EOF in NETCONF transport"
        );

        let mut s = Session::new(Fake::bytewise(b"\n#x\n"));
        s.decoder.set_framing(Framing::Chunked);
        assert_eq!(
            s.recv_message().unwrap_err().to_string(),
            "NETCONF transport framing error"
        );
    }

    #[test]
    fn small_chunks_are_batched_into_few_writes() {
        let mut s = Session::new(Fake::default());
        for _ in 0..1000 {
            s.send_chunk(&[b'x'; 100]).unwrap();
        }
        s.send_end_of_message().unwrap();
        assert_eq!(s.transport.written.len(), 100_000 + 6);
        assert!(s.transport.write_calls <= 7, "{}", s.transport.write_calls);
    }

    #[test]
    fn trace_records_wire_bytes_including_framing() {
        let mut s = Session::new(Fake::bytewise(b"<a/>]]>]]>"));
        s.set_trace(true);
        s.recv_message().unwrap();
        assert_eq!(s.take_trace(), b"<a/>]]>]]>");
        assert!(s.take_trace().is_empty());
    }

    #[test]
    fn close_asks_the_server_to_close_the_session_first() {
        let mut s = Session::new(Fake::bytewise(b"<rpc-reply/>]]>]]>"));
        s.close(true).unwrap();
        let sent = String::from_utf8(s.transport.written.clone()).unwrap();
        assert!(
            sent.contains("<close-session/>") && sent.contains("message-id=\"0\""),
            "{sent}"
        );
        assert!(s.transport.closed);
    }
}
