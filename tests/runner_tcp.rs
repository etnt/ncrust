//! Runs the whole tool (minus argument parsing) against the in-process mock server over TCP.

mod common;

use common::assert_xml_eq;
use common::mock_server::{
    CLOSE_REPLY, MockServer, REPLY, Step, Transcript, chunked, goodbye, hello_steps, round_trip,
    script, server_hello,
};
use ncrust::config::{
    Action, ConnectionSpec, Db, InputSource, Invocation, NetconfVersion, OutputStyle, ReadOptions,
    TransportKind,
};
use ncrust::protocol::ns::{BASE_1_0, BASE_1_1, NC_NS};

fn invocation(port: u16, action: Action, style: OutputStyle, iterations: i64) -> Invocation {
    Invocation {
        connection: ConnectionSpec {
            host: "127.0.0.1".into(),
            port,
            username: "admin".into(),
            kind: TransportKind::Tcp {
                groups: String::new(),
                sup_gids: String::new(),
            },
            call_home: None,
        },
        action,
        style: Some(style),
        iterations,
        version: None,
        rpc_attribute: String::new(),
        debug: false,
    }
}

fn get_config() -> Action {
    Action::GetConfig {
        db: Db::default(),
        options: ReadOptions::default(),
    }
}

fn run(inv: &Invocation) -> (Result<(), ncrust::error::Error>, String) {
    let mut out = Vec::new();
    let result = ncrust::runner::run(inv, &mut out, &mut Vec::new());
    (result, String::from_utf8(out).unwrap())
}

fn assert_request(transcript: &Transcript, index: usize, id: u32, body: &str) {
    let expected = format!("<rpc xmlns=\"{NC_NS}\" message-id=\"{id}\">{body}</rpc>");
    assert_xml_eq(&expected, &transcript.received[index]);
}

#[test]
fn raw_style_prints_the_wire_bytes_of_every_server_message() {
    let server = MockServer::start(script(vec![hello_steps(), round_trip(REPLY), goodbye()]));
    let (result, out) = run(&invocation(server.port, get_config(), OutputStyle::Raw, 1));
    result.unwrap();

    let hello_wire = format!("{}]]>]]>", server_hello(&[BASE_1_0, BASE_1_1]));
    assert_eq!(
        out,
        format!("{hello_wire}{}{}", chunked(REPLY), chunked(CLOSE_REPLY))
    );

    let t = server.finish();
    assert!(
        t.preamble
            .as_deref()
            .unwrap()
            .starts_with("[admin;127.0.0.1;tcp;")
    );
    assert!(t.received[0].contains(BASE_1_0) && t.received[0].contains(BASE_1_1));
    assert_request(
        &t,
        1,
        1,
        "<get-config><source><running/></source></get-config>",
    );
    assert!(
        t.received[2].contains("<close-session/>") && t.received[2].contains("message-id=\"0\"")
    );
}

#[test]
fn plain_style_prints_the_reply_without_framing() {
    let server = MockServer::start(script(vec![hello_steps(), round_trip(REPLY), goodbye()]));
    let (result, out) = run(&invocation(
        server.port,
        get_config(),
        OutputStyle::Plain,
        1,
    ));
    result.unwrap();
    assert_eq!(out, REPLY);
    server.finish();
}

#[test]
fn iterations_send_numbered_requests_and_plain_prints_only_the_first_reply() {
    let server = MockServer::start(script(vec![
        hello_steps(),
        round_trip("<a/>"),
        round_trip("<b/>"),
        round_trip("<c/>"),
        goodbye(),
    ]));
    let action = Action::Get(ReadOptions::default());
    let (result, out) = run(&invocation(server.port, action, OutputStyle::Plain, 3));
    result.unwrap();
    assert_eq!(out, "<a/>");

    let t = server.finish();
    for (index, id) in [(1, 1), (2, 2), (3, 3)] {
        assert_request(&t, index, id, "<get></get>");
    }
}

#[test]
fn raw_style_prints_every_reply_of_an_iteration() {
    let server = MockServer::start(script(vec![
        hello_steps(),
        round_trip("<a/>"),
        round_trip("<b/>"),
        goodbye(),
    ]));
    let action = Action::Get(ReadOptions::default());
    let (result, out) = run(&invocation(server.port, action, OutputStyle::Raw, 2));
    result.unwrap();
    assert!(
        out.contains(&chunked("<a/>")) && out.contains(&chunked("<b/>")),
        "{out:?}"
    );
    server.finish();
}

