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

use std::net::TcpStream;

use crate::config::{ConnectionSpec, TransportKind};
use crate::error::Error;
use auth::AuthMethod;
use backend::{SshBackend, SshConnection, SshOptions};

/// An SSH connection that is ready to be made: the settings are checked against what the
/// back end supports, so mistakes are reported before any network traffic.
pub struct Prepared {
    backend: Box<dyn SshBackend>,
    options: SshOptions,
}

impl Prepared {
    // Without `tailf-compat` SSH is the only kind of connection and the pattern always matches.
    #[cfg_attr(not(feature = "tailf-compat"), allow(irrefutable_let_patterns))]
    pub fn new(spec: &ConnectionSpec) -> Result<Self, Error> {
        let TransportKind::Ssh {
            password,
            private_key,
            settings,
        } = &spec.kind
        else {
            return Err(Error::Stderr(
                "internal error: not an SSH connection".into(),
            ));
        };
        let backend = default_backend()?;
        let algorithms = algorithms::resolve_all(&settings.algorithms, &backend.caps().supported)?;
        let options = SshOptions {
            host: spec.host.clone(),
            port: spec.port,
            user: spec.username.clone(),
            auth: AuthMethod::from_config(password, private_key.as_ref()),
            algorithms,
            host_key: settings.host_key.clone(),
        };
        Ok(Prepared { backend, options })
    }

    /// Runs the SSH handshake and authentication over `stream`, a connection to the server
    /// (or from it, for call home).
    pub fn connect(self, stream: TcpStream) -> Result<SshConnection, Error> {
        self.backend.connect(stream, &self.options)
    }
}

fn default_backend() -> Result<Box<dyn SshBackend>, Error> {
    #[cfg(feature = "russh")]
    {
        Ok(Box::new(russh_backend::RusshBackend))
    }
    #[cfg(not(feature = "russh"))]
    {
        Err(Error::Stdout("This build has no SSH support".into()))
    }
}

#[cfg(all(test, feature = "russh"))]
mod tests {
    use super::*;
    use crate::config::{AlgorithmSpecs, SshSettings};

    fn spec(algorithms: AlgorithmSpecs) -> ConnectionSpec {
        ConnectionSpec {
            host: "localhost".into(),
            port: 2022,
            username: "admin".into(),
            kind: TransportKind::Ssh {
                password: "admin".into(),
                private_key: None,
                settings: SshSettings {
                    algorithms,
                    ..SshSettings::default()
                },
            },
            call_home: None,
        }
    }

    #[test]
    fn the_hybrid_post_quantum_exchange_is_first_among_the_defaults() {
        let prepared = Prepared::new(&spec(AlgorithmSpecs::default())).unwrap();
        let kex = &prepared.options.algorithms.kex;
        assert_eq!(kex[0], "mlkem768x25519-sha256", "{kex:?}");
        assert!(kex.iter().any(|n| n == "curve25519-sha256"), "{kex:?}");
        assert!(
            !kex.iter().any(|n| n == "none" || n.starts_with("ext-info")),
            "{kex:?}"
        );
    }

    #[test]
    fn a_post_quantum_exchange_can_be_requested() {
        let specs = AlgorithmSpecs {
            kex: Some("mlkem768x25519-sha256".into()),
            ..AlgorithmSpecs::default()
        };
        let prepared = Prepared::new(&spec(specs)).unwrap();
        assert_eq!(prepared.options.algorithms.kex, ["mlkem768x25519-sha256"]);

        let specs = AlgorithmSpecs {
            kex: Some("^mlkem768x25519-sha256".into()),
            ..AlgorithmSpecs::default()
        };
        let kex = Prepared::new(&spec(specs)).unwrap().options.algorithms.kex;
        assert_eq!(kex[0], "mlkem768x25519-sha256");
        assert!(kex.len() > 1);
    }

    #[test]
    fn unsupported_algorithms_fail_before_connecting() {
        for specs in [
            AlgorithmSpecs {
                kex: Some("+bogus-kex".into()),
                ..AlgorithmSpecs::default()
            },
            AlgorithmSpecs {
                host_key: Some("bogus-key".into()),
                ..AlgorithmSpecs::default()
            },
            AlgorithmSpecs {
                ciphers: Some("bogus-cipher".into()),
                ..AlgorithmSpecs::default()
            },
            AlgorithmSpecs {
                macs: Some("bogus-mac".into()),
                ..AlgorithmSpecs::default()
            },
        ] {
            let Err(err) = Prepared::new(&spec(specs)) else {
                panic!("accepted a bogus algorithm");
            };
            assert!(err.to_string().starts_with("Unsupported "), "{err}");
        }
    }

    #[test]
    fn the_host_key_list_names_the_usual_algorithms() {
        let defaults = Prepared::new(&spec(AlgorithmSpecs::default()))
            .unwrap()
            .options
            .algorithms;
        assert!(defaults.host_key.iter().any(|n| n == "ssh-ed25519"));
        assert!(defaults.host_key.iter().any(|n| n == "rsa-sha2-256"));
        assert!(!defaults.ciphers.is_empty() && !defaults.macs.is_empty());
    }
}
