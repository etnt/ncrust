//! Output styles, special actions and streams, run end to end against the mock server.

mod common;

use common::mock_server::{
    CLOSE_REPLY, MockServer, REPLY, Step, goodbye, hello_steps, round_trip, script, server_hello,
};
use common::{assert_xml_eq, run_capture, tcp_invocation, temp_file_with};
use ncrust::config::{
    Action, Db, InputSource, OutputStyle, Payload, ReadOptions, Subscription, UtcTimestamp,
};
use ncrust::output::pretty;
use ncrust::protocol::ns::{BASE_1_0, BASE_1_1, NC_NS};

fn formatted(xml: &str) -> String {
    String::from_utf8(pretty::format(xml.as_bytes()).unwrap()).unwrap()
}

fn get() -> Action {
    Action::Get(ReadOptions::default())
}

const NOTIFICATION: &str = "<notification xmlns=\"urn:ietf:params:xml:ns:netconf:notification:1.0\"><eventTime>2024-01-01T00:00:00Z</eventTime><linkUp/></notification>";
const REPLAY_COMPLETE: &str = "<notification xmlns=\"urn:ietf:params:xml:ns:netconf:notification:1.0\"><eventTime>2024-01-01T00:00:01Z</eventTime><replayComplete/></notification>";

fn subscription(stop_time: Option<&str>) -> Subscription {
    Subscription {
        stream: "NETCONF".into(),
        filter: None,
        start_time: None,
        stop_time: stop_time.map(|t| UtcTimestamp::parse(t).unwrap()),
    }
}

#[test]
fn the_default_style_pretty_prints_the_first_reply_formatted() {
    let server = MockServer::start(script(vec![hello_steps(), round_trip(REPLY), goodbye()]));
    let (result, out, err) = run_capture(&tcp_invocation(server.port, get(), None, 1));
    result.unwrap();
    assert_eq!(out, formatted(REPLY));
    assert!(
        out.starts_with("<?xml version=\"1.0\"?>\n<rpc-reply"),
        "{out}"
    );
    assert_eq!(err, "");
    server.finish();
}

#[test]
fn pretty_skips_the_rest_while_all_prints_every_reply() {
    for (style, expected) in [
        (OutputStyle::Pretty, formatted("<a/>")),
        (OutputStyle::All, formatted("<a/>") + &formatted("<b/>")),
    ] {
        let server = MockServer::start(script(vec![
            hello_steps(),
            round_trip("<a/>"),
            round_trip("<b/>"),
            goodbye(),
        ]));
        let (result, out, _) = run_capture(&tcp_invocation(server.port, get(), Some(style), 2));
        result.unwrap();
        assert_eq!(out, expected, "{style:?}");
        server.finish();
    }
}

#[test]
fn last_prints_only_the_final_reply() {
    let server = MockServer::start(script(vec![
        hello_steps(),
        round_trip("<a/>"),
        round_trip("<b/>"),
        round_trip("<c/>"),
        goodbye(),
    ]));
    let inv = tcp_invocation(server.port, get(), Some(OutputStyle::Last), 3);
    let (result, out, _) = run_capture(&inv);
    result.unwrap();
    assert_eq!(out, formatted("<c/>"));
    server.finish();
}

#[test]
fn noaaa_removes_aaa_and_nacm_subtrees_from_the_formatted_reply() {
    let reply = "<rpc-reply xmlns=\"urn:x\"><data><aaa xmlns=\"urn:aaa\"><u/></aaa><keep/><nacm xmlns=\"urn:n\"><v/></nacm></data></rpc-reply>";
    let server = MockServer::start(script(vec![hello_steps(), round_trip(reply), goodbye()]));
    let inv = tcp_invocation(server.port, get(), Some(OutputStyle::NoAaa), 1);
    let (result, out, _) = run_capture(&inv);
    result.unwrap();
    assert_eq!(
        out,
        "<?xml version=\"1.0\"?>\n<rpc-reply xmlns=\"urn:x\">\n  <data>\n    <keep/>\n  </data>\n</rpc-reply>\n"
    );
    server.finish();
}

#[test]
fn the_hello_action_is_formatted_like_any_reply() {
    let server = MockServer::start(script(vec![hello_steps(), goodbye()]));
    let inv = tcp_invocation(server.port, Action::Hello, None, 1);
    let (result, out, _) = run_capture(&inv);
    result.unwrap();
    assert_eq!(out, formatted(&server_hello(&[BASE_1_0, BASE_1_1])));
    server.finish();
}

