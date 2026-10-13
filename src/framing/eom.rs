//! End-of-message framing of NETCONF 1.0 (`]]>]]>`).
//!
//! May depend on: error.

use super::Frame;

pub const MARKER: &[u8] = b"]]>]]>";

/// Next frame in `buf`, or `None` if more bytes are needed. Bytes that could be the start
/// of the marker are held back until the next read decides.
pub(super) fn decode(buf: &mut Vec<u8>) -> Option<Frame> {
    if let Some(idx) = buf.windows(MARKER.len()).position(|w| w == MARKER) {
        if idx == 0 {
            buf.drain(..MARKER.len());
            return Some(Frame::EndOfMessage);
        }
        return Some(Frame::Data(buf.drain(..idx).collect()));
    }
    let held = (1..MARKER.len())
        .rev()
        .find(|&k| buf.ends_with(&MARKER[..k]))
        .unwrap_or(0);
    let safe = buf.len() - held;
    (safe > 0).then(|| Frame::Data(buf.drain(..safe).collect()))
}

#[cfg(test)]
mod tests {
    use super::super::{Decoder, Frame, Framing};

    fn data(s: &str) -> Frame {
        Frame::Data(s.as_bytes().to_vec())
    }

    fn drain(d: &mut Decoder) -> Vec<Frame> {
        std::iter::from_fn(|| d.next_frame().unwrap()).collect()
    }

    #[test]
    fn single_message() {
        let mut d = Decoder::new(Framing::Eom);
        d.feed(b"<a/>]]>]]>");
        assert_eq!(drain(&mut d), [data("<a/>"), Frame::EndOfMessage]);
    }

    #[test]
    fn marker_split_at_every_position() {
        let input = b"<a/>]]>]]>";
        for split in 0..=input.len() {
            let mut d = Decoder::new(Framing::Eom);
            let mut frames = Vec::new();
            d.feed(&input[..split]);
            frames.extend(drain(&mut d));
            d.feed(&input[split..]);
            frames.extend(drain(&mut d));
            let text: Vec<u8> = frames
                .iter()
                .filter_map(|f| match f {
                    Frame::Data(b) => Some(b.clone()),
                    Frame::EndOfMessage => None,
                })
                .flatten()
                .collect();
            assert_eq!(text, b"<a/>", "split at {split}");
            assert_eq!(
                frames.last(),
                Some(&Frame::EndOfMessage),
                "split at {split}"
            );
        }
    }

    #[test]
    fn bytes_after_marker_belong_to_next_message() {
        let mut d = Decoder::new(Framing::Eom);
        d.feed(b"one]]>]]>two]]>]]>tr");
        assert_eq!(
            drain(&mut d),
            [
                data("one"),
                Frame::EndOfMessage,
                data("two"),
                Frame::EndOfMessage,
                data("tr")
            ]
        );
    }

    #[test]
    fn partial_marker_is_held_back_then_released_if_it_was_data() {
        let mut d = Decoder::new(Framing::Eom);
        d.feed(b"abc]]");
        assert_eq!(drain(&mut d), [data("abc")]);
        d.feed(b"x");
        assert_eq!(drain(&mut d), [data("]]x")]);
        assert!(d.take_pending().is_empty());
    }

    #[test]
    fn pending_bytes_are_reported() {
        let mut d = Decoder::new(Framing::Eom);
        d.feed(b"ab]]>");
        assert_eq!(drain(&mut d), [data("ab")]);
        assert_eq!(d.take_pending(), b"]]>");
    }
}
