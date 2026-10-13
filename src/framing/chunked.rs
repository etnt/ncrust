//! Chunked framing of NETCONF 1.1 (byte-counted chunks, `\n##\n` terminator).
//!
//! May depend on: error.

use super::{Frame, FramingError};

pub const END: &[u8] = b"\n##\n";

const MAX_CHUNK_SIZE: u64 = u32::MAX as u64;
/// `\n#` + at most 10 digits is the longest valid header prefix before the terminating `\n`.
const MAX_HEADER_LF_INDEX: usize = 12;

/// Appends one chunk. Sizes count bytes, not characters. Empty payloads produce nothing
/// because a zero-size chunk is illegal.
pub(super) fn encode_chunk(payload: &[u8], out: &mut Vec<u8>) {
    if payload.is_empty() {
        return;
    }
    out.extend_from_slice(format!("\n#{}\n", payload.len()).as_bytes());
    out.extend_from_slice(payload);
}

/// Next frame in `buf`, or `None` if more bytes are needed. `remaining` is the number of bytes
/// still owed by the current chunk.
pub(super) fn decode(
    buf: &mut Vec<u8>,
    remaining: &mut u64,
) -> Result<Option<Frame>, FramingError> {
    if *remaining > 0 {
        if buf.is_empty() {
            return Ok(None);
        }
        let n = buf
            .len()
            .min(usize::try_from(*remaining).unwrap_or(usize::MAX));
        *remaining -= n as u64;
        return Ok(Some(Frame::Data(buf.drain(..n).collect())));
    }

    if buf.first().is_some_and(|&b| b != b'\n') || buf.get(1).is_some_and(|&b| b != b'#') {
        return Err(FramingError::BadHeader);
    }
    if buf.len() < 3 {
        return Ok(None);
    }
    let Some(lf) = buf[2..].iter().position(|&b| b == b'\n').map(|p| p + 2) else {
        // The terminating LF can only be at index 12 or earlier.
        return if buf.len() > MAX_HEADER_LF_INDEX {
            Err(FramingError::BadHeader)
        } else {
            Ok(None)
        };
    };
    if lf > MAX_HEADER_LF_INDEX {
        return Err(FramingError::BadHeader);
    }

    let digits = &buf[2..lf];
    if digits == b"#" {
        buf.drain(..=lf);
        return Ok(Some(Frame::EndOfMessage));
    }
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return Err(FramingError::BadHeader);
    }
    let size: u64 = std::str::from_utf8(digits)
        .ok()
        .and_then(|s| s.parse().ok())
        .ok_or(FramingError::BadHeader)?;
    if !(1..=MAX_CHUNK_SIZE).contains(&size) {
        return Err(FramingError::BadSize);
    }
    buf.drain(..=lf);
    *remaining = size;
    decode(buf, remaining)
}

#[cfg(test)]
mod tests {
    use super::super::{Decoder, Frame, Framing, FramingError};

    fn data(s: &str) -> Frame {
        Frame::Data(s.as_bytes().to_vec())
    }

    fn decode_all(input: &[u8]) -> Result<Vec<Frame>, FramingError> {
        let mut d = Decoder::new(Framing::Chunked);
        d.feed(input);
        let mut frames = Vec::new();
        while let Some(f) = d.next_frame()? {
            frames.push(f);
        }
        Ok(frames)
    }

    #[test]
    fn single_chunk_message() {
        assert_eq!(
            decode_all(b"\n#4\nabcd\n##\n").unwrap(),
            [data("abcd"), Frame::EndOfMessage]
        );
    }

    #[test]
    fn two_chunks_one_message() {
        assert_eq!(
            decode_all(b"\n#2\nab\n#3\ncde\n##\n").unwrap(),
            [data("ab"), data("cde"), Frame::EndOfMessage]
        );
    }

    #[test]
    fn data_split_across_reads_is_delivered_incrementally() {
        let mut d = Decoder::new(Framing::Chunked);
        d.feed(b"\n#5\nab");
        assert_eq!(d.next_frame().unwrap(), Some(data("ab")));
        assert_eq!(d.next_frame().unwrap(), None);
        d.feed(b"cde\n##");
        assert_eq!(d.next_frame().unwrap(), Some(data("cde")));
        assert_eq!(d.next_frame().unwrap(), None);
        d.feed(b"\n");
        assert_eq!(d.next_frame().unwrap(), Some(Frame::EndOfMessage));
    }

    #[test]
    fn header_split_at_every_position() {
        let input = b"\n#10\n0123456789\n##\n";
        for split in 0..=input.len() {
            let mut d = Decoder::new(Framing::Chunked);
            let mut bytes = Vec::new();
            let mut eom = false;
            for part in [&input[..split], &input[split..]] {
                d.feed(part);
                while let Some(f) = d.next_frame().unwrap() {
                    match f {
                        Frame::Data(b) => bytes.extend(b),
                        Frame::EndOfMessage => eom = true,
                    }
                }
            }
            assert_eq!(
                (bytes.as_slice(), eom),
                (&b"0123456789"[..], true),
                "split at {split}"
            );
        }
    }

    #[test]
    fn sizes_count_bytes_not_characters() {
        let mut out = Vec::new();
        Framing::Chunked.encode_chunk("é<a/>".as_bytes(), &mut out);
        assert_eq!(out, "\n#6\né<a/>".as_bytes());
    }

    #[test]
    fn empty_payload_encodes_to_nothing() {
        let mut out = Vec::new();
        Framing::Chunked.encode_chunk(b"", &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn rejects_bad_headers() {
        for input in [
            &b"x"[..],
            b"\nx",
            b"\n#a\n",
            b"\n#\n",
            b"\n#-1\n",
            b"\n#12345678901\n",
            b"\n#1234567890123",
            b"\n##x\n",
        ] {
            assert_eq!(
                decode_all(input),
                Err(FramingError::BadHeader),
                "{:?}",
                String::from_utf8_lossy(input)
            );
        }
    }

    #[test]
    fn rejects_sizes_out_of_range() {
        assert_eq!(decode_all(b"\n#0\n"), Err(FramingError::BadSize));
        assert_eq!(decode_all(b"\n#4294967296\n"), Err(FramingError::BadSize));
        assert_eq!(decode_all(b"\n#4294967295\n").unwrap(), []);
    }

    #[test]
    fn incomplete_header_waits_for_more_bytes() {
        for input in [&b""[..], b"\n", b"\n#", b"\n#12", b"\n##"] {
            assert_eq!(decode_all(input).unwrap(), [], "{input:?}");
        }
    }

    #[test]
    fn switching_framing_keeps_buffered_bytes() {
        let mut d = Decoder::new(Framing::Eom);
        d.feed(b"hello]]>]]>\n#2\nok\n##\n");
        assert_eq!(d.next_frame().unwrap(), Some(data("hello")));
        assert_eq!(d.next_frame().unwrap(), Some(Frame::EndOfMessage));
        d.set_framing(Framing::Chunked);
        assert_eq!(d.next_frame().unwrap(), Some(data("ok")));
        assert_eq!(d.next_frame().unwrap(), Some(Frame::EndOfMessage));
    }
}