#[test]
fn closing_before_the_first_pretty_reply_is_an_error_but_later_is_fine() {
    let server = MockServer::start(script(vec![hello_steps(), vec![Step::Recv, Step::Close]]));
    let (result, out, _) = run_capture(&tcp_invocation(server.port, get(), None, 1));
    assert_eq!(
        result.unwrap_err().to_string(),
        "No data received in the reply."
    );
    assert_eq!(out, "");
    server.finish();

    let server = MockServer::start(script(vec![
        hello_steps(),
        round_trip("<a/>"),
        vec![Step::Recv, Step::Close],
    ]));
    let (result, out, _) = run_capture(&tcp_invocation(server.port, get(), None, 2));
    result.unwrap();
    assert_eq!(out, formatted("<a/>"));
    server.finish();
}

#[test]
fn a_reply_that_is_not_xml_is_reported_like_a_failing_xmllint() {
    let server = MockServer::start(script(vec![hello_steps(), round_trip("<a><b></a>")]));
    let (result, out, _) = run_capture(&tcp_invocation(server.port, get(), None, 1));
    assert_eq!(
        result.unwrap_err().to_string(),
        "xmllint failed. Is the returned XML ok?"
    );
    assert_eq!(out, "");
    drop(server);
}

#[test]
fn get_schema_without_a_style_prints_just_the_schema() {
    let reply = format!(
        "<rpc-reply xmlns=\"{NC_NS}\" message-id=\"1\"><data>module m {{ }}\n</data></rpc-reply>"
    );
    let server = MockServer::start(script(vec![hello_steps(), round_trip(&reply), goodbye()]));
    let inv = tcp_invocation(server.port, Action::GetSchema("m".into()), None, 1);
    let (result, out, err) = run_capture(&inv);
    result.unwrap();
    assert_eq!((out.as_str(), err.as_str()), ("module m { }\n", ""));

    let t = server.finish();
    assert!(t.received[1].contains("<get-schema xmlns=\"urn:ietf:params:xml:ns:yang:ietf-netconf-monitoring\"><identifier>m</identifier></get-schema>"), "{}", t.received[1]);
}

#[test]
fn get_schema_problems_go_to_stderr_and_the_run_succeeds() {
    let reply = format!("<rpc-reply xmlns=\"{NC_NS}\" message-id=\"1\"><rpc-error/></rpc-reply>");
    let server = MockServer::start(script(vec![hello_steps(), round_trip(&reply), goodbye()]));
    let inv = tcp_invocation(server.port, Action::GetSchema("m".into()), None, 1);
    let (result, out, err) = run_capture(&inv);
    result.unwrap();
    assert_eq!(
        (out.as_str(), err.as_str()),
        ("", "Didn't get data in the reply")
    );
    server.finish();
}

#[test]
fn get_schema_with_a_style_prints_the_whole_reply() {
    let reply = format!("<rpc-reply xmlns=\"{NC_NS}\" message-id=\"1\"><data>x</data></rpc-reply>");
    let server = MockServer::start(script(vec![hello_steps(), round_trip(&reply), goodbye()]));
    let inv = tcp_invocation(
        server.port,
        Action::GetSchema("m".into()),
        Some(OutputStyle::Pretty),
        1,
    );
    let (result, out, _) = run_capture(&inv);
    result.unwrap();
    assert_eq!(out, formatted(&reply));
    server.finish();
}

#[test]
fn simple_operations_send_their_request_and_print_the_reply() {
    let server = MockServer::start(script(vec![hello_steps(), round_trip(REPLY), goodbye()]));
    let inv = tcp_invocation(server.port, Action::Commit, Some(OutputStyle::Plain), 1);
    let (result, out, _) = run_capture(&inv);
    result.unwrap();
    assert_eq!(out, REPLY);
    let t = server.finish();
    assert_xml_eq(
        &format!("<rpc xmlns=\"{NC_NS}\" message-id=\"1\"><commit/></rpc>"),
        &t.received[1],
    );
}

#[test]
fn edit_config_streams_the_file_inside_its_envelope_with_the_trace_id() {
    let file = temp_file_with(b"<a><b>1</b></a>");
    let action = Action::EditConfig(Payload {
        db: Db("candidate".into()),
        input: InputSource::File(file.path().to_path_buf()),
        trace_id: Some("6b8a4c52-a4c3-4a3f-8e0e-8a2f5c1b9d11".into()),
    });
    let server = MockServer::start(script(vec![hello_steps(), round_trip(REPLY), goodbye()]));
    let (result, out, _) = run_capture(&tcp_invocation(
        server.port,
        action,
        Some(OutputStyle::Plain),
        1,
    ));
    result.unwrap();
    assert_eq!(out, REPLY);

    let t = server.finish();
    let expected = format!(
        "<rpc xmlns=\"{NC_NS}\" message-id=\"1\"><edit-config xmlns:nc=\"{NC_NS}\"><target><candidate/></target>\
         <trace-id xmlns=\"http://tail-f.com/ns/netconf/ncs\">6b8a4c52-a4c3-4a3f-8e0e-8a2f5c1b9d11</trace-id>\
         <config><a><b>1</b></a></config></edit-config></rpc>"
    );
    assert_xml_eq(&expected, &t.received[1]);
}

