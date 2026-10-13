//! Authentication methods.
//!
//! May depend on: config, error, transport::ssh::keys.

use std::path::PathBuf;

use crate::config::PrivateKeySpec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthMethod {
    Password(String),
    /// A private key without passphrase. `type_hint` is the user's `--privKeyType`.
    PrivateKeyFile {
        path: PathBuf,
        type_hint: Option<String>,
    },
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
