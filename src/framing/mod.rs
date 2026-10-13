//! Message framing (RFC 6242), sans-IO: bytes in, frames out.
//!
//! May depend on: error.

pub mod chunked;
pub mod eom;

use crate::error::Error;

/// The two NETCONF framings. `Eom` is the `]]>]]>` end-of-message framing of base:1.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Framing {
    Eom,
    Chunked,
}

impl Framing {
    /// Appends `payload` as part of a message; the message is finished by `end_of_message`.
    pub fn encode_chunk(self, payload: &[u8], out: &mut Vec<u8>) {
        match self {
            Framing::Eom => out.extend_from_slice(payload),
            Framing::Chunked => chunked::encode_chunk(payload, out),
        }
    }

    pub fn end_of_message(self) -> &'static [u8] {
        match self {
            Framing::Eom => eom::MARKER,
            Framing::Chunked => chunked::END,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    /// Message bytes, possibly only a part of a message.
    Data(Vec<u8>),
    EndOfMessage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FramingError {
    #[error("NETCONF transport framing error")]
    BadHeader,
    #[error("NETCONF transport framing error")]
    BadSize,
    #[error("unexpected EOF in NETCONF transport")]
    UnexpectedEof,
}

impl From<FramingError> for Error {
    fn from(e: FramingError) -> Self {
        Error::Stderr(e.to_string())
    }
}

/// Incremental decoder. Feed received bytes, then call `next_frame` until it returns `None`.
#[derive(Debug)]
pub struct Decoder {
    framing: Framing,
    buf: Vec<u8>,
    /// Bytes still to come of the current chunk (chunked framing only).
    chunk_remaining: u64,
}

impl Decoder {
    pub fn new(framing: Framing) -> Self {
        Decoder {
            framing,
            buf: Vec::new(),
            chunk_remaining: 0,
        }
    }

    pub fn framing(&self) -> Framing {
        self.framing
    }

    /// Switches framing at a message boundary; buffered bytes are kept and decoded with the new framing.
    pub fn set_framing(&mut self, framing: Framing) {
        self.framing = framing;
        self.chunk_remaining = 0;
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    pub fn next_frame(&mut self) -> Result<Option<Frame>, FramingError> {
        match self.framing {
            Framing::Eom => Ok(eom::decode(&mut self.buf)),
            Framing::Chunked => chunked::decode(&mut self.buf, &mut self.chunk_remaining),
        }
    }

    /// Bytes received but not yet delivered as frames (shown to the user after EOF or a framing error).
    pub fn take_pending(&mut self) -> Vec<u8> {
        self.chunk_remaining = 0;
        std::mem::take(&mut self.buf)
    }
}
