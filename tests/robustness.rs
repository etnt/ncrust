//! Robustness against hostile or broken servers: whatever arrives on the wire, nothing panics,
//! memory stays bounded, and splitting the input differently changes nothing.
//!
//! The number of random cases is 2 000 by default; set `PROPTEST_CASES` for long runs
//! (`tools/fuzz_long.sh` runs this file for an hour). Inputs that once mattered are kept in
//! `tests/fuzz_corpus/` and replayed on every run.

use std::fs;
use std::path::PathBuf;

use ncrust::config::NetconfVersion;
use ncrust::framing::{Decoder, Frame, Framing, FramingError};
use ncrust::output::{noaaa, pretty, schema};
use ncrust::protocol::hello::{client_hello, negotiate, parse_server_hello};
use proptest::prelude::*;

/// What a decoder produces for some input, independent of how the input was split.
#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    data: Vec<u8>,
    messages: usize,
    error: Option<FramingError>,
    pending: Vec<u8>,
}

fn decode(framing: Framing, input: &[u8], read_sizes: &[usize]) -> Outcome {
    let mut decoder = Decoder::new(framing);
    let mut outcome = Outcome {
        data: Vec::new(),
        messages: 0,
        error: None,
        pending: Vec::new(),
    };
    let mut rest = input;
    let mut sizes = read_sizes.iter().cycle();
    'input: while !rest.is_empty() {
        let n = (*sizes.next().unwrap()).clamp(1, rest.len());
        decoder.feed(&rest[..n]);
        rest = &rest[n..];
        loop {
            match decoder.next_frame() {
                Ok(Some(Frame::Data(d))) => outcome.data.extend(d),
                Ok(Some(Frame::EndOfMessage)) => outcome.messages += 1,
                Ok(None) => break,
                Err(e) => {
                    outcome.error = Some(e);
                    break 'input;
                }
            }
        }
    }
    outcome.pending = decoder.take_pending();
    outcome
}

fn wire_bytes() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        // Anything at all.
        prop::collection::vec(any::<u8>(), 0..300),
        // Things that look like framing: headers, sizes, markers, newlines.
        prop::collection::vec(
            prop_oneof![
                Just(b"\n#".to_vec()),
                Just(b"\n##\n".to_vec()),
                Just(b"]]>]]>".to_vec()),
                Just(b"\n".to_vec()),
                Just(b"#".to_vec()),
                "[0-9]{1,12}".prop_map(String::into_bytes),
                prop::collection::vec(any::<u8>(), 0..8),
            ],
            0..30,
        )
        .prop_map(|parts| parts.concat()),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(
        std::env::var("PROPTEST_CASES").ok().and_then(|v| v.parse().ok()).unwrap_or(2000)
    ))]

    #[test]
    fn the_decoder_never_panics_and_does_not_invent_data(
        input in wire_bytes(),
        read_sizes in prop::collection::vec(1usize..64, 1..6),
    ) {
        for framing in [Framing::Eom, Framing::Chunked] {
            let outcome = decode(framing, &input, &read_sizes);
            prop_assert!(outcome.data.len() + outcome.pending.len() <= input.len());
        }
    }

    #[test]
    fn splitting_the_input_changes_nothing(
        input in wire_bytes(),
        first in prop::collection::vec(1usize..64, 1..6),
        second in prop::collection::vec(1usize..64, 1..6),
    ) {
        for framing in [Framing::Eom, Framing::Chunked] {
            let whole = decode(framing, &input, &[usize::MAX]);
            let split = decode(framing, &input, &first);
            let other = decode(framing, &input, &second);
            prop_assert_eq!(&whole.data, &split.data);
            prop_assert_eq!(whole.messages, split.messages);
            prop_assert_eq!(whole.error, split.error);
            prop_assert_eq!(&split.data, &other.data);
            prop_assert_eq!(split.messages, other.messages);
            prop_assert_eq!(split.error, other.error);
        }
    }

    #[test]
    fn a_huge_announced_chunk_does_not_allocate_it(digits in "[1-9][0-9]{9,30}") {
        let mut decoder = Decoder::new(Framing::Chunked);
        decoder.feed(format!("\n#{digits}\nabc").as_bytes());
        // Either rejected or delivered as it arrives; never a giant buffer.
        match decoder.next_frame() {
            Ok(Some(Frame::Data(d))) => prop_assert!(d.len() <= 3),
            Ok(Some(Frame::EndOfMessage)) => prop_assert!(false, "no message end possible"),
            Ok(None) | Err(_) => {}
        }
        prop_assert!(decoder.take_pending().len() <= 16 + digits.len());
    }

    #[test]
    fn the_hello_parser_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..400)) {
        let _ = parse_server_hello(&bytes);
    }

    #[test]
    fn mutated_hellos_never_panic(
        cut in 0usize..400,
        flips in prop::collection::vec((0usize..400, any::<u8>()), 0..6),
    ) {
        let mut hello = client_hello(&[NetconfVersion::V1_0, NetconfVersion::V1_1]).into_bytes();
        for (at, byte) in flips {
            if !hello.is_empty() {
                let at = at % hello.len();
                hello[at] = byte;
            }
        }
        hello.truncate(cut.max(1));
        if let Ok(capabilities) = parse_server_hello(&hello) {
            let _ = negotiate(&[NetconfVersion::V1_0, NetconfVersion::V1_1], &capabilities);
        }
    }

    #[test]
    fn the_formatters_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..400)) {
        let _ = pretty::format(&bytes);
        let _ = schema::extract(&bytes);
        let _ = noaaa::filter(&bytes);
    }

    #[test]
    fn the_formatters_survive_deep_and_wide_documents(depth in 1usize..400, width in 0usize..50) {
        let mut xml = String::new();
        for i in 0..depth {
            xml.push_str(&format!("<a{i} xmlns=\"urn:{i}\" x=\"{width}\">"));
            xml.push_str(&"t ".repeat(width));
        }
        for i in (0..depth).rev() {
            xml.push_str(&format!("</a{i}>"));
        }
        let _ = pretty::format(xml.as_bytes());
        let _ = parse_server_hello(xml.as_bytes());
    }
}

