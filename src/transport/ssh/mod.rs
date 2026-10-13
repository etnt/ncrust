//! SSH transport. All cryptographic choices (algorithms, keys, host keys) stay in this
//! module tree so that post-quantum algorithms are a change here only.
//!
//! May depend on: config, error.

pub mod algorithms;
pub mod auth;
pub mod backend;
pub mod hostkey;
pub mod keys;
#[cfg(feature = "russh")]
pub mod russh_backend;
#[cfg(feature = "ssh-subprocess")]
pub mod subprocess_backend;
