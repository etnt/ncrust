//! Runs the real binary against the in-process mock server.

mod common;

use assert_cmd::cargo::cargo_bin_cmd;
use common::assert_xml_eq;
use common::mock_server::{MockServer, REPLY, Step, goodbye, hello_steps, round_trip, script};
use ncrust::output::pretty;
use ncrust::protocol::ns::NC_NS;
use predicates::str::contains;

fn tcp_args(port: u16) -> Vec<String> {
    [
        "--proto",
        "tcp",
        "--host",
        "127.0.0.1",
        "--port",
        &port.to_string(),
    ]
    .map(String::from)
    .to_vec()
}

#[test]
fn get_config_in_plain_style_over_tcp() {
    let server = MockServer::start(script(vec![hello_steps(), round_trip(REPLY), goodbye()]));
    cargo_bin_cmd!("netconf-console")
        .args(tcp_args(server.port))
        .args(["--get-config", "-s", "plain"])
        .assert()
        .success()
        .stdout(REPLY)
        .stderr("");

    let t = server.finish();
    assert_xml_eq(
        &format!(
            "<rpc xmlns=\"{NC_NS}\" message-id=\"1\"><get-config><source><running/></source></get-config></rpc>"
        ),
        &t.received[1],
    );
}

#[test]
fn rpc_is_read_from_standard_input() {
    let server = MockServer::start(script(vec![hello_steps(), round_trip(REPLY), goodbye()]));
    cargo_bin_cmd!("netconf-console")
        .args(tcp_args(server.port))
        .args(["--rpc", "-", "-s", "plain"])
        .write_stdin("<get-schema xmlns=\"urn:x\"><identifier>m</identifier></get-schema>")
        .assert()
        .success()
        .stdout(REPLY);

    let t = server.finish();
    assert_xml_eq(
        &format!(
            "<rpc xmlns=\"{NC_NS}\" message-id=\"1\">\
             <get-schema xmlns=\"urn:x\"><identifier>m</identifier></get-schema></rpc>"
        ),
        &t.received[1],
    );
}

#[test]
fn get_with_xpath_words_and_filters_reaches_the_server() {
    let server = MockServer::start(script(vec![hello_steps(), round_trip(REPLY), goodbye()]));
    cargo_bin_cmd!("netconf-console")
        .args(tcp_args(server.port))
        .args([
            "--get",
            "-x",
            "/a[b",
            "=",
            "'c']",
            "--with-inactive",
            "-s",
            "plain",
        ])
        .assert()
        .success();

    let request = server.finish().received[1].clone();
    assert!(request.contains("select=\"/a[b = 'c']\""), "{request}");
    assert!(request.contains("<with-inactive "), "{request}");
}

#[test]
fn missing_command_is_a_usage_error_with_exit_code_2() {
    cargo_bin_cmd!("netconf-console")
        .assert()
        .code(2)
        .stdout("")
        .stderr(contains("a filename or a command option is required"));
}

#[test]
fn connection_refused_is_reported_on_stdout_with_exit_code_1() {
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    cargo_bin_cmd!("netconf-console")
        .args(tcp_args(port))
        .args(["--hello", "-s", "plain"])
        .assert()
        .code(1)
        .stdout(contains("Failed to connect to 127.0.0.1: "))
        .stderr("");
}

#[test]
fn help_goes_to_stdout_with_exit_code_0() {
    cargo_bin_cmd!("netconf-console")
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("Usage: netconf-console"));
}

#[test]
fn ssh_is_the_default_protocol() {
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    cargo_bin_cmd!("netconf-console")
        .args(["--host", "127.0.0.1", "--port", &port.to_string()])
        .args(["--hello", "-s", "plain"])
        .assert()
        .code(1)
        .stdout(contains("Failed to connect to 127.0.0.1: "));
}

fn formatted(xml: &str) -> String {
    String::from_utf8(pretty::format(xml.as_bytes()).unwrap()).unwrap()
}

#[test]
fn without_a_style_the_reply_is_pretty_printed() {
    let server = MockServer::start(script(vec![hello_steps(), round_trip(REPLY), goodbye()]));
    cargo_bin_cmd!("netconf-console")
        .args(tcp_args(server.port))
        .arg("--get-config")
        .assert()
        .success()
        .stdout(formatted(REPLY));
    server.finish();
}

#[test]
fn interactive_mode_sends_operations_from_stdin_numbered_from_2() {
    let server = MockServer::start(script(vec![
        hello_steps(),
        round_trip(REPLY),
        vec![Step::Close],
    ]));
    cargo_bin_cmd!("netconf-console")
        .args(tcp_args(server.port))
        .arg("-i")
        .write_stdin("<get/>\n\n")
        .assert()
        .success()
        .stdout(contains(
            "* Enter a NETCONF operation, end with an empty line",
        ))
        .stdout(contains(formatted(REPLY)));

    let t = server.finish();
    assert_xml_eq(
        &format!("<rpc xmlns=\"{NC_NS}\" message-id=\"2\"><get/></rpc>"),
        &t.received[1],
    );
}

#[test]
fn a_session_file_is_read_from_standard_input_with_a_dash() {
    use ncrust::protocol::ns::BASE_1_0;
    let session = format!(
        "<hello xmlns=\"{NC_NS}\"><capabilities><capability>{BASE_1_0}</capability></capabilities></hello>]]>]]>\
         <rpc xmlns=\"{NC_NS}\" message-id=\"1\"><get/></rpc>]]>]]>"
    );
    let server = MockServer::start(vec![
        Step::Preamble,
        Step::SendMsg(common::mock_server::server_hello(&[BASE_1_0])),
        Step::Recv,
        Step::Recv,
        Step::SendMsg(REPLY.to_string()),
        Step::Close,
    ]);
    cargo_bin_cmd!("netconf-console")
        .args(tcp_args(server.port))
        .arg("-")
        .write_stdin(session)
        .assert()
        .success()
        .stdout(formatted(REPLY));
    server.finish();
}

#[test]
fn the_external_xmllint_gives_the_same_output_as_the_built_in_formatter() {
    if std::process::Command::new("xmllint")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("xmllint not installed, skipping");
        return;
    }
    let reply = "<rpc-reply xmlns=\"urn:x\"><data><a>é</a><b>\n  <c/></b></data></rpc-reply>";
    for external in [false, true] {
        let server = MockServer::start(script(vec![hello_steps(), round_trip(reply), goodbye()]));
        let mut cmd = cargo_bin_cmd!("netconf-console");
        cmd.args(tcp_args(server.port)).arg("--get-config");
        if external {
            cmd.env("NETCONF_CONSOLE_XMLLINT", "1");
        }
        cmd.assert().success().stdout(formatted(reply));
        server.finish();
    }
}
