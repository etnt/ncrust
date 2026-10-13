//! Private key loading and key type detection.
//!
//! May depend on: error.

/// A warning if `--privKeyType` names another key type than the key file holds. The type is
/// detected from the file, so the hint is not needed; it is only checked.
pub fn type_hint_warning(hint: Option<&str>, algorithm: &str) -> Option<String> {
    let hint = hint.map(str::trim).filter(|h| !h.is_empty())?;
    let matches = match hint {
        "rsa" => algorithm == "ssh-rsa" || algorithm.starts_with("rsa-sha2"),
        "dss" | "dsa" => algorithm == "ssh-dss",
        "ed25519" => algorithm == "ssh-ed25519",
        "ecdsa" => algorithm.starts_with("ecdsa-sha2"),
        _ => false,
    };
    (!matches).then(|| {
        format!("--privKeyType {hint} does not match the key in the file ({algorithm}); using the key as it is")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_and_missing_hints_give_no_warning() {
        for (hint, algorithm) in [
            ("rsa", "ssh-rsa"),
            ("rsa", "rsa-sha2-512"),
            ("ed25519", "ssh-ed25519"),
            ("ecdsa", "ecdsa-sha2-nistp384"),
            ("dss", "ssh-dss"),
        ] {
            assert_eq!(
                type_hint_warning(Some(hint), algorithm),
                None,
                "{hint} {algorithm}"
            );
        }
        assert_eq!(type_hint_warning(None, "ssh-rsa"), None);
        assert_eq!(type_hint_warning(Some(""), "ssh-rsa"), None);
    }

    #[test]
    fn a_wrong_or_unknown_hint_gives_a_warning_naming_both() {
        let w = type_hint_warning(Some("rsa"), "ssh-ed25519").unwrap();
        assert!(w.contains("rsa") && w.contains("ssh-ed25519"), "{w}");
        assert!(type_hint_warning(Some("mystery"), "ssh-rsa").is_some());
    }
}
