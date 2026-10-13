//! In-process NETCONF peer for tests: accepts one connection and plays a script.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use ncrust::framing::{Decoder, Frame, Framing};

const IO_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub enum Step {
    /// Read the IPC preamble line sent by the TCP transport and record it.
    Preamble,
    /// Send bytes exactly as given.
    Send(Vec<u8>),
    /// Send a message framed with the current framing.
    SendMsg(String),
    /// Like `SendMsg`, written in pieces of at most `n` bytes, flushed after each piece.
    SendMsgPieces(String, usize),
    /// Receive one complete message (current framing) and record it.
    Recv,
    /// Use this framing from now on, for sending and receiving.
    Framing(Framing),
    /// Close the connection.
    Close,
}

#[derive(Debug, Default)]
pub struct Transcript {
    pub preamble: Option<String>,
    /// Messages received by `Step::Recv`, in order.
    pub received: Vec<String>,
}

pub struct MockServer {
    pub port: u16,
    handle: JoinHandle<Result<Transcript, String>>,
}

impl MockServer {
    pub fn start(steps: Vec<Step>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || {
            let stream = accept(&listener)?;
            play(stream, steps)
        });
        MockServer { port, handle }
    }

    /// Waits for the script to end and returns what the server saw. Panics if the script failed.
    pub fn finish(self) -> Transcript {
        match self.handle.join().expect("mock server thread panicked") {
            Ok(transcript) => transcript,
            Err(e) => panic!("mock server: {e}"),
        }
    }
}

fn accept(listener: &TcpListener) -> Result<TcpStream, String> {
    let deadline = Instant::now() + IO_TIMEOUT;
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).map_err(|e| e.to_string())?;
                stream
                    .set_read_timeout(Some(IO_TIMEOUT))
                    .map_err(|e| e.to_string())?;
                return Ok(stream);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() > deadline {
                    return Err("no client connected".into());
                }
                thread::sleep(Duration::from_millis(2));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

fn play(mut stream: TcpStream, steps: Vec<Step>) -> Result<Transcript, String> {
    let mut framing = Framing::Eom;
    let mut decoder = Decoder::new(framing);
    let mut inbuf: Vec<u8> = Vec::new();
    let mut transcript = Transcript::default();

    for step in steps {
        match step {
            Step::Preamble => {
                let end = loop {
                    if let Some(i) = inbuf.iter().position(|&b| b == b'\n') {
                        break i;
                    }
                    read_more(&mut stream, &mut inbuf)?;
                };
                let line: Vec<u8> = inbuf.drain(..=end).collect();
                transcript.preamble = Some(String::from_utf8_lossy(&line).into_owned());
                decoder.feed(&std::mem::take(&mut inbuf));
            }
            Step::Send(bytes) => send(&mut stream, &bytes)?,
            Step::SendMsg(msg) => send(&mut stream, &frame(framing, &msg))?,
            Step::SendMsgPieces(msg, n) => {
                for piece in frame(framing, &msg).chunks(n) {
                    send(&mut stream, piece)?;
                }
            }
            Step::Recv => {
                // Bytes read before a preamble step may still sit in `inbuf`.
                decoder.feed(&std::mem::take(&mut inbuf));
                let mut message = Vec::new();
                loop {
                    match decoder.next_frame().map_err(|e| e.to_string())? {
                        Some(Frame::Data(d)) => message.extend(d),
                        Some(Frame::EndOfMessage) => break,
                        None => {
                            let mut more = Vec::new();
                            read_more(&mut stream, &mut more)?;
                            decoder.feed(&more);
                        }
                    }
                }
                transcript
                    .received
                    .push(String::from_utf8_lossy(&message).into_owned());
            }
            Step::Framing(f) => {
                framing = f;
                decoder.set_framing(f);
            }
            Step::Close => {
                let _ = stream.shutdown(std::net::Shutdown::Both);
            }
        }
    }
    Ok(transcript)
}

fn frame(framing: Framing, msg: &str) -> Vec<u8> {
    let mut out = Vec::new();
    framing.encode_chunk(msg.as_bytes(), &mut out);
    out.extend_from_slice(framing.end_of_message());
    out
}

fn send(stream: &mut TcpStream, bytes: &[u8]) -> Result<(), String> {
    stream.write_all(bytes).map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())
}

fn read_more(stream: &mut TcpStream, into: &mut Vec<u8>) -> Result<(), String> {
    let mut buf = [0u8; 4096];
    match stream.read(&mut buf) {
        Ok(0) => Err("client closed the connection unexpectedly".into()),
        Ok(n) => {
            into.extend_from_slice(&buf[..n]);
            Ok(())
        }
        Err(e) => Err(format!("read failed: {e}")),
    }
}

// Building blocks for the usual session script.

pub const REPLY: &str = "<rpc-reply xmlns=\"urn:ietf:params:xml:ns:netconf:base:1.0\" message-id=\"1\"><data/></rpc-reply>";
pub const CLOSE_REPLY: &str = "<rpc-reply xmlns=\"urn:ietf:params:xml:ns:netconf:base:1.0\" message-id=\"0\"><ok/></rpc-reply>";

pub fn server_hello(caps: &[&str]) -> String {
    let items: String = caps
        .iter()
        .map(|c| format!("<capability>{c}</capability>"))
        .collect();
    format!(
        "<hello xmlns=\"{}\"><capabilities>{items}</capabilities><session-id>1</session-id></hello>",
        ncrust::protocol::ns::NC_NS
    )
}

/// `msg` as it appears on the wire with chunked framing.
pub fn chunked(msg: &str) -> String {
    format!("\n#{}\n{msg}\n##\n", msg.len())
}

/// Preamble, server hello (both versions), client hello; then both sides use 1.1 framing.
pub fn hello_steps() -> Vec<Step> {
    use ncrust::protocol::ns::{BASE_1_0, BASE_1_1};
    vec![
        Step::Preamble,
        Step::SendMsg(server_hello(&[BASE_1_0, BASE_1_1])),
        Step::Recv,
        Step::Framing(Framing::Chunked),
    ]
}

/// One request/reply round trip.
pub fn round_trip(reply: &str) -> Vec<Step> {
    vec![Step::Recv, Step::SendMsg(reply.to_string())]
}

/// The close-session exchange, then the server hangs up.
pub fn goodbye() -> Vec<Step> {
    vec![
        Step::Recv,
        Step::SendMsg(CLOSE_REPLY.to_string()),
        Step::Close,
    ]
}

pub fn script(parts: Vec<Vec<Step>>) -> Vec<Step> {
    parts.into_iter().flatten().collect()
}
