//! Python `optparse` command-line rules that clap does not provide: abbreviated long options,
//! options that take the next argument whatever it looks like, the optional database of
//! `--get-config`/`--get-data`, the greedy `-x`, and last-one-wins repetition.
//!
//! The arguments are read the way `optparse` reads them and handed to clap in a plain form
//! (`--name=value`, positional arguments after `--`), so clap only has to build the struct.
//!
//! May depend on: error.

use std::collections::VecDeque;

use clap::Command;

use crate::error::Error;

/// How an option consumes arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Flag,
    /// Takes the next argument, whatever it looks like.
    Value,
    /// Like `Value`, the text must be an integer (Python `type="int"`).
    Int,
    /// Takes the next argument only if it does not start with `-` (`--get-config [DB]`).
    Optional,
    /// Takes all following arguments up to the next option (`-x`).
    Greedy,
    Help,
}

#[derive(Debug, Clone)]
pub struct OptionInfo {
    /// The clap argument id, equal to the field name of `Cli`.
    pub id: String,
    /// Long names without the dashes; the first is the primary one.
    pub longs: Vec<String>,
    pub short: Option<char>,
    pub kind: Kind,
}

const INT_OPTIONS: [&str; 4] = ["port", "iter", "kill_subscription", "listen_port"];
const FIRST_WINS: [&str; 2] = ["get_config", "get_data"];
const APPENDING: [&str; 2] = ["origin_filter", "negated_origin_filter"];

/// The options of `cmd`, classified by how Python reads them.
pub fn options(cmd: &Command) -> Vec<OptionInfo> {
    let mut found = vec![OptionInfo {
        id: "help".into(),
        longs: vec!["help".into()],
        short: Some('h'),
        kind: Kind::Help,
    }];
    for arg in cmd.get_arguments().filter(|a| !a.is_positional()) {
        let id = arg.get_id().as_str();
        let mut longs: Vec<String> = arg.get_long().map(str::to_string).into_iter().collect();
        longs.extend(
            arg.get_all_aliases()
                .into_iter()
                .flatten()
                .map(str::to_string),
        );
        let kind = match id {
            "xpath" => Kind::Greedy,
            "get_config" | "get_data" => Kind::Optional,
            _ if INT_OPTIONS.contains(&id) => Kind::Int,
            _ if arg.get_action().takes_values() => Kind::Value,
            _ => Kind::Flag,
        };
        found.push(OptionInfo {
            id: id.to_string(),
            longs,
            short: arg.get_short(),
            kind,
        });
    }
    found
}

#[derive(Debug, PartialEq, Eq)]
pub enum Normalized {
    /// `-h` or `--help` was reached.
    Help,
    /// Arguments for clap.
    Args(Vec<String>),
}

fn usage(message: String) -> Error {
    Error::Usage(format!("error: {message}"))
}

/// Python's `repr()` of a string, as used in `optparse` messages.
fn repr(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let escaped: String = text
        .chars()
        .flat_map(|c| match c {
            '\\' => vec!['\\', '\\'],
            '\n' => vec!['\\', 'n'],
            c if c == quote => vec!['\\', c],
            c => vec![c],
        })
        .collect();
    format!("{quote}{escaped}{quote}")
}

/// `optparse`'s integer syntax: `0x` hex, `0b` binary, a leading `0` means octal.
fn parse_int(text: &str) -> Option<i64> {
    let text = text.trim();
    let lower = text.to_ascii_lowercase();
    if let Some(digits) = lower.strip_prefix("0x") {
        i64::from_str_radix(digits, 16).ok()
    } else if let Some(digits) = lower.strip_prefix("0b") {
        i64::from_str_radix(if digits.is_empty() { "0" } else { digits }, 2).ok()
    } else if text.starts_with('0') {
        i64::from_str_radix(text, 8).ok()
    } else {
        text.parse().ok()
    }
}

struct Reader<'a> {
    opts: &'a [OptionInfo],
    rargs: VecDeque<String>,
    /// Options in the order given, with their value.
    seen: Vec<(usize, Option<String>)>,
    positional: Vec<String>,
}

