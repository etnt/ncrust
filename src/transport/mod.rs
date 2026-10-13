//! Blocking byte-stream transports (TCP IPC, SSH). Knows nothing about NETCONF messages.
//!
//! May depend on: config, error.

pub mod connect;
pub mod ssh;
pub mod tcp;
