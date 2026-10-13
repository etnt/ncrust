//! Authentication methods.
//!
//! May depend on: config, error, transport::ssh::keys.

use std::path::PathBuf;

use crate::config::PrivateKeySpec;

#[derive(Clone, PartialEq, Eq)]
pub enum AuthMethod {
    Password(String),
    /// A private key without passphrase. `type_hint` is the user's `--privKeyType`.
    PrivateKeyFile {
        path: PathBuf,
        type_hint: Option<String>,
    },
}

// Written by hand so that a password can never end up in a log or a panic message.
impl std::fmt::Debug for AuthMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthMethod::Password(_) => f.write_str("Password(<hidden>)"),
            AuthMethod::PrivateKeyFile { path, type_hint } => f
                .debug_struct("PrivateKeyFile")
                .field("path", path)
                .field("type_hint", type_hint)
                .finish(),
        }
    }
}

impl AuthMethod {
    /// A key file wins over the password, as in the Python script.
    pub fn from_config(password: &str, private_key: Option<&PrivateKeySpec>) -> Self {
        match private_key {
            Some(key) => AuthMethod::PrivateKeyFile {
                path: key.file.clone(),
                type_hint: key.key_type.clone(),
            },
            None => AuthMethod::Password(password.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_hides_the_password() {
        let shown = format!("{:?}", AuthMethod::Password("hunter2".into()));
        assert!(!shown.contains("hunter2"), "{shown}");
    }

    #[test]
    fn a_key_file_wins_over_the_password() {
        let key = PrivateKeySpec {
            file: "id".into(),
            key_type: Some("rsa".into()),
        };
        assert_eq!(
            AuthMethod::from_config("secret", Some(&key)),
            AuthMethod::PrivateKeyFile {
                path: "id".into(),
                type_hint: Some("rsa".into())
            }
        );
        assert_eq!(
            AuthMethod::from_config("secret", None),
            AuthMethod::Password("secret".into())
        );
    }
}
