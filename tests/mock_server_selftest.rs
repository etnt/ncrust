mod common;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use common::mock_server::{MockServer, Step};
use ncrust::framing::Framing;

fn connect(port: u16) -> TcpStream {
    let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
}

fn read_exactly(stream: &mut TcpStream, n: usize) -> Vec<u8> {
    let mut buf = vec![0; n];
    stream.read_exact(&mut buf).unwrap();
    buf
}

#[test]
fn plays_a_script_in_both_framings_and_records_what_it_received() {
    let server = MockServer::start(vec![
        Step::Preamble,
        Step::SendMsg("<hello/>".into()),
        Step::Recv,
        Step::Framing(Framing::Chunked),
        Step::SendMsgPieces("<reply/>".into(), 3),
        Step::Recv,
        Step::Send(b"tail".to_vec()),
        Step::Close,
    ]);

    let mut client = connect(server.port);
    client
        .write_all(b"[u;1.2.3.4;tcp;1;2;;/;;]\n<hi/>]]>]]>")
        .unwrap();
    assert_eq!(read_exactly(&mut client, 14), b"<hello/>]]>]]>");
    assert_eq!(read_exactly(&mut client, 16), b"\n#8\n<reply/>\n##\n");
    client.write_all(b"\n#5\n<ok/>\n##\n").unwrap();
    assert_eq!(read_exactly(&mut client, 4), b"tail");

    let transcript = server.finish();
    assert_eq!(
        transcript.preamble.as_deref(),
        Some("[u;1.2.3.4;tcp;1;2;;/;;]\n")
    );
    assert_eq!(transcript.received, ["<hi/>", "<ok/>"]);
}

#[test]
#[should_panic(expected = "client closed the connection unexpectedly")]
fn a_script_fails_when_the_client_disappears() {
    let server = MockServer::start(vec![Step::Recv]);
    drop(connect(server.port));
    server.finish();
}
