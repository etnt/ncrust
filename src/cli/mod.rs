//! Command-line layer: turns argv into a validated `config::Invocation`.
//!
//! May depend on: config, error.

pub mod legacy;
pub mod validate;
