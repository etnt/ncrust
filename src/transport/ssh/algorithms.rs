//! Ordered, name-based algorithm preference lists (kex, host key, cipher, MAC).
//!
//! May depend on: config, error.

use crate::config::AlgorithmSpecs;
use crate::error::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Kex,
    HostKey,
    Cipher,
    Mac,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Kex => "key exchange",
            Kind::HostKey => "host key",
            Kind::Cipher => "cipher",
            Kind::Mac => "MAC",
        }
    }
}

/// What a back end can do for one kind of algorithm.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AlgorithmSet {
    pub supported: Vec<String>,
    /// The default preference order; a subset of `supported`.
    pub defaults: Vec<String>,
}

/// The lists to offer to the server, most preferred first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAlgorithms {
    pub kex: Vec<String>,
    pub host_key: Vec<String>,
    pub ciphers: Vec<String>,
    pub macs: Vec<String>,
}

/// What the back end supports, per kind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Supported {
    pub kex: AlgorithmSet,
    pub host_key: AlgorithmSet,
    pub ciphers: AlgorithmSet,
    pub macs: AlgorithmSet,
}

pub fn resolve_all(
    specs: &AlgorithmSpecs,
    supported: &Supported,
) -> Result<ResolvedAlgorithms, Error> {
    Ok(ResolvedAlgorithms {
        kex: resolve(Kind::Kex, specs.kex.as_deref(), &supported.kex)?,
        host_key: resolve(
            Kind::HostKey,
            specs.host_key.as_deref(),
            &supported.host_key,
        )?,
        ciphers: resolve(Kind::Cipher, specs.ciphers.as_deref(), &supported.ciphers)?,
        macs: resolve(Kind::Mac, specs.macs.as_deref(), &supported.macs)?,
    })
}

/// Applies an OpenSSH-style list specification to the default list. `*` and `?` match
/// algorithm names. Names the back end does not support are reported before connecting.
pub fn resolve(kind: Kind, spec: Option<&str>, set: &AlgorithmSet) -> Result<Vec<String>, Error> {
    let Some(spec) = spec.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(set.defaults.clone());
    };
    let (edit, list) = match spec.chars().next() {
        Some(c @ ('+' | '-' | '^')) => (Some(c), &spec[1..]),
        _ => (None, spec),
    };
    let tokens: Vec<&str> = list
        .split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .collect();

    // Names a token stands for among the supported ones.
    let expand = |token: &str| -> Result<Vec<String>, Error> {
        if token.contains(['*', '?']) {
            return Ok(set
                .supported
                .iter()
                .filter(|name| glob_match(token, name))
                .cloned()
                .collect());
        }
        if set.supported.iter().any(|name| name == token) {
            return Ok(vec![token.to_string()]);
        }
        Err(Error::Stdout(format!(
            "Unsupported {} algorithm '{token}'. Supported: {}",
            kind.label(),
            set.supported.join(", ")
        )))
    };

    let result = if edit == Some('-') {
        // Removing something that is not there is fine, as in OpenSSH.
        set.defaults
            .iter()
            .filter(|name| !tokens.iter().any(|t| glob_match(t, name)))
            .cloned()
            .collect()
    } else {
        let mut named = Vec::new();
        for token in &tokens {
            for name in expand(token)? {
                if !named.contains(&name) {
                    named.push(name);
                }
            }
        }
        let others = || set.defaults.iter().filter(|d| !named.contains(d)).cloned();
        match edit {
            None => named.clone(),
            Some('+') => others().chain(named.iter().cloned()).collect(),
            _ => named.iter().cloned().chain(others()).collect(),
        }
    };

    if result.is_empty() {
        return Err(Error::Stdout(format!(
            "No {} algorithm left after applying '{spec}'",
            kind.label()
        )));
    }
    Ok(result)
}

