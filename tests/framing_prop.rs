//! Property tests: any message set survives encode -> arbitrary re-chunking -> decode.

use ncrust::framing::{Decoder, Frame, Framing};
use proptest::prelude::*;

fn encode(framing: Framing, messages: &[Vec<u8>], chunk_size: usize) -> Vec<u8> {
    let mut wire = Vec::new();
    for m in messages {
        for part in m.chunks(chunk_size) {
            framing.encode_chunk(part, &mut wire);
        }
        wire.extend_from_slice(framing.end_of_message());
    }
    wire
}

fn decode(framing: Framing, wire: &[u8], read_sizes: &[usize]) -> Vec<Vec<u8>> {
    let mut decoder = Decoder::new(framing);
    let mut messages = Vec::new();
    let mut current = Vec::new();
    let mut rest = wire;
    let mut sizes = read_sizes.iter().cycle();
    while !rest.is_empty() {
        let n = (*sizes.next().unwrap()).min(rest.len());
        decoder.feed(&rest[..n]);
        rest = &rest[n..];
        while let Some(frame) = decoder.next_frame().expect("valid framing") {
            match frame {
                Frame::Data(d) => current.extend(d),
                Frame::EndOfMessage => messages.push(std::mem::take(&mut current)),
            }
        }
    }
    assert!(current.is_empty() && decoder.take_pending().is_empty());
    messages
}

/// Non-empty messages; the 1.0 end marker must not occur inside a message.
fn messages() -> impl Strategy<Value = Vec<Vec<u8>>> {
    prop::collection::vec(
        prop::collection::vec(any::<u8>(), 1..200).prop_filter("contains the 1.0 marker", |m| {
            !m.windows(6).any(|w| w == b"]]>]]>")
        }),
        1..5,
    )
}

proptest! {
    #[test]
    fn eom_roundtrip(
        msgs in messages(),
        read_sizes in prop::collection::vec(1usize..40, 1..8),
    ) {
        let wire = encode(Framing::Eom, &msgs, usize::MAX);
        prop_assert_eq!(decode(Framing::Eom, &wire, &read_sizes), msgs);
    }

    #[test]
    fn chunked_roundtrip(
        msgs in messages(),
        chunk_size in 1usize..64,
        read_sizes in prop::collection::vec(1usize..40, 1..8),
    ) {
        let wire = encode(Framing::Chunked, &msgs, chunk_size);
        prop_assert_eq!(decode(Framing::Chunked, &wire, &read_sizes), msgs);
    }
}
