//! Requests built by ncrust must equal (as canonical XML) what the Python script sends.
//!
//! Every case in `golden/cases.json` is a command line. `tools/gen_golden.py` runs it with the
//! Python script against a fake server and stores the requests it received; here the same
//! command line goes through ncrust's argument parser and request builders.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use common::assert_xml_eq;
use ncrust::cli::{Parsed, parse};
use ncrust::config::{Action, NetconfVersion};
use ncrust::protocol::hello::client_hello;
use ncrust::runner::request::render;
use serde_json::Value;

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn cases() -> Value {
    serde_json::from_str(&fs::read_to_string(golden_dir().join("cases.json")).unwrap()).unwrap()
}

fn read(path: PathBuf) -> String {
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e} (run tools/gen_golden.py)", path.display()))
}

fn file_count(dir: PathBuf) -> usize {
    fs::read_dir(dir).unwrap().count()
}

#[test]
fn hello_messages_match_python() {
    let cases = cases();
    let hello = cases["hello"].as_array().unwrap();
    assert_eq!(
        file_count(golden_dir().join("hello")),
        hello.len(),
        "golden/hello has files not listed in cases.json"
    );
    for case in hello {
        let versions: Vec<NetconfVersion> = case["versions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| match v.as_str().unwrap() {
                "1.0" => NetconfVersion::V1_0,
                "1.1" => NetconfVersion::V1_1,
                other => panic!("unknown version {other}"),
            })
            .collect();
        let name = case["name"].as_str().unwrap();
        let expected = read(golden_dir().join("hello").join(format!("{name}.xml")));
        assert_xml_eq(&expected, &client_hello(&versions));
    }
}

#[test]
fn requests_match_python() {
    let cases = cases();
    let cases = cases["cases"].as_array().unwrap();
    assert_eq!(
        file_count(golden_dir().join("requests")),
        cases.len(),
        "golden/requests has directories not listed in cases.json"
    );

    for case in cases {
        let name = case["name"].as_str().unwrap();

        // Input files named in the arguments as {f:name}.
        let dir = tempfile::tempdir().unwrap();
        let mut files = BTreeMap::new();
        for (file, content) in case["files"].as_object().into_iter().flatten() {
            let path = dir.path().join(file);
            fs::write(&path, content.as_str().unwrap()).unwrap();
            files.insert(file.clone(), path.to_string_lossy().into_owned());
        }
        let args: Vec<String> = case["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| {
                let mut arg = a.as_str().unwrap().to_string();
                for (file, path) in &files {
                    arg = arg.replace(&format!("{{f:{file}}}"), path);
                }
                arg
            })
            .collect();

        let Ok(Parsed::Run(invocation)) = parse(&args) else {
            panic!("{name}: arguments not accepted: {args:?}");
        };

        let expected_dir = golden_dir().join("requests").join(name);
        let count = file_count(expected_dir.clone());
        // Generated requests are repeated per iteration, with growing message ids.
        let repeated = matches!(invocation.action, Action::Get(_));
        assert_eq!(
            count,
            if repeated {
                invocation.iterations as usize
            } else {
                1
            },
            "{name}"
        );
        for id in 1..=count {
            let expected = read(expected_dir.join(format!("{id}.xml")));
            let actual = render(&invocation, id as u32)
                .unwrap_or_else(|e| panic!("{name}: {e}"))
                .unwrap_or_else(|| panic!("{name}: nothing to send"));
            assert_xml_eq(&without_pythons_xpath_space(&expected), &actual);
        }
    }
}

/// The Python script sends xpath filters with a leading space (see plan/DEVIATIONS.md).
fn without_pythons_xpath_space(xml: &str) -> String {
    xml.replace("select=\" /", "select=\"/")
        .replace("select=' /", "select='/")
        .replace("<xpath-filter> /", "<xpath-filter>/")
        .replace(
            "ietf-subscribed-notifications\"> /",
            "ietf-subscribed-notifications\">/",
        )
}
