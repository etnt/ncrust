//! Runs the real binary against the in-process mock server.

mod common;

use assert_cmd::cargo::cargo_bin_cmd;
use common::assert_xml_eq;
use common::mock_server::{MockServer, REPLY, goodbye, hello_steps, round_trip, script};
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
fn ssh_is_the_default_protocol_but_not_available_yet() {
    cargo_bin_cmd!("netconf-console")
        .args(["--hello", "-s", "plain"])
        .assert()
        .code(1)
        .stdout(contains("SSH is not implemented yet"));
}