enum Step {
    Continue,
    Help,
}

impl Reader<'_> {
    /// The option for `--name` (an exact name or a unique prefix) and its full name.
    fn match_long(&self, name: &str) -> Result<(usize, String), Error> {
        let bare = &name[2..];
        let all: Vec<(&str, usize)> = self
            .opts
            .iter()
            .enumerate()
            .flat_map(|(i, o)| o.longs.iter().map(move |l| (l.as_str(), i)))
            .collect();
        if let Some((long, i)) = all.iter().find(|(l, _)| *l == bare) {
            return Ok((*i, format!("--{long}")));
        }
        let mut candidates: Vec<(&str, usize)> = all
            .into_iter()
            .filter(|(l, _)| l.starts_with(bare))
            .collect();
        match candidates.len() {
            0 => Err(usage(format!("no such option: {name}"))),
            1 => Ok((candidates[0].1, format!("--{}", candidates[0].0))),
            _ => {
                candidates.sort();
                let names: Vec<String> = candidates.iter().map(|(l, _)| format!("--{l}")).collect();
                Err(usage(format!(
                    "ambiguous option: {name} ({}?)",
                    names.join(", ")
                )))
            }
        }
    }

    fn long(&mut self) -> Result<Step, Error> {
        let arg = self.rargs.pop_front().expect("checked by the caller");
        let (name, explicit) = match arg.split_once('=') {
            Some((name, value)) => {
                self.rargs.push_front(value.to_string());
                (name.to_string(), true)
            }
            None => (arg, false),
        };
        let (index, shown) = self.match_long(&name)?;
        self.take(index, &shown, explicit)
    }

    fn short(&mut self) -> Result<Step, Error> {
        let arg = self.rargs.pop_front().expect("checked by the caller");
        for (at, ch) in arg.char_indices().skip(1) {
            let shown = format!("-{ch}");
            let Some(index) = self.opts.iter().position(|o| o.short == Some(ch)) else {
                return Err(usage(format!("no such option: {shown}")));
            };
            let takes_argument = matches!(self.opts[index].kind, Kind::Value | Kind::Int);
            let rest = &arg[at + ch.len_utf8()..];
            if takes_argument && !rest.is_empty() {
                self.rargs.push_front(rest.to_string());
            }
            if let Step::Help = self.take(index, &shown, false)? {
                return Ok(Step::Help);
            }
            if takes_argument {
                break;
            }
        }
        Ok(Step::Continue)
    }

    /// Reads what option `index` needs and records it.
    fn take(&mut self, index: usize, shown: &str, explicit: bool) -> Result<Step, Error> {
        let kind = self.opts[index].kind;
        let value = match kind {
            Kind::Help => return Ok(Step::Help),
            Kind::Value | Kind::Int => {
                let Some(value) = self.rargs.pop_front() else {
                    return Err(usage(format!("{shown} option requires 1 argument")));
                };
                if kind == Kind::Int {
                    let number = parse_int(&value).ok_or_else(|| {
                        usage(format!(
                            "option {shown}: invalid integer value: {}",
                            repr(&value)
                        ))
                    })?;
                    Some(number.to_string())
                } else {
                    Some(value)
                }
            }
            _ if explicit => {
                return Err(usage(format!("{shown} option does not take a value")));
            }
            Kind::Flag => None,
            Kind::Optional => match self.rargs.front() {
                Some(next) if !next.starts_with('-') => self.rargs.pop_front(),
                _ => None,
            },
            Kind::Greedy => {
                let mut words = Vec::new();
                while let Some(next) = self.rargs.front() {
                    let long_option = next.starts_with("--") && next.len() > 2;
                    let short_option =
                        next.starts_with('-') && next.len() > 1 && !next.starts_with("--");
                    if long_option || short_option {
                        break;
                    }
                    words.extend(self.rargs.pop_front());
                }
                Some(words.join(" "))
            }
        };
        self.seen.push((index, value));
        Ok(Step::Continue)
    }
}

