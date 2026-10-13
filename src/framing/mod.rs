//! Message framing (RFC 6242), sans-IO: bytes in, frames out.
//!
//! May depend on: error.

pub mod chunked;
pub mod eom;
