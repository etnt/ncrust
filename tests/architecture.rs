//! Enforces the module dependency rules documented in each module's header.
//!
//! The scanner only looks at `crate::<module>` paths in `src/`, which is how modules refer to
//! each other. Add a rule here when adding a top-level module.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Top-level module -> top-level modules it may use.
const RULES: &[(&str, &[&str])] = &[
    ("cli", &["config", "error"]),
    ("config", &[]),
    ("error", &[]),
    ("framing", &["error"]),
    ("input", &["config", "error"]),
    ("output", &["config", "error"]),
    ("protocol", &["config", "error"]),
    (
        "runner",
        &[
            "config",
            "error",
            "framing",
            "input",
            "output",
            "protocol",
            "session",
            "transport",
        ],
    ),
    (
        "session",
        &["config", "error", "framing", "protocol", "transport"],
    ),
    ("transport", &["config", "error"]),
];

fn identifier(s: &str) -> String {
    s.chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect()
}

fn strip_line_comments(src: &str) -> String {
    src.lines()
        .map(|line| line.find("//").map_or(line, |i| &line[..i]))
        .collect::<Vec<_>>()
        .join("\n")
}

/// First path segment of each item in a `{a::b, c}` group; `s` starts right after the `{`.
fn group_heads(s: &str) -> Vec<String> {
    let bytes = s.as_bytes();
    let (mut heads, mut depth, mut at_item_start, mut i) = (Vec::new(), 1usize, true, 0usize);
    while i < bytes.len() && depth > 0 {
        match bytes[i] {
            b'{' => {
                depth += 1;
                at_item_start = false;
            }
            b'}' => depth -= 1,
            b',' => at_item_start = depth == 1,
            c if c.is_ascii_whitespace() => {}
            _ if depth == 1 && at_item_start => {
                let id = identifier(&s[i..]);
                at_item_start = false;
                if !id.is_empty() {
                    i += id.len();
                    heads.push(id);
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }
    heads
}

/// Top-level modules referenced as `crate::<module>` in `src`.
fn crate_refs(src: &str) -> BTreeSet<String> {
    let text = strip_line_comments(src);
    let mut refs = BTreeSet::new();
    let mut rest = text.as_str();
    while let Some(pos) = rest.find("crate::") {
        let after = &rest[pos + "crate::".len()..];
        match after.strip_prefix('{') {
            Some(group) => refs.extend(group_heads(group)),
            None => {
                refs.insert(identifier(after));
            }
        }
        rest = after;
    }
    refs
}

fn violations(module: &str, src: &str, allowed: &[&str]) -> Vec<String> {
    crate_refs(src)
        .into_iter()
        .filter(|r| r != module && !allowed.contains(&r.as_str()))
        .map(|r| format!("`{module}` must not depend on `{r}`"))
        .collect()
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn modules_respect_dependency_rules() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);

    let mut sources: BTreeMap<String, String> = BTreeMap::new();
    for file in files {
        let rel = file.strip_prefix(&src).unwrap();
        let top = rel
            .components()
            .next()
            .unwrap()
            .as_os_str()
            .to_string_lossy();
        let module = top.trim_end_matches(".rs").to_string();
        if module == "lib" || module == "main" {
            continue;
        }
        sources
            .entry(module)
            .or_default()
            .push_str(&fs::read_to_string(&file).unwrap());
    }

    let known: BTreeSet<&str> = RULES.iter().map(|(m, _)| *m).collect();
    let found: BTreeSet<&str> = sources.keys().map(String::as_str).collect();
    assert_eq!(found, known, "RULES and the modules in src/ differ");

    let problems: Vec<String> = RULES
        .iter()
        .flat_map(|(module, allowed)| violations(module, &sources[*module], allowed))
        .collect();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn detects_framing_using_transport() {
    let v = violations("framing", "use crate::transport::Transport;", &["error"]);
    assert_eq!(v, ["`framing` must not depend on `transport`"]);
}

#[test]
fn detects_protocol_using_transport() {
    let v = violations(
        "protocol",
        "fn f(t: crate::transport::tcp::Tcp) {}",
        &["config", "error"],
    );
    assert_eq!(v, ["`protocol` must not depend on `transport`"]);
}

#[test]
fn detects_cli_using_runner() {
    let v = violations("cli", "use crate::runner::run;", &["config", "error"]);
    assert_eq!(v, ["`cli` must not depend on `runner`"]);
}

#[test]
fn detects_violation_inside_multiline_group() {
    let src =
        "use crate::{\n    config::Invocation,\n    error::{Error, Stream},\n    runner::run,\n};";
    let v = violations("cli", src, &["config", "error"]);
    assert_eq!(v, ["`cli` must not depend on `runner`"]);
}

#[test]
fn ignores_comments_and_own_module() {
    let src = "// use crate::transport::x;\nuse crate::cli::legacy;\nuse crate::config::Db;";
    assert!(violations("cli", src, &["config", "error"]).is_empty());
}
