//! Random documents are formatted by the built-in formatter and by the real `xmllint --format`;
//! the results must be identical. Skipped when `xmllint` is not installed.

use std::io::Write;
use std::process::{Command, Stdio};

use ncrust::output::pretty;
use proptest::prelude::*;

fn xmllint_available() -> bool {
    Command::new("xmllint").arg("--version").output().is_ok()
}

/// `Some(output)` if xmllint accepts the document.
fn xmllint(xml: &str) -> Option<Vec<u8>> {
    let mut child = Command::new("xmllint")
        .args(["--format", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(xml.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    output.status.success().then_some(output.stdout)
}

fn pick(options: &'static [&'static str]) -> impl Strategy<Value = String> {
    prop::sample::select(options).prop_map(str::to_string)
}

fn text() -> impl Strategy<Value = String> {
    prop::collection::vec(
        pick(&[
            "a", "b", "x y", " ", "  ", "\n", "\t", "é", "日", "&amp;", "&lt;", "&gt;", "&#65;",
            "&#xe9;", "&#13;", "1",
        ]),
        1..5,
    )
    .prop_map(|parts| parts.concat())
}

// Raw carriage returns are left out: libxml2 splits text runs at CR LF, which changes which
// whitespace is dropped, and NETCONF servers do not send them.
fn whitespace() -> impl Strategy<Value = String> {
    pick(&[" ", "  ", "\n", "\n  ", "\n    ", "\t", " \n "])
}

fn leaf() -> impl Strategy<Value = String> {
    prop_oneof![
        3 => text(),
        3 => whitespace(),
        1 => pick(&["<![CDATA[x < y]]>", "<![CDATA[]]>", "<![CDATA[ ]]>", "<![CDATA[a&b]]>"]),
        1 => pick(&["<!-- c -->", "<!---->", "<!-- é -->"]),
        1 => pick(&["<?pi?>", "<?pi data?>", "<?t  spaced ?>"]),
        2 => pick(&["<e/>", "<e></e>", "<e> </e>", "<e\n/>", "<p:e/>"]),
    ]
}

fn attributes() -> impl Strategy<Value = String> {
    let name = pick(&["k1", "k2", "k3", "xmlns", "xmlns:p", "p:k", "xml:space"]);
    let value = pick(&[
        "x", "a b", "é", "&amp;", "&lt;", "&gt;", "&#10;", "&#9;", "&quot;", "&apos;", "a\nb",
        "urn:a", "preserve", "default", "weird", "",
    ]);
    prop::collection::btree_map(name, value, 0..4)
        .prop_map(|m| m.into_iter().collect::<Vec<_>>())
        .prop_shuffle()
        .prop_flat_map(|attrs| {
            let quotes = prop::collection::vec(prop::bool::ANY, attrs.len());
            quotes.prop_map(move |q| {
                attrs
                    .iter()
                    .zip(q)
                    .map(|((k, v), single)| {
                        if single {
                            format!(" {k}='{v}'")
                        } else {
                            format!(" {k}=\"{v}\"")
                        }
                    })
                    .collect::<String>()
            })
        })
}

fn element() -> impl Strategy<Value = String> {
    leaf().prop_recursive(5, 60, 6, |inner| {
        (
            pick(&["a", "b", "c", "p:d", "é"]),
            attributes(),
            prop::collection::vec(inner, 0..6),
        )
            .prop_map(|(name, attrs, children)| {
                format!("<{name}{attrs}>{}</{name}>", children.concat())
            })
    })
}

fn document() -> impl Strategy<Value = String> {
    let declaration = pick(&[
        "",
        "<?xml version=\"1.0\"?>",
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<?xml version='1.0' encoding='utf-8' standalone='yes'?>",
    ]);
    let around = || pick(&["", "\n", "<!-- c -->", "<?pi x?>", "\n<!-- c -->\n"]);
    (
        declaration,
        around(),
        pick(&["a", "b", "r"]),
        attributes(),
        prop::collection::vec(element(), 0..5),
        around(),
    )
        .prop_map(|(decl, before, name, attrs, children, after)| {
            format!(
                "{decl}{before}<{name}{attrs}>{}</{name}>{after}",
                children.concat()
            )
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    #[test]
    fn formats_exactly_like_xmllint(xml in document()) {
        if !xmllint_available() {
            return Ok(());
        }
        let ours = pretty::try_format(xml.as_bytes()).ok().map(String::into_bytes);
        prop_assert_eq!(ours, xmllint(&xml), "input: {:?}", xml);
    }
}
