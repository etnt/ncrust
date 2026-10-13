//! Executes an `Invocation`: builds the plan, drives the request/reply loop.
//!
//! May depend on: config, error, framing, input, output, protocol, session, transport.

pub mod interactive;
pub mod plan;
pub mod replies;
pub mod request;
pub mod subscription;

use std::io::Write;

use crate::config::{ConnectionSpec, Invocation, NetconfVersion, OutputStyle, TransportKind};
use crate::error::Error;
use crate::input;
use crate::protocol::message_id::MessageIds;
use crate::protocol::ops::{raw_rpc, rpc};
use crate::session::Session;
use crate::transport::Transport;
use crate::transport::tcp::TcpTransport;
use plan::{Plan, RequestSource};
use replies::{ReplyEnd, flush_trace, read_reply};

/// Connects as described by `invocation` and carries out its action; replies go to `out`.
pub fn run(invocation: &Invocation, out: &mut dyn Write) -> Result<(), Error> {
    let plan = plan::plan(invocation)?;
    let transport = connect(&invocation.connection)?;
    execute(Session::new(transport), invocation, &plan, out)
}

fn connect(spec: &ConnectionSpec) -> Result<Box<dyn Transport>, Error> {
    match &spec.kind {
        TransportKind::Tcp { groups, sup_gids } => Ok(Box::new(TcpTransport::connect(
            &spec.host,
            spec.port,
            &spec.username,
            groups,
            sup_gids,
        )?)),
        TransportKind::Ssh { .. } => Err(Error::Stdout(
            "SSH is not implemented yet; use --proto tcp".into(),
        )),
    }
}

fn execute<T: Transport>(
    mut session: Session<T>,
    invocation: &Invocation,
    plan: &Plan,
    out: &mut dyn Write,
) -> Result<(), Error> {
    session.set_trace(plan.style == OutputStyle::Raw);

    let versions = match invocation.version {
        None => vec![NetconfVersion::V1_0, NetconfVersion::V1_1],
        Some(version) => vec![version],
    };
    let hello = session.hello_exchange(&versions, true);
    flush_trace(&mut session, out)?;
    let hello = hello?;

    if plan.show_hello {
        out.write_all(&hello.raw)?;
    }
    if let RequestSource::RpcFile(source) = &plan.request {
        send_rpc_file(&mut session, source, &invocation.rpc_attribute)?;
    }

    let operation = request::build_operation(&invocation.action)?;
    let mut ids = MessageIds::starting_at(1);
    let mut printed = false;
    let mut closed = false;
    let mut remaining = plan.replies;
    while remaining != 0 && !closed {
        if let Some(operation) = &operation {
            let message = rpc(operation.as_ref(), ids.next_id(), &invocation.rpc_attribute);
            session.send_message(message.as_bytes())?;
        }
        // Plain prints one reply and skips the rest.
        let print = plan.style == OutputStyle::Plain && !printed;
        closed = read_reply(&mut session, out, print)? == ReplyEnd::ConnectionClosed;
        printed = true;
        remaining -= 1;
    }

    let result = session.close(!closed);
    flush_trace(&mut session, out)?;
    result
}

fn send_rpc_file<T: Transport>(
    session: &mut Session<T>,
    source: &crate::config::InputSource,
    rpc_attribute: &str,
) -> Result<(), Error> {
    session.send_chunk(raw_rpc::start(1, rpc_attribute).as_bytes())?;
    input::for_each_chunk(&mut input::open(source)?, |chunk| session.send_chunk(chunk))?;
    session.send_chunk(raw_rpc::END.as_bytes())?;
    session.send_end_of_message()
}
