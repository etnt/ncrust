//! Runs the real binary over SSH against an in-process SSH server that is bridged to the mock
//! NETCONF server.
#![cfg(feature = "russh")]

mod common;

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use common::mock_server::{MockServer, REPLY, goodbye, round_trip, script, ssh_hello_steps};
use common::ssh_server::{
    ServerOptions, SshServer, fixture, host_key_fingerprint, host_public_key_line,
};
use common::temp_file_with;
use predicates::str::{contains, starts_with};
use russh::kex;

fn session() -> MockServer {
    MockServer::start(script(vec![
        ssh_hello_steps(),
        round_trip(REPLY),
        goodbye(),
    ]))
}

/// `netconf-console --get-config -s plain` aimed at an SSH server on `port`.
fn client(port: u16) -> Command {
    let mut cmd = cargo_bin_cmd!("netconf-console");
    cmd.args(["--host", "127.0.0.1", "--port", &port.to_string()])
        .args(["--get-config", "-s", "plain"]);
    cmd
}

fn key_arg(name: &str) -> String {
    fixture(name).display().to_string()
}

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

#[test]
fn password_login_and_get_config() {
    let mock = session();
    let ssh = SshServer::start(ServerOptions::default(), mock.port);
    client(ssh.port).assert().success().stdout(REPLY).stderr("");
    let transcript = mock.finish();
    assert_eq!(transcript.received.len(), 3); // hello, get-config, close-session
    assert!(transcript.preamble.is_none(), "no TCP preamble over SSH");
}

#[test]
fn a_wrong_password_fails_with_the_python_message() {
    let mock = session();
    let ssh = SshServer::start(ServerOptions::default(), mock.port);
    client(ssh.port)
        .args(["-p", "wrong"])
        .assert()
        .code(1)
        .stdout("Authentication failed.\n");
}

#[test]
fn a_wrong_user_fails_too() {
    let mock = session();
    let ssh = SshServer::start(ServerOptions::default(), mock.port);
    client(ssh.port)
        .args(["-u", "bob"])
        .assert()
        .code(1)
        .stdout(contains("Authentication failed."));
}

#[test]
fn every_kind_of_key_file_works_without_a_type_option() {
    for name in ["user_ed25519", "user_ecdsa", "user_rsa"] {
        let mock = session();
        let ssh = SshServer::start(
            ServerOptions {
                password: None,
                ..ServerOptions::default()
            },
            mock.port,
        );
        client(ssh.port)
            .args(["--privKeyFile", &key_arg(name)])
            .assert()
            .success()
            .stdout(REPLY)
            .stderr("");
        mock.finish();
    }
}

#[test]
fn a_key_file_wins_over_the_password() {
    let mock = session();
    let ssh = SshServer::start(
        ServerOptions {
            password: None,
            ..ServerOptions::default()
        },
        mock.port,
    );
    client(ssh.port)
        .args(["-p", "ignored", "--privKeyFile", &key_arg("user_ed25519")])
        .assert()
        .success();
    mock.finish();
}

#[test]
fn a_key_the_server_does_not_know_is_refused() {
    let mock = session();
    let ssh = SshServer::start(ServerOptions::default(), mock.port);
    client(ssh.port)
        .args(["--privKeyFile", &key_arg("stranger_ed25519")])
        .assert()
        .code(1)
        .stdout("Authentication failed.\n");
}

#[test]
fn a_matching_key_type_hint_is_silent_and_a_wrong_one_is_a_warning() {
    let mock = session();
    let ssh = SshServer::start(ServerOptions::default(), mock.port);
    client(ssh.port)
        .args([
            "--privKeyFile",
            &key_arg("user_ed25519"),
            "--privKeyType",
            "ed25519",
        ])
        .assert()
        .success()
        .stderr("");
    mock.finish();

    let mock = session();
    let ssh = SshServer::start(ServerOptions::default(), mock.port);
    client(ssh.port)
        .args([
            "--privKeyFile",
            &key_arg("user_ed25519"),
            "--privKeyType",
            "rsa",
        ])
        .assert()
        .success()
        .stdout(REPLY)
        .stderr(contains("--privKeyType rsa does not match"));
    mock.finish();
}