/// `*` matches any run of characters and `?` any single one.
fn glob_match(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    let (mut pi, mut ti) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|c| *c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set() -> AlgorithmSet {
        let names = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        AlgorithmSet {
            supported: names(&[
                "mlkem768x25519-sha256",
                "curve25519-sha256",
                "dh-g14",
                "dh-g16",
                "ecdh-p256",
            ]),
            defaults: names(&["curve25519-sha256", "dh-g14", "ecdh-p256"]),
        }
    }

    fn run(spec: Option<&str>) -> Result<Vec<String>, String> {
        resolve(Kind::Kex, spec, &set()).map_err(|e| e.to_string())
    }

    fn list(names: &[&str]) -> Result<Vec<String>, String> {
        Ok(names.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn no_spec_keeps_the_defaults() {
        assert_eq!(
            run(None),
            list(&["curve25519-sha256", "dh-g14", "ecdh-p256"])
        );
        assert_eq!(run(Some("  ")), run(None));
    }

    #[test]
    fn a_plain_list_replaces_the_defaults_in_the_given_order() {
        assert_eq!(
            run(Some("dh-g16, mlkem768x25519-sha256")),
            list(&["dh-g16", "mlkem768x25519-sha256"])
        );
        assert_eq!(run(Some("dh-g14,dh-g14")), list(&["dh-g14"]));
    }

    #[test]
    fn plus_appends_minus_removes_caret_prepends() {
        assert_eq!(
            run(Some("+dh-g16")),
            list(&["curve25519-sha256", "dh-g14", "ecdh-p256", "dh-g16"])
        );
        assert_eq!(
            run(Some("+curve25519-sha256")),
            list(&["dh-g14", "ecdh-p256", "curve25519-sha256"])
        );
        assert_eq!(
            run(Some("-dh-g14")),
            list(&["curve25519-sha256", "ecdh-p256"])
        );
        assert_eq!(run(Some("-dh-g16")), run(None));
        assert_eq!(
            run(Some("^mlkem768x25519-sha256")),
            list(&[
                "mlkem768x25519-sha256",
                "curve25519-sha256",
                "dh-g14",
                "ecdh-p256"
            ])
        );
        assert_eq!(
            run(Some("^ecdh-p256")),
            list(&["ecdh-p256", "curve25519-sha256", "dh-g14"])
        );
    }

    #[test]
    fn wildcards_select_among_the_supported_names() {
        assert_eq!(run(Some("dh-*")), list(&["dh-g14", "dh-g16"]));
        assert_eq!(
            run(Some("-dh-*")),
            list(&["curve25519-sha256", "ecdh-p256"])
        );
        assert_eq!(
            run(Some("+dh-g1?")),
            list(&["curve25519-sha256", "ecdh-p256", "dh-g14", "dh-g16"])
        );
        assert_eq!(run(Some("nothing*,dh-g14")), list(&["dh-g14"]));
    }

    #[test]
    fn unsupported_names_are_reported_with_the_supported_ones() {
        let msg = run(Some("+nonsense")).unwrap_err();
        assert!(
            msg.starts_with(
                "Unsupported key exchange algorithm 'nonsense'. Supported: mlkem768x25519-sha256, "
            ),
            "{msg}"
        );
        assert!(run(Some("dh-g14,bogus")).is_err());
    }

    #[test]
    fn an_empty_result_is_an_error() {
        assert_eq!(
            run(Some("-*")).unwrap_err(),
            "No key exchange algorithm left after applying '-*'"
        );
        assert!(run(Some("nothing*")).is_err());
    }

    #[test]
    fn resolve_all_handles_each_kind_with_its_own_label() {
        let supported = Supported {
            kex: set(),
            host_key: set(),
            ciphers: set(),
            macs: set(),
        };
        let specs = AlgorithmSpecs {
            ciphers: Some("bogus".into()),
            ..AlgorithmSpecs::default()
        };
        let err = resolve_all(&specs, &supported).unwrap_err().to_string();
        assert!(
            err.starts_with("Unsupported cipher algorithm 'bogus'"),
            "{err}"
        );
        let ok = resolve_all(&AlgorithmSpecs::default(), &supported).unwrap();
        assert_eq!(ok.macs, set().defaults);
    }

    #[test]
    fn glob_matching() {
        for (p, t, expected) in [
            ("a*", "abc", true),
            ("*c", "abc", true),
            ("a*c", "abc", true),
            ("a*c", "ab", false),
            ("a?c", "abc", true),
            ("a?c", "ac", false),
            ("*", "", true),
            ("", "", true),
            ("a", "ab", false),
            ("*a*b*", "xxaxxbxx", true),
        ] {
            assert_eq!(glob_match(p, t), expected, "{p} vs {t}");
        }
    }
}
