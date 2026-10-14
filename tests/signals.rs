//! Ctrl-C, closed pipes and exit codes of the real binary.
#![cfg(feature = "tailf-compat")]
#![cfg(unix)]

mod common;

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use common::mock_server::{MockServer, REPLY, Step, hello_steps, round_trip, script};
use rustix::process::{Pid, Signal, kill_process};

const PATIENCE: Duration = Duration::from_secs(15);

// A child started while another test holds a pipe open would inherit it, and the closed-pipe
// test would never see its pipe close.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn one_at_a_time() -> std::sync::MutexGuard<'static, ()> {
    ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner())
}

fn binary(port: u16) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ncrust"));
    cmd.args(["--proto", "tcp", "--host", "127.0.0.1", "--port"])
        .arg(port.to_string());
    cmd
}

fn wait_for_exit(child: &mut Child) -> std::process::ExitStatus {
    let deadline = Instant::now() + PATIENCE;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        assert!(Instant::now() < deadline, "the program did not end");
        thread::sleep(Duration::from_millis(10));
    }
}

/// Reads the child's standard output on a thread until `needle` has appeared.
fn read_until(child: &mut Child, needle: &str) -> mpsc::Receiver<String> {
    let mut stdout = child.stdout.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    let needle = needle.to_string();
    thread::spawn(move || {
        let mut seen = String::new();
        let mut buf = [0u8; 1024];
        while let Ok(n) = stdout.read(&mut buf) {
            if n == 0 {
                break;
            }
            seen.push_str(&String::from_utf8_lossy(&buf[..n]));
            if seen.contains(&needle) {
                let _ = sender.send(seen);
                return;
            }
        }
    });
    receiver
}

const NOTIFICATION: &str = "<notification xmlns=\"urn:ietf:params:xml:ns:netconf:notification:1.0\">\
     <eventTime>2020-01-01T00:00:00Z</eventTime></notification>";

#[test]
fn ctrl_c_while_waiting_closes_the_connection_and_exits_with_0() {
    let _serial = one_at_a_time();
    let server = MockServer::start(script(vec![
        hello_steps(),
        vec![
            Step::Recv,
            Step::SendMsg(NOTIFICATION.into()),
            Step::WaitForClose,
        ],
    ]));
    let mut child = binary(server.port)
        .args(["--create-subscription", "NETCONF", "-s", "raw"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let printed = read_until(&mut child, "</notification>");
    printed
        .recv_timeout(PATIENCE)
        .expect("the notification was not printed");

    let pid = Pid::from_raw(child.id() as i32).unwrap();
    kill_process(pid, Signal::INT).unwrap();
    let status = wait_for_exit(&mut child);
    assert_eq!(status.code(), Some(0), "{status:?}");

    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert_eq!(stderr, "");
    // The client hung up by itself, without a close-session request.
    let transcript = server.finish();
    assert_eq!(
        transcript.received.len(),
        2,
        "hello and the subscription only"
    );
}

#[test]
fn ctrl_c_before_the_connection_is_made_still_ends_the_program() {
    let _serial = one_at_a_time();
    // A listener that never answers keeps the client waiting for the server's hello.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let mut child = binary(port)
        .args(["--hello", "-s", "plain"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let _held = listener.accept().unwrap();
    // The hello wait is not armed (Python has no handler there either): default action.
    let pid = Pid::from_raw(child.id() as i32).unwrap();
    kill_process(pid, Signal::INT).unwrap();
    let status = wait_for_exit(&mut child);
    assert_eq!(status.code(), None, "ended by the signal: {status:?}");
}

#[test]
fn a_closed_pipe_is_silent() {
    let _serial = one_at_a_time();
    let server = MockServer::start(script(vec![hello_steps(), round_trip(REPLY)]));
    let mut child = binary(server.port)
        .args(["--get-config", "-s", "plain"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Nobody reads the output: writing it fails with a broken pipe.
    drop(child.stdout.take());
    let status = wait_for_exit(&mut child);
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert_eq!(stderr, "", "{status:?}");
    assert_eq!(status.code(), Some(1));
    server.finish();
}