#[test]
fn a_payload_larger_than_one_megabyte_arrives_intact() {
    let payload = format!("<big>{}</big>", "x".repeat(1_100_000));
    let file = temp_file_with(payload.as_bytes());
    let action = Action::EditData(Payload {
        db: Db::default(),
        input: InputSource::File(file.path().to_path_buf()),
        trace_id: None,
    });
    let server = MockServer::start(script(vec![hello_steps(), round_trip(REPLY), goodbye()]));
    let (result, _, _) = run_capture(&tcp_invocation(
        server.port,
        action,
        Some(OutputStyle::Plain),
        1,
    ));
    result.unwrap();

    let received = &server.finish().received[1];
    assert!(
        received.contains(&payload),
        "payload corrupted or truncated"
    );
    assert!(received.ends_with("</config></edit-data></rpc>"));
}

#[test]
fn a_subscription_is_requested_once_and_notifications_are_printed_until_the_server_closes() {
    let server = MockServer::start(script(vec![
        hello_steps(),
        vec![
            Step::Recv,
            Step::SendMsg(REPLY.to_string()),
            Step::SendMsg(NOTIFICATION.to_string()),
            Step::SendMsg(NOTIFICATION.to_string()),
            Step::Close,
        ],
    ]));
    let action = Action::CreateSubscription(subscription(None));
    let (result, out, _) = run_capture(&tcp_invocation(server.port, action, None, 1));
    result.unwrap();
    assert_eq!(
        out,
        formatted(REPLY) + &formatted(NOTIFICATION) + &formatted(NOTIFICATION)
    );

    let t = server.finish();
    // One request and no close-session: the connection was already closed by the server.
    assert_eq!(t.received.len(), 2, "{:?}", t.received);
    assert!(t.received[1].contains("<create-subscription"));
}

#[test]
fn a_subscription_ends_after_the_replay_when_the_stop_time_has_passed() {
    let server = MockServer::start(script(vec![
        hello_steps(),
        vec![
            Step::Recv,
            Step::SendMsg(REPLY.to_string()),
            Step::SendMsg(REPLAY_COMPLETE.to_string()),
        ],
        goodbye(),
    ]));
    let action = Action::EstablishSubscription(subscription(Some("2000-01-01T00:00:00")));
    let (result, out, _) = run_capture(&tcp_invocation(server.port, action, None, 1));
    result.unwrap();
    assert_eq!(out, formatted(REPLY) + &formatted(REPLAY_COMPLETE));

    let t = server.finish();
    assert!(t.received[1].contains("<establish-subscription"));
    assert!(
        t.received[2].contains("<close-session/>"),
        "{:?}",
        t.received
    );
}

#[test]
fn a_session_file_is_sent_as_is_and_replies_are_read_until_the_server_closes() {
    let session = format!(
        "<hello xmlns=\"{NC_NS}\"><capabilities><capability>{BASE_1_0}</capability></capabilities></hello>]]>]]>\
         <rpc xmlns=\"{NC_NS}\" message-id=\"1\"><get/></rpc>]]>]]>\
         <rpc xmlns=\"{NC_NS}\" message-id=\"2\"><close-session/></rpc>]]>]]>"
    );
    let file = temp_file_with(session.as_bytes());
    let server = MockServer::start(vec![
        Step::Preamble,
        Step::SendMsg(server_hello(&[BASE_1_0])),
        Step::Recv,
        Step::Recv,
        Step::SendMsg(REPLY.to_string()),
        Step::Recv,
        Step::SendMsg(CLOSE_REPLY.to_string()),
        Step::Close,
    ]);
    let action = Action::SessionFile(InputSource::File(file.path().to_path_buf()));
    let (result, out, _) = run_capture(&tcp_invocation(
        server.port,
        action,
        Some(OutputStyle::All),
        1,
    ));
    result.unwrap();
    assert_eq!(out, formatted(REPLY) + &formatted(CLOSE_REPLY));

    let t = server.finish();
    assert_eq!(
        t.received.len(),
        3,
        "the tool must not add a hello or a close-session"
    );
    assert!(t.received[1].contains("<get/>"));
}
