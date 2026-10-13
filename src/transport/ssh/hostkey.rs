//! Host key verification policy.
//!
//! May depend on: config, error.

use crate::config::HostKeyCheck;

/// Whether `actual` (the SHA-256 fingerprint of the key the server presented, as `SHA256:...`)
/// satisfies the check. Returns the reason if it does not. Known-hosts files are looked up by
/// the back end, which has the key type.
pub fn check_fingerprint(check: &HostKeyCheck, actual: &str) -> Result<(), String> {
    match check {
        HostKeyCheck::Fingerprint(expected) if expected != actual => Err(format!(
            "Host key verification failed: the server key is {actual} but {expected} was expected"
        )),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_fingerprint_check_compares_fingerprints() {
        let expected = HostKeyCheck::Fingerprint("SHA256:abc".into());
        assert_eq!(check_fingerprint(&expected, "SHA256:abc"), Ok(()));
        assert!(
            check_fingerprint(&expected, "SHA256:xyz")
                .unwrap_err()
                .contains("SHA256:xyz")
        );
        assert_eq!(
            check_fingerprint(&HostKeyCheck::AcceptAny, "anything"),
            Ok(())
        );
        assert_eq!(
            check_fingerprint(&HostKeyCheck::KnownHosts("f".into()), "x"),
            Ok(())
        );
    }
}
