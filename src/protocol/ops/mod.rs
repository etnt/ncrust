//! One file per NETCONF operation; each writes its request body through `protocol::xmlw`.
//!
//! May depend on: config, error, protocol::{filter, ns, xmlw}.

pub mod copy_config;
pub mod edit_config;
pub mod edit_data;
pub mod get;
pub mod get_config;
pub mod get_data;
pub mod get_schema;
pub mod raw_rpc;
pub mod simple;
pub mod subscription;