#[test]
fn a_key_with_a_passphrase_is_reported() {
    let mock = session();
    let ssh = SshServer::start(ServerOptions::default(), mock.port);
    client(ssh.port)
        .args(["--privKeyFile", &key_arg("user_ed25519_encrypted")])
        .assert()
        .code(1)
        .stdout(contains("Cannot use the private key"));
}

#[test]
fn a_missing_key_file_is_reported() {
    let mock = session();
    let ssh = SshServer::start(ServerOptions::default(), mock.port);
    client(ssh.port)
        .args(["--privKeyFile", "/no/such/key"])
        .assert()
        .code(1)
        .stdout(contains("Cannot use the private key /no/such/key"));
}

#[test]
fn a_refused_netconf_subsystem_is_reported() {
    let ssh = SshServer::start(ServerOptions::default(), free_port());
    client(ssh.port)
        .assert()
        .code(1)
        .stdout(contains("The server refused the netconf subsystem."));
}

#[test]
fn nothing_listening_is_a_connection_failure() {
    client(free_port())
        .assert()
        .code(1)
        .stdout(contains("Failed to connect to 127.0.0.1: "));
}

// Host key policies.

#[test]
fn by_default_any_host_key_is_accepted() {
    let mock = session();
    let ssh = SshServer::start(ServerOptions::default(), mock.port);
    client(ssh.port).assert().success();
    mock.finish();
}

#[test]
fn a_matching_fingerprint_is_accepted() {
    let mock = session();
    let ssh = SshServer::start(ServerOptions::default(), mock.port);
    client(ssh.port)
        .args([
            "--host-key-fingerprint",
            &host_key_fingerprint("host_ed25519"),
        ])
        .assert()
        .success()
        .stdout(REPLY);
    mock.finish();
}

#[test]
fn another_fingerprint_is_refused_and_both_are_shown() {
    let mock = session();
    let ssh = SshServer::start(ServerOptions::default(), mock.port);
    let other = host_key_fingerprint("stranger_ed25519");
    client(ssh.port)
        .args(["--host-key-fingerprint", &other])
        .assert()
        .code(1)
        .stdout(contains("Host key verification failed"))
        .stdout(contains(&other))
        .stdout(contains(host_key_fingerprint("host_ed25519")));
}

fn known_hosts(port: u16, key_file: &str) -> tempfile::NamedTempFile {
    temp_file_with(format!("[127.0.0.1]:{port} {}\n", host_public_key_line(key_file)).as_bytes())
}

#[test]
fn a_known_host_is_accepted() {
    let mock = session();
    let ssh = SshServer::start(ServerOptions::default(), mock.port);
    let file = known_hosts(ssh.port, "host_ed25519");
    client(ssh.port)
        .args(["--known-hosts"])
        .arg(file.path())
        .assert()
        .success()
        .stdout(REPLY);
    mock.finish();
}

#[test]
fn a_changed_host_key_is_refused() {
    let mock = session();
    let ssh = SshServer::start(ServerOptions::default(), mock.port);
    let file = known_hosts(ssh.port, "stranger_ed25519");
    client(ssh.port)
        .args(["--known-hosts"])
        .arg(file.path())
        .assert()
        .code(1)
        .stdout(contains("Host key verification failed"));
}

#[test]
fn an_unlisted_host_is_refused() {
    let mock = session();
    let ssh = SshServer::start(ServerOptions::default(), mock.port);
    let file = known_hosts(ssh.port.wrapping_add(1), "host_ed25519");
    client(ssh.port)
        .args(["--known-hosts"])
        .arg(file.path())
        .assert()
        .code(1)
        .stdout(contains("is not in"));
}

// Algorithms.