/// Reads `args` as `optparse` does and returns what clap should see.
pub fn normalize(cmd: &Command, args: &[String]) -> Result<Normalized, Error> {
    let opts = options(cmd);
    let mut reader = Reader {
        opts: &opts,
        rargs: args.iter().cloned().collect(),
        seen: Vec::new(),
        positional: Vec::new(),
    };
    while let Some(arg) = reader.rargs.front() {
        let step = if arg == "--" {
            reader.rargs.pop_front();
            reader.positional.extend(reader.rargs.drain(..));
            break;
        } else if arg.starts_with("--") {
            reader.long()?
        } else if arg.starts_with('-') && arg.len() > 1 {
            reader.short()?
        } else {
            reader.positional.extend(reader.rargs.pop_front());
            Step::Continue
        };
        if let Step::Help = step {
            return Ok(Normalized::Help);
        }
    }

    // Repeating an option: the last one wins, except where Python keeps the first or appends.
    let mut out = Vec::new();
    for (position, (index, value)) in reader.seen.iter().enumerate() {
        let info = &opts[*index];
        let same = |(i, _): &(usize, Option<String>)| opts[*i].id == info.id;
        let keep = if FIRST_WINS.contains(&info.id.as_str()) {
            !reader.seen[..position].iter().any(same)
        } else if APPENDING.contains(&info.id.as_str()) {
            true
        } else {
            !reader.seen[position + 1..].iter().any(same)
        };
        if !keep {
            continue;
        }
        let long = &info.longs[0];
        match (info.kind, value) {
            // `-x` without words clears the filter.
            (Kind::Greedy, Some(words)) if words.is_empty() => {}
            (_, Some(value)) => out.push(format!("--{long}={value}")),
            (_, None) => out.push(format!("--{long}")),
        }
    }
    if !reader.positional.is_empty() {
        out.push("--".into());
        out.extend(reader.positional);
    }
    Ok(Normalized::Args(out))
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;
    use crate::cli::Cli;

    fn run(args: &[&str]) -> Result<Vec<String>, String> {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        match normalize(&Cli::command(), &args) {
            Ok(Normalized::Args(a)) => Ok(a),
            Ok(Normalized::Help) => Ok(vec!["<help>".into()]),
            Err(e) => Err(e.to_string()),
        }
    }

    fn ok(args: &[&str]) -> Vec<String> {
        run(args).unwrap()
    }

    fn err(args: &[&str]) -> String {
        run(args).unwrap_err()
    }

    #[test]
    fn long_options_can_be_abbreviated_when_unique() {
        assert_eq!(ok(&["--hell"]), ["--hello"]);
        assert_eq!(ok(&["--get-c"]), ["--get-config"]);
        assert_eq!(
            ok(&["--get"]),
            ["--get"],
            "an exact name wins over longer ones"
        );
        assert_eq!(
            ok(&["--replay", "2020-01-01T00:00:00"]),
            ["--start-time=2020-01-01T00:00:00"]
        );
    }

    #[test]
    fn ambiguous_and_unknown_options_are_usage_errors() {
        assert_eq!(
            err(&["--he"]),
            "error: ambiguous option: --he (--hello, --help?)"
        );
        assert_eq!(err(&["--nope"]), "error: no such option: --nope");
        assert_eq!(err(&["-Z"]), "error: no such option: -Z");
        assert_eq!(err(&["-dZ"]), "error: no such option: -Z");
    }

    #[test]
    fn a_value_is_the_next_argument_whatever_it_looks_like() {
        assert_eq!(ok(&["-p", "-secret"]), ["--password=-secret"]);
        assert_eq!(ok(&["--user", "--hello"]), ["--user=--hello"]);
        assert_eq!(ok(&["-uadmin"]), ["--user=admin"]);
        assert_eq!(ok(&["--user=a=b"]), ["--user=a=b"]);
        assert_eq!(err(&["--user"]), "error: --user option requires 1 argument");
        assert_eq!(err(&["-u"]), "error: -u option requires 1 argument");
    }

    #[test]
    fn short_options_can_be_clustered() {
        assert_eq!(ok(&["-di"]), ["--debug", "--interactive"]);
        assert_eq!(ok(&["-dufoo"]), ["--debug", "--user=foo"]);
    }

    #[test]
    fn integers_follow_the_optparse_syntax() {
        assert_eq!(ok(&["--port", "0x10"]), ["--port=16"]);
        assert_eq!(ok(&["--port", "010"]), ["--port=8"]);
        assert_eq!(ok(&["--iter", "-3"]), ["--iter=-3"]);
        assert_eq!(
            err(&["--port", "x"]),
            "error: option --port: invalid integer value: 'x'"
        );
        assert_eq!(
            err(&["--iter=08"]),
            "error: option --iter: invalid integer value: '08'"
        );
    }

    #[test]
    fn an_option_without_a_value_rejects_one() {
        assert_eq!(
            err(&["--hello=1"]),
            "error: --hello option does not take a value"
        );
        assert_eq!(
            err(&["--get-config=x"]),
            "error: --get-config option does not take a value"
        );
        assert_eq!(
            err(&["--xpath=/a"]),
            "error: --xpath option does not take a value"
        );
    }

    #[test]
    fn the_database_of_get_config_is_optional() {
        assert_eq!(ok(&["--get-config"]), ["--get-config"]);
        assert_eq!(
            ok(&["--get-config", "candidate"]),
            ["--get-config=candidate"]
        );
        assert_eq!(
            ok(&["--get-config", "--hello"]),
            ["--get-config", "--hello"]
        );
        assert_eq!(ok(&["--get-config", "-"]), ["--get-config", "--", "-"]);
    }

    #[test]
    fn xpath_takes_words_up_to_the_next_option() {
        assert_eq!(ok(&["-x", "/a", "/b", "--get"]), ["--xpath=/a /b", "--get"]);
        assert_eq!(ok(&["-x", "-"]), ["--xpath=-"]);
        assert_eq!(ok(&["-x", "--get"]), ["--get"]);
        assert_eq!(ok(&["--xpath", "a", "-x", "b"]), ["--xpath=b"]);
    }

    #[test]
    fn repeated_options_follow_python() {
        assert_eq!(ok(&["--user", "a", "--user", "b"]), ["--user=b"]);
        assert_eq!(
            ok(&["--get-config", "a", "--get-config", "b", "--hello"]),
            ["--get-config=a", "--hello"]
        );
        assert_eq!(
            ok(&["--origin-filter", "x", "--origin-filter", "y"]),
            ["--origin-filter=x", "--origin-filter=y"]
        );
        assert_eq!(
            ok(&["--start-time", "1", "--replay-start-time", "2"]),
            ["--start-time=2"]
        );
    }

    #[test]
    fn positional_arguments_go_after_a_separator() {
        assert_eq!(
            ok(&["file.xml", "--hello", "-"]),
            ["--hello", "--", "file.xml", "-"]
        );
        assert_eq!(
            ok(&["--hello", "--", "--get", "-x"]),
            ["--hello", "--", "--get", "-x"]
        );
    }

    #[test]
    fn help_is_acted_on_where_it_is_reached() {
        assert_eq!(ok(&["--hello", "-h", "--bogus"]), ["<help>"]);
        assert_eq!(err(&["--bogus", "-h"]), "error: no such option: --bogus");
    }

    #[test]
    fn every_option_is_classified() {
        let table = options(&Cli::command());
        let kind = |id: &str| table.iter().find(|o| o.id == id).unwrap().kind;
        assert_eq!(kind("xpath"), Kind::Greedy);
        assert_eq!(kind("get_data"), Kind::Optional);
        assert_eq!(kind("port"), Kind::Int);
        assert_eq!(kind("user"), Kind::Value);
        assert_eq!(kind("hello"), Kind::Flag);
        assert_eq!(kind("start_time"), Kind::Value);
        let start = table.iter().find(|o| o.id == "start_time").unwrap();
        assert_eq!(start.longs, ["start-time", "replay-start-time"]);
    }
}
