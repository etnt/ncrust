//! Executes an `Invocation`: connects, runs the plan, closes the session.
//!
//! May depend on: config, error, framing, input, output, protocol, session, transport.

pub mod interactive;
pub mod interrupt;
pub mod plan;
pub mod replies;
pub mod request;
pub mod subscription;

use std::io::{self, BufRead, Write};

use crate::config::{Action, InputSource, Invocation, OutputStyle, TransportKind};
use crate::error::Error;
use crate::input;
use crate::output::{Mode, Printer, schema};
use crate::protocol::message_id::MessageIds;
use crate::protocol::ops::{Streamed, raw_rpc, rpc};
use crate::session::Session;
use crate::transport::Transport;
use crate::transport::connect::{connect as connect_tcp, listen};
use crate::transport::ssh;
#[cfg(feature = "tailf-compat")]
use crate::transport::tcp::TcpTransport;
use interrupt::Interrupt;
use plan::{Plan, Source};
use replies::{ReplyEnd, flush_trace, read_reply};

/// Where output goes and where interactive requests come from.
pub struct Io<'a> {
    pub out: &'a mut dyn Write,
    pub err: &'a mut dyn Write,
    pub stdin: &'a mut dyn BufRead,
}

/// Connects as described by `invocation` and carries out its action.
pub fn run(invocation: &Invocation, out: &mut dyn Write, err: &mut dyn Write) -> Result<(), Error> {
    let plan = plan::plan(invocation);
    if invocation.debug {
        writeln!(out, "DEBUG: pid: {}", std::process::id())?;
        out.flush()?;
    }
    let transport = connect(invocation, out, err)?;
    let external_xmllint = std::env::var("NETCONF_CONSOLE_XMLLINT").is_ok_and(|v| v == "1");
    let printer = Printer::new(plan.style, external_xmllint);

    let stdin = io::stdin();
    let mut locked;
    let mut empty = io::empty();
    // Only interactive mode reads standard input here; payload inputs use it on their own.
    let input: &mut dyn BufRead = if plan.source == Source::Interactive {
        locked = stdin.lock();
        &mut locked
    } else {
        &mut empty
    };
    let mut io = Io {
        out,
        err,
        stdin: input,
    };
    execute(
        Session::new(transport),
        invocation,
        &plan,
        &printer,
        &mut io,
    )
}

