//! The built-in formatter must produce exactly what `xmllint --format` produced for each fixture.
//! Regenerate the expectations with `tools/gen_pretty_golden.py` after editing `cases.json`.

use std::fs;
use std::path::PathBuf;

use ncrust::output::pretty;
use serde_json::Value;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pretty")
}

#[test]
fn matches_xmllint_on_every_fixture() {
    let cases: Value =
        serde_json::from_str(&fs::read_to_string(fixtures().join("cases.json")).unwrap()).unwrap();
    let cases = cases["cases"].as_array().unwrap();
    assert!(cases.len() >= 60, "fixture list shrank to {}", cases.len());

    let mut failures = Vec::new();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let xml = match case["file"].as_str() {
            Some(file) => fs::read(fixtures().join(file)).unwrap(),
            None => case["xml"].as_str().unwrap().as_bytes().to_vec(),
        };
        let expected_out = fixtures().join("expected").join(format!("{name}.out"));
        let expected_invalid = fixtures().join("expected").join(format!("{name}.invalid"));

        let actual = pretty::try_format(&xml);
        match (fs::read(&expected_out), expected_invalid.exists()) {
            (Ok(expected), _) => match actual {
                Ok(text) if text.as_bytes() == expected => {}
                Ok(text) => failures.push(format!(
                    "{name}: output differs\n  xmllint: {:?}\n  ncrust:  {text:?}",
                    String::from_utf8_lossy(&expected)
                )),
                Err(e) => failures.push(format!("{name}: rejected ({e:?}) but xmllint accepts it")),
            },
            (Err(_), true) => {
                if let Ok(text) = actual {
                    failures.push(format!("{name}: accepted but xmllint rejects it: {text:?}"));
                }
            }
            (Err(_), false) => panic!("{name}: no expectation (run tools/gen_pretty_golden.py)"),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
