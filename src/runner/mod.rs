//! Executes an `Invocation`: builds the plan, drives the request/reply loop.
//!
//! May depend on: config, error, input, output, protocol, session, transport.

pub mod interactive;
pub mod plan;
pub mod replies;
pub mod subscription;
