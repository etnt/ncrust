//! Command lines must be read as the Python script reads them.
#![cfg(feature = "tailf-compat")]
//!
//! `tests/golden/cli_expected.json` is made by `tools/gen_cli_golden.py` from the Python
//! script: the values optparse produces for each command line in `cli_cases.json`, the
//! script's output for command lines that it rejects, and the list of its options.

use std::fs;
use std::path::PathBuf;

use clap::CommandFactory;
use ncrust::cli::legacy::{Kind, options};
use ncrust::cli::{Cli, Raw, parse, parse_cli};
use ncrust::error::Error;
use serde_json::{Map, Value, json};

fn expected() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/cli_expected.json");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e} (run tools/gen_cli_golden.py)", path.display()));
    serde_json::from_str(&text).unwrap()
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

fn text(value: &Option<String>) -> Value {
    json!(value.clone().unwrap_or_default())
}

/// The values of `cli` named and normalised like Python's `optparse` result.
fn python_view(cli: &Cli) -> Map<String, Value> {
    let database = |value: &Option<Option<String>>| match value {
        None => Value::Null,
        Some(None) => json!("default"),
        Some(Some(db)) => json!(db),
    };
    let pairs = [
        ("version", json!(cli.version)),
        ("debug", json!(cli.debug)),
        ("username", json!(cli.user.as_deref().unwrap_or("admin"))),
        (
            "password",
            json!(cli.password.as_deref().unwrap_or("admin")),
        ),
        ("proto", json!(cli.proto)),
        ("host", json!(cli.host)),
        ("port", json!(cli.port)),
        ("listen", json!(cli.listen)),
        ("listen_address", json!(cli.listen_address)),
        ("listen_port", json!(cli.listen_port)),
        ("iter", json!(cli.iter)),
        ("interactive", json!(cli.interactive)),
        (
            "style",
            json!(cli.output_style.clone().unwrap_or_else(|| "default".into())),
        ),
        ("privKeyType", text(&cli.priv_key_type)),
        ("privKeyFile", text(&cli.priv_key_file)),
        ("groups", text(&cli.groups)),
        ("supgroups", text(&cli.sup_groups)),
        ("hello", json!(cli.hello)),
        ("get", json!(cli.get)),
        ("getConfig", database(&cli.get_config)),
        ("getData", database(&cli.get_data)),
        ("max_depth", text(&cli.max_depth)),
        ("config_filter", json!(cli.config_filter)),
        ("subtree_filter", text(&cli.subtree_filter)),
        ("with_origin", json!(cli.with_origin)),
        ("origin_filter", json!(cli.origin_filter)),
        ("negated_origin_filter", json!(cli.negated_origin_filter)),
        ("db", json!(cli.db)),
        ("wdefaults", text(&cli.with_defaults)),
        ("winactive", json!(cli.with_inactive)),
        ("xpath", json!(cli.xpath.join(" "))),
        ("kill_session", text(&cli.kill_session)),
        ("discard_changes", json!(cli.discard_changes)),
        ("commit", json!(cli.commit)),
        ("validate", json!(cli.validate)),
        (
            "copy_running_to_startup",
            json!(cli.copy_running_to_startup),
        ),
        ("copy", text(&cli.copy_config)),
        ("trace_id", json!(cli.trace_id)),
        ("edit", text(&cli.edit_config)),
        ("edit_data", text(&cli.edit_data)),
        ("get_schema", json!(cli.get_schema)),
        ("create_subscription", json!(cli.create_subscription)),
        ("establish_subscription", json!(cli.establish_subscription)),
        ("kill_subscription", json!(cli.kill_subscription)),
        ("start_time", text(&cli.start_time)),
        ("rpc", text(&cli.rpc)),
        ("stop_time", text(&cli.stop_time)),
        ("rpc_attribute", json!(cli.rpc_attribute)),
    ];
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

/// Python's flags are `None` until given.
fn python_values(values: &Map<String, Value>) -> Map<String, Value> {
    let mut out = Map::new();
    for (key, value) in values {
        let normal = match (key.as_str(), value) {
            ("xpath", Value::String(s)) => json!(s.trim()),
            (_, Value::Null) if is_flag(key) => json!(false),
            _ => value.clone(),
        };
        out.insert(key.clone(), normal);
    }
    for key in ["getConfig", "getData"] {
        out.entry(key).or_insert(Value::Null);
    }
    out
}

fn is_flag(dest: &str) -> bool {
    [
        "debug",
        "listen",
        "interactive",
        "hello",
        "get",
        "with_origin",
        "winactive",
        "discard_changes",
        "commit",
        "validate",
        "copy_running_to_startup",
    ]
    .contains(&dest)
}

#[test]
fn parsed_values_equal_what_optparse_gives() {
    let expected = expected();
    let cases = expected["parse"].as_array().unwrap();
    assert!(cases.len() >= 40, "the table must have at least 40 vectors");
    let mut failures = Vec::new();
    for case in cases {
        let args = strings(&case["args"]);
        let result = parse_cli(&args);
        if let Some(message) = case.get("error") {
            let want = format!("error: {}", message.as_str().unwrap());
            match result {
                Err(Error::Usage(got)) if got == want => {}
                other => failures.push(format!("{args:?}: expected {want:?}, got {other:?}")),
            }
        } else if case.get("help").is_some() {
            if !matches!(result, Ok(Raw::Help(_))) {
                failures.push(format!("{args:?}: expected help, got {result:?}"));
            }
        } else if case["positional"].as_array().unwrap().len() > 1 {
            // Python ignores them all; ncrust rejects (see DEVIATIONS.md).
            if !matches!(result, Err(Error::Usage(_))) {
                failures.push(format!("{args:?}: expected a usage error, got {result:?}"));
            }
        } else {
            let Ok(Raw::Cli(cli)) = result else {
                failures.push(format!("{args:?}: expected values, got {result:?}"));
                continue;
            };
            let want = python_values(case["values"].as_object().unwrap());
            let got = python_view(&cli);
            for (key, value) in &want {
                if got.get(key) != Some(value) {
                    failures.push(format!(
                        "{args:?}: {key}: python {value}, ncrust {}",
                        got.get(key).unwrap_or(&Value::Null)
                    ));
                }
            }
            let positional = case["positional"].as_array().unwrap();
            let filename = cli.filename.as_deref();
            if positional.first().and_then(Value::as_str) != filename {
                failures.push(format!("{args:?}: filename {filename:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_python_option_is_known_with_the_same_kind() {
    let expected = expected();
    let ours = options(&Cli::command());
    let mut python_names = Vec::new();
    for option in expected["options"].as_array().unwrap() {
        let longs = strings(&option["longs"]);
        let shorts = strings(&option["shorts"]);
        let kind = match option["kind"].as_str().unwrap() {
            "flag" => Kind::Flag,
            "value" => Kind::Value,
            "int" => Kind::Int,
            "optional" => Kind::Optional,
            "greedy" => Kind::Greedy,
            other => panic!("unknown kind {other}"),
        };
        let found = ours
            .iter()
            .find(|o| longs.iter().all(|l| o.longs.contains(l)))
            .unwrap_or_else(|| panic!("option --{} is missing", longs[0]));
        assert_eq!(found.kind, kind, "--{}", longs[0]);
        assert_eq!(
            found
                .short
                .map(String::from)
                .into_iter()
                .collect::<Vec<_>>(),
            shorts,
            "--{}",
            longs[0]
        );
        python_names.extend(longs);
    }
    let added: Vec<String> = ours
        .iter()
        .flat_map(|o| o.longs.clone())
        .filter(|l| l != "help" && !python_names.contains(l))
        .collect();
    let mut added = added;
    added.sort();
    assert_eq!(
        added,
        [
            "host-key-fingerprint",
            "known-hosts",
            "ssh-ciphers",
            "ssh-host-key-algs",
            "ssh-kex",
            "ssh-macs"
        ],
        "ncrust has options that Python lacks; list them in plan/option-map.md"
    );
}

#[test]
fn rejected_command_lines_fail_like_the_script() {
    let expected = expected();
    for case in expected["invalid"].as_array().unwrap() {
        let args = strings(&case["args"]);
        let err = match parse(&args) {
            Err(err) => err,
            Ok(parsed) => panic!("{args:?}: accepted ({parsed:?})"),
        };
        let exit = case["exit"].as_i64().unwrap();
        assert_eq!(i64::from(err.exit_code()), exit, "{args:?}: {err}");
        if let Some(message) = case["usage_message"].as_str() {
            assert!(matches!(err, Error::Usage(_)), "{args:?}: {err:?}");
            assert_eq!(err.to_string(), format!("error: {message}"), "{args:?}");
        } else {
            let stdout = case["stdout"].as_str().unwrap();
            assert!(matches!(err, Error::Stdout(_)), "{args:?}: {err:?}");
            assert_eq!(format!("{err}\n"), stdout, "{args:?}");
            assert_eq!(case["stderr"].as_str().unwrap(), "", "{args:?}");
        }
    }
}

#[test]
fn help_lists_every_option_and_group() {
    let Ok(Raw::Help(help)) = parse_cli(&["--help".to_string()]) else {
        panic!("no help");
    };
    for option in expected()["options"].as_array().unwrap() {
        for long in strings(&option["longs"]) {
            assert!(
                help.contains(&format!("--{long}")),
                "--{long} is not in the help"
            );
        }
    }
    for group in [
        "Style Options:",
        "SSH Options:",
        "TCP Options:",
        "NETCONF Command Options:",
    ] {
        assert!(help.contains(group), "{group}");
    }
}