fn connect(
    invocation: &Invocation,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<Box<dyn Transport>, Error> {
    let spec = &invocation.connection;
    match &spec.kind {
        #[cfg(feature = "tailf-compat")]
        TransportKind::Tcp { groups, sup_gids } => Ok(Box::new(TcpTransport::connect(
            &spec.host,
            spec.port,
            &spec.username,
            groups,
            sup_gids,
        )?)),
        TransportKind::Ssh { .. } => {
            // Settings are checked first, so a mistake never costs a connection.
            let prepared = ssh::Prepared::new(spec)?;
            let stream = match &spec.call_home {
                Some(call_home) => listen(&call_home.address, call_home.port, out)?,
                None => connect_tcp(&spec.host, spec.port)?,
            };
            let connection = prepared.connect(stream)?;
            for warning in &connection.warnings {
                writeln!(err, "{warning}")?;
            }
            if let (true, Some(negotiated)) = (invocation.debug, &connection.negotiated) {
                writeln!(err, "SSH algorithms: {negotiated}")?;
            }
            Ok(connection.transport)
        }
    }
}

fn execute<T: Transport>(
    mut session: Session<T>,
    invocation: &Invocation,
    plan: &Plan,
    printer: &Printer,
    io: &mut Io,
) -> Result<(), Error> {
    session.set_trace(plan.style == OutputStyle::Raw);

    let hello = session.hello_exchange(&plan.versions, plan.send_client_hello);
    flush_trace(&mut session, io.out)?;
    let hello = hello?;

    send_initial(&mut session, invocation, plan)?;
    if plan.show_hello {
        printer.print_complete(&hello.raw, 1, 0, io.out)?;
    }

    let closed = if plan.schema_text {
        print_schema_text(&mut session, invocation, io)?;
        false
    } else {
        reply_loop(&mut session, invocation, plan, printer, io)?
    };

    let result = session.close(!closed);
    flush_trace(&mut session, io.out)?;
    result
}

/// Sends what is streamed from an input: an operation inside `<rpc>`, or a whole session script.
fn send_initial<T: Transport>(
    session: &mut Session<T>,
    invocation: &Invocation,
    plan: &Plan,
) -> Result<(), Error> {
    match &plan.source {
        Source::Streamed => {
            let Some((streamed, source)) = request::build_streamed(&invocation.action) else {
                return Err(Error::Stderr("internal error: nothing to stream".into()));
            };
            send_streamed(session, &streamed, &source, &invocation.rpc_attribute)
        }
        Source::SessionFile(source) => {
            input::for_each_chunk(&mut input::open(source)?, |chunk| session.send_raw(chunk))?;
            session.flush()
        }
        Source::Nothing | Source::Generated | Source::Interactive => Ok(()),
    }
}

fn send_streamed<T: Transport>(
    session: &mut Session<T>,
    streamed: &Streamed,
    source: &InputSource,
    rpc_attribute: &str,
) -> Result<(), Error> {
    let start = format!("{}{}", raw_rpc::start(1, rpc_attribute), streamed.prefix);
    session.send_chunk(start.as_bytes())?;
    input::for_each_chunk(&mut input::open(source)?, |chunk| session.send_chunk(chunk))?;
    let end = format!("{}{}", streamed.suffix, raw_rpc::END);
    session.send_chunk(end.as_bytes())?;
    session.send_end_of_message()
}

/// `--get-schema` without a style: only the schema text is printed.
fn print_schema_text<T: Transport>(
    session: &mut Session<T>,
    invocation: &Invocation,
    io: &mut Io,
) -> Result<(), Error> {
    let Some(operation) = request::build_operation(&invocation.action)? else {
        return Err(Error::Stderr(
            "internal error: no get-schema request".into(),
        ));
    };
    session.send_message(rpc(operation.as_ref(), 1, &invocation.rpc_attribute).as_bytes())?;
    let reply = session.recv_message();
    flush_trace(session, io.out)?;
    // Problems with the reply are only reported, as the Python script does.
    match schema::extract(&reply?) {
        Ok(text) => io.out.write_all(text.as_bytes())?,
        Err(message) => io.err.write_all(message.as_bytes())?,
    }
    Ok(())
}

/// Sends requests and reads replies as planned. Returns true if the server closed the connection.
fn reply_loop<T: Transport>(
    session: &mut Session<T>,
    invocation: &Invocation,
    plan: &Plan,
    printer: &Printer,
    io: &mut Io,
) -> Result<bool, Error> {
    let operation = request::build_operation(&invocation.action)?;
    // A subscription is requested once, after that the server just sends notifications.
    let request_once = matches!(
        invocation.action,
        Action::CreateSubscription(_) | Action::EstablishSubscription(_)
    );
    let mut ids = MessageIds::starting_at(1);
    // Interactive requests are numbered from 2, as in Python.
    let mut interactive_ids = MessageIds::starting_at(2);

    let mut remaining = plan.replies;
    let per_reply = plan.source == Source::Interactive;
    let whole_run = if per_reply {
        Interrupt::never()
    } else {
        Interrupt::arm()
    };
    let mut seen = 0usize;
    let mut closed = false;
    let mut replay_complete = false;

    while remaining != 0
        && !closed
        && !subscription::stop_reached(
            plan.stop_time,
            replay_complete,
            subscription::now_epoch_secs(),
        )
    {
        match plan.source {
            Source::Generated if !(request_once && seen > 0) => {
                if let Some(operation) = &operation {
                    let message = rpc(operation.as_ref(), ids.next_id(), &invocation.rpc_attribute);
                    session.send_message(message.as_bytes())?;
                }
            }
            Source::Interactive => {
                let id = interactive_ids.next_id();
                if let Some(message) = interactive::read_request(io.stdin, io.out, id)? {
                    session.send_message(message.as_bytes())?;
                }
            }
            _ => {}
        }

        let mode = printer.mode(remaining, seen);
        let mut collected = (mode == Mode::Collect || plan.stop_time.is_some()).then(Vec::new);
        // Waiting for the server can be interrupted; so can the whole run, except while the
        // user is typing a request.
        let waiting = per_reply.then(Interrupt::arm);
        let interrupt = waiting.as_ref().unwrap_or(&whole_run);
        let end = read_reply(
            session,
            io.out,
            mode == Mode::Stream,
            collected.as_mut(),
            interrupt,
        )?;
        drop(waiting);
        match end {
            ReplyEnd::Interrupted => {
                // As in Python: the connection is just closed, no close-session.
                closed = true;
            }
            ReplyEnd::Complete => {
                if let Some(reply) = &collected {
                    if mode == Mode::Collect {
                        printer.print_formatted(reply, io.out)?;
                    }
                    replay_complete |=
                        plan.stop_time.is_some() && subscription::is_replay_complete(reply);
                }
            }
            ReplyEnd::ConnectionClosed => {
                if mode == Mode::Collect && seen == 0 {
                    return Err(Error::Stderr("No data received in the reply.".into()));
                }
                closed = true;
            }
        }
        seen += 1;
        remaining -= 1;
        // Output must be visible reply by reply, notifications arrive over a long time.
        io.out.flush()?;
    }
    Ok(closed)
}