fn debug_line(kex: &'static [kex::Name], args: &[&str]) -> assert_cmd::assert::Assert {
    let mock = session();
    let ssh = SshServer::start(
        ServerOptions {
            kex: Some(kex.to_vec()),
            ..ServerOptions::default()
        },
        mock.port,
    );
    client(ssh.port).arg("--debug").args(args).assert()
}

#[test]
fn the_default_exchange_is_classical() {
    debug_line(&[kex::CURVE25519], &[])
        .success()
        .stderr(contains("kex=curve25519-sha256"));
}

#[test]
fn a_post_quantum_exchange_can_be_asked_for() {
    debug_line(
        &[kex::MLKEM768X25519_SHA256],
        &["--ssh-kex", "mlkem768x25519-sha256"],
    )
    .success()
    .stderr(contains("kex=mlkem768x25519-sha256"));
}

#[test]
fn the_post_quantum_exchange_can_be_put_first() {
    debug_line(
        &[kex::MLKEM768X25519_SHA256, kex::CURVE25519],
        &["--ssh-kex", "^mlkem768x25519-sha256"],
    )
    .success()
    .stderr(contains("kex=mlkem768x25519-sha256"));
}

#[test]
fn a_server_that_offers_nothing_the_client_allows_fails_the_handshake() {
    debug_line(&[kex::MLKEM768X25519_SHA256], &[])
        .code(1)
        .stdout(contains("SSH error"));
}

#[test]
fn chosen_cipher_and_host_key_algorithm_are_used() {
    debug_line(
        &[kex::CURVE25519],
        &[
            "--ssh-ciphers",
            "aes256-gcm@openssh.com",
            "--ssh-host-key-algs",
            "ssh-ed25519",
        ],
    )
    .success()
    .stderr(contains("cipher=aes256-gcm@openssh.com"))
    .stderr(contains("host-key=ssh-ed25519"));
}

#[test]
fn an_unsupported_algorithm_fails_before_connecting() {
    // No server at all: the check must come first.
    cargo_bin_cmd!("netconf-console")
        .args(["--host", "127.0.0.1", "--port", &free_port().to_string()])
        .args(["--hello", "--ssh-kex", "bogus-kex"])
        .assert()
        .code(1)
        .stdout(starts_with(
            "Unsupported key exchange algorithm 'bogus-kex'",
        ));
}

// Call home.

#[test]
fn call_home_waits_for_the_server_to_connect() {
    let mock = session();
    let port = free_port();
    let ssh = SshServer::call_home(ServerOptions::default(), mock.port, port);
    cargo_bin_cmd!("netconf-console")
        .args(["--listen", "--listen-address", "127.0.0.1"])
        .args(["--listen-port", &port.to_string()])
        .args(["--get-config", "-s", "plain"])
        .assert()
        .success()
        .stdout(starts_with(format!(
            "** listening on  127.0.0.1 {port}\n** call home from ('127.0.0.1', "
        )))
        .stdout(contains(format!("\n{REPLY}")));
    mock.finish();
    drop(ssh);
}

// Output volume.

#[test]
fn a_large_request_is_sent_in_few_data_packets() {
    const SIZE: usize = 1 << 20;
    let payload = format!("<a xmlns=\"urn:x\">{}</a>", "x".repeat(SIZE));
    let file = temp_file_with(payload.as_bytes());
    let mock = session();
    let ssh = SshServer::start(ServerOptions::default(), mock.port);
    cargo_bin_cmd!("netconf-console")
        .args(["--host", "127.0.0.1", "--port", &ssh.port.to_string()])
        .args(["-s", "plain", "--edit-config"])
        .arg(file.path())
        .assert()
        .success();
    let transcript = mock.finish();
    assert!(transcript.received[1].len() > SIZE);

    // The server accepts packets of up to 32 KiB; the client writes in 16 KiB batches.
    let events = ssh
        .stats
        .data_events
        .load(std::sync::atomic::Ordering::SeqCst);
    let expected = SIZE.div_ceil(16 * 1024);
    assert!(
        (expected..=expected + 8).contains(&events),
        "{events} data packets for {SIZE} bytes"
    );
}