/// Characters that XML 1.0 can carry at all (even escaped).
fn xml_char() -> impl Strategy<Value = char> {
    prop_oneof![
        8 => proptest::char::range('\u{20}', '\u{D7FF}'),
        2 => proptest::char::range('\u{E000}', '\u{FFFD}'),
        1 => proptest::char::range('\u{10000}', '\u{10FFFF}'),
        1 => proptest::sample::select(vec!['\t', '\n', '\r', '<', '>', '&', '"', '\'']),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(
        std::env::var("PROPTEST_CASES").ok().and_then(|v| v.parse().ok()).unwrap_or(2000)
    ))]

    /// Text and attribute values from the command line cannot break out of their place.
    #[test]
    fn escaped_text_and_attributes_come_back_unchanged(
        text in prop::collection::vec(xml_char(), 0..60),
        value in prop::collection::vec(xml_char(), 0..60),
    ) {
        use ncrust::protocol::xmlw::XmlWriter;
        let text: String = text.into_iter().collect();
        let value: String = value.into_iter().collect();
        let mut w = XmlWriter::new();
        w.start("a", &[("k", &value)]).text(&text).end();
        let xml = w.finish();
        let doc = roxmltree::Document::parse(&xml).expect("well-formed");
        let root = doc.root_element();
        prop_assert_eq!(root.attribute("k"), Some(value.as_str()));
        // Parsers turn a literal CR LF into LF; the writer escapes CR only in attributes.
        let got: String = root.children().filter_map(|n| n.text()).collect();
        prop_assert_eq!(got.replace("\r\n", "\n").replace('\r', "\n"), text.replace("\r\n", "\n").replace('\r', "\n"));
        prop_assert_eq!(root.children().filter(|n| n.is_element()).count(), 0);
    }

    /// An XPath given on the command line ends up as one attribute value of the request.
    #[test]
    fn a_hostile_xpath_stays_inside_its_attribute(body in prop::collection::vec(xml_char(), 0..60)) {
        use ncrust::cli::{Parsed, parse};
        use ncrust::runner::request::build_operation;
        let xpath = format!("/a{}b", body.into_iter().collect::<String>().replace(['\r', '\n', '\t'], " "));
        let args = vec!["--get".to_string(), "-x".to_string(), xpath.clone()];
        let Ok(Parsed::Run(invocation)) = parse(&args) else {
            return Ok(());
        };
        let operation = build_operation(&invocation.action).unwrap().unwrap();
        let message = ncrust::protocol::ops::rpc(operation.as_ref(), 1, "");
        let doc = roxmltree::Document::parse(&message).expect("well-formed request");
        let selects: Vec<&str> = doc
            .descendants()
            .filter(|n| n.is_element())
            .filter_map(|n| n.attribute("select"))
            .collect();
        prop_assert_eq!(selects, vec![xpath.trim()]);
        // Nothing but get and its filter inside the rpc.
        let names: Vec<&str> = doc.descendants().filter(|n| n.is_element()).map(|n| n.tag_name().name()).collect();
        prop_assert_eq!(names, vec!["rpc", "get", "filter"]);
    }
}

#[test]
fn saved_inputs_are_handled() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fuzz_corpus");
    let mut count = 0;
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let bytes = fs::read(&path).unwrap();
        for framing in [Framing::Eom, Framing::Chunked] {
            let whole = decode(framing, &bytes, &[usize::MAX]);
            let bytewise = decode(framing, &bytes, &[1]);
            // What was still buffered at an error depends on how much had been read.
            assert_eq!(
                (whole.data, whole.messages, whole.error),
                (bytewise.data, bytewise.messages, bytewise.error),
                "{}",
                path.display()
            );
        }
        let _ = parse_server_hello(&bytes);
        let _ = pretty::format(&bytes);
        let _ = schema::extract(&bytes);
        let _ = noaaa::filter(&bytes);
        count += 1;
    }
    assert!(count >= 10, "the corpus is missing");
}
