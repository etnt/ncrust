//! Requests built by ncrust must equal (as canonical XML) what the Python script builds.
//! Regenerate the expected files with `tools/gen_golden.py` after editing `golden/cases.json`.

mod common;

use std::fs;
use std::path::PathBuf;

use common::{assert_xml_eq, temp_file_with};
use ncrust::config::{Action, Db, Filter, InputSource, NetconfVersion, ReadOptions, WithDefaults};
use ncrust::protocol::hello::client_hello;
use ncrust::protocol::ops::rpc;
use ncrust::runner::request::build_operation;
use serde_json::Value;

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn cases() -> Value {
    serde_json::from_str(&fs::read_to_string(golden_dir().join("cases.json")).unwrap()).unwrap()
}

fn golden(dir: &str, name: &str) -> String {
    let path = golden_dir().join(dir).join(format!("{name}.xml"));
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e} (run tools/gen_golden.py)", path.display()))
}

fn assert_no_stale_files(dir: &str, expected: usize) {
    let count = fs::read_dir(golden_dir().join(dir)).unwrap().count();
    assert_eq!(
        count, expected,
        "golden/{dir} has files not listed in cases.json"
    );
}

#[test]
fn hello_messages_match_python() {
    let cases = cases();
    let hello = cases["hello"].as_array().unwrap();
    assert_no_stale_files("hello", hello.len());
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
        assert_xml_eq(&golden("hello", name), &client_hello(&versions));
    }
}

fn with_defaults(name: &str) -> WithDefaults {
    match name {
        "explicit" => WithDefaults::Explicit,
        "trim" => WithDefaults::Trim,
        "report-all" => WithDefaults::ReportAll,
        "report-all-tagged" => WithDefaults::ReportAllTagged,
        "true" => WithDefaults::LegacyTrue,
        "false" => WithDefaults::LegacyFalse,
        other => panic!("unknown with-defaults {other}"),
    }
}

#[test]
fn get_and_get_config_requests_match_python() {
    let cases = cases();
    let requests = cases["requests"].as_array().unwrap();
    assert_no_stale_files("requests", requests.len());

    for case in requests {
        let name = case["name"].as_str().unwrap();
        // Kept alive until the request is built; the filter file is read at that point.
        let subtree_file = case["subtree"]
            .as_str()
            .map(|content| temp_file_with(content.as_bytes()));

        // As in Python, an xpath filter wins over a subtree filter.
        let filter = match (case["xpath"].as_str(), &subtree_file) {
            (Some(xpath), _) => Some(Filter::XPath(xpath.to_string())),
            (None, Some(file)) => Some(Filter::Subtree(InputSource::File(
                file.path().to_path_buf(),
            ))),
            (None, None) => None,
        };
        let options = ReadOptions {
            filter,
            with_defaults: case["with_defaults"].as_str().map(with_defaults),
            with_inactive: case["with_inactive"].as_bool().unwrap_or(false),
        };
        let action = match case["cmd"].as_str().unwrap() {
            "get" => Action::Get(options),
            "get-config" => Action::GetConfig {
                db: Db::parse(case["db"].as_str().unwrap()).unwrap(),
                options,
            },
            other => panic!("unknown cmd {other}"),
        };

        let op = build_operation(&action).unwrap().unwrap();
        let actual = rpc(&*op, 1, case["rpc_attribute"].as_str().unwrap_or(""));
        assert_xml_eq(&golden("requests", name), &actual);
    }
}