#[test]
fn hello_action_prints_the_server_hello() {
    let server = MockServer::start(script(vec![hello_steps(), goodbye()]));
    let (result, out) = run(&invocation(
        server.port,
        Action::Hello,
        OutputStyle::Plain,
        1,
    ));
    result.unwrap();
    assert_eq!(out, server_hello(&[BASE_1_0, BASE_1_1]));
    server.finish();
}

#[test]
fn hello_action_in_raw_style_is_not_printed_twice() {
    let server = MockServer::start(script(vec![hello_steps(), goodbye()]));
    let (result, out) = run(&invocation(server.port, Action::Hello, OutputStyle::Raw, 1));
    result.unwrap();
    let hello_wire = format!("{}]]>]]>", server_hello(&[BASE_1_0, BASE_1_1]));
    assert_eq!(out, format!("{hello_wire}{}", chunked(CLOSE_REPLY)));
    server.finish();
}

#[test]
fn rpc_file_is_wrapped_in_rpc_and_gets_one_reply() {
    let file = common::temp_file_with(
        b"<get-schema xmlns=\"urn:x\"><identifier>m</identifier></get-schema>",
    );
    let server = MockServer::start(script(vec![hello_steps(), round_trip(REPLY), goodbye()]));
    let mut inv = invocation(
        server.port,
        Action::Rpc(InputSource::File(file.path().to_path_buf())),
        OutputStyle::Plain,
        1,
    );
    inv.rpc_attribute = "xmlns:foo=\"urn:foo\" foo:bar=\"1\"".into();
    let (result, out) = run(&inv);
    result.unwrap();
    assert_eq!(out, REPLY);

    let t = server.finish();
    let expected = format!(
        "<rpc xmlns=\"{NC_NS}\" message-id=\"1\" xmlns:foo=\"urn:foo\" foo:bar=\"1\">\
         <get-schema xmlns=\"urn:x\"><identifier>m</identifier></get-schema></rpc>"
    );
    assert_xml_eq(&expected, &t.received[1]);
}

#[test]
fn offering_only_1_0_keeps_end_of_message_framing() {
    let server = MockServer::start(script(vec![
        vec![
            Step::Preamble,
            Step::SendMsg(server_hello(&[BASE_1_0, BASE_1_1])),
            Step::Recv,
        ],
        round_trip(REPLY),
        vec![Step::Recv, Step::SendMsg(CLOSE_REPLY.into()), Step::Close],
    ]));
    let mut inv = invocation(server.port, get_config(), OutputStyle::Plain, 1);
    inv.version = Some(NetconfVersion::V1_0);
    let (result, out) = run(&inv);
    result.unwrap();
    assert_eq!(out, REPLY);

    let t = server.finish();
    assert!(t.received[0].contains(BASE_1_0) && !t.received[0].contains(BASE_1_1));
}

#[test]
fn server_closing_before_a_reply_ends_the_run_quietly() {
    let server = MockServer::start(script(vec![hello_steps(), vec![Step::Recv, Step::Close]]));
    let (result, out) = run(&invocation(
        server.port,
        get_config(),
        OutputStyle::Plain,
        1,
    ));
    result.unwrap();
    assert_eq!(out, "");
    server.finish();
}

#[test]
fn eof_in_the_middle_of_a_reply_is_an_error_after_printing_what_arrived() {
    let server = MockServer::start(script(vec![
        hello_steps(),
        vec![Step::Recv, Step::Send(b"\n#10\nabc".to_vec()), Step::Close],
    ]));
    let (result, out) = run(&invocation(
        server.port,
        get_config(),
        OutputStyle::Plain,
        1,
    ));
    assert_eq!(
        result.unwrap_err().to_string(),
        "unexpected EOF in NETCONF transport"
    );
    assert_eq!(out, "abc");
    server.finish();
}

#[test]
fn a_framing_error_in_the_middle_of_a_reply_is_reported() {
    let server = MockServer::start(script(vec![
        hello_steps(),
        vec![
            Step::Recv,
            Step::Send(b"\n#3\nabc\nxyz".to_vec()),
            Step::Close,
        ],
    ]));
    let (result, out) = run(&invocation(
        server.port,
        get_config(),
        OutputStyle::Plain,
        1,
    ));
    assert_eq!(
        result.unwrap_err().to_string(),
        "NETCONF transport framing error"
    );
    assert_eq!(out, "abc\nxyz");
    server.finish();
}

#[test]
fn connection_refused_is_reported_on_stdout() {
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let (result, _) = run(&invocation(port, get_config(), OutputStyle::Plain, 1));
    let err = result.unwrap_err();
    assert!(
        matches!(&err, ncrust::error::Error::Stdout(m) if m.starts_with("Failed to connect to 127.0.0.1: "))
    );
}
