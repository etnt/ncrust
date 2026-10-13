//! Validated, I/O-free description of what the user asked for.
//!
//! May depend on: nothing.

use std::path::PathBuf;

/// Everything the runner needs; built once by `cli::validate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub connection: ConnectionSpec,
    pub action: Action,
    /// `None` means no style was given; the runner picks the default per action.
    pub style: Option<OutputStyle>,
    /// How many times a request-built action is sent. Negative means "until EOF", as in Python's `--iter`.
    pub iterations: i64,
    /// `None` advertises both 1.0 and 1.1.
    pub version: Option<NetconfVersion>,
    /// Raw XML attributes added to `<rpc>` (`--rpc-attribute`).
    pub rpc_attribute: String,
    pub debug: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionSpec {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub kind: TransportKind,
    /// Set for `--listen`: wait for an incoming SSH connection instead of connecting.
    pub call_home: Option<CallHome>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportKind {
    Ssh {
        password: String,
        private_key: Option<PrivateKeySpec>,
    },
    /// NSO/ConfD IPC over TCP; `groups` and `sup_gids` go into the connection preamble.
    Tcp { groups: String, sup_gids: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateKeySpec {
    pub file: PathBuf,
    /// Only a hint (`--privKeyType`); the key type is detected from the file.
    pub key_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallHome {
    pub address: String,
    pub port: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetconfVersion {
    V1_0,
    V1_1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStyle {
    Raw,
    Plain,
    Pretty,
    All,
    Last,
    NoAaa,
}

/// Datastore name; any string is accepted, as the server decides what exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Db(pub String);

impl Default for Db {
    fn default() -> Self {
        Db("running".to_string())
    }
}

impl Db {
    /// The name is sent as an element name, so only plain XML name characters are accepted.
    pub fn parse(name: &str) -> Option<Db> {
        let mut chars = name.chars();
        let first_ok = chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
        let rest_ok = chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
        (first_ok && rest_ok).then(|| Db(name.to_string()))
    }
}

/// Where a payload (filter, edit data, rpc, session script) is read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputSource {
    Stdin,
    File(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Filter {
    XPath(String),
    Subtree(InputSource),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WithDefaults {
    Explicit,
    Trim,
    ReportAll,
    ReportAllTagged,
    /// Deprecated tail-f `with-defaults="true"` attribute on `<rpc>`.
    LegacyTrue,
    /// Deprecated tail-f `with-defaults="false"` attribute on `<rpc>`.
    LegacyFalse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Default,
    Dynamic,
    Intended,
    Learned,
    System,
    Unknown,
}

impl Origin {
    pub const ALL: [Origin; 6] = [
        Origin::Default,
        Origin::Dynamic,
        Origin::Intended,
        Origin::Learned,
        Origin::System,
        Origin::Unknown,
    ];

    /// The identity name in the `ietf-origin` module.
    pub fn name(self) -> &'static str {
        match self {
            Origin::Default => "default",
            Origin::Dynamic => "dynamic",
            Origin::Intended => "intended",
            Origin::Learned => "learned",
            Origin::System => "system",
            Origin::Unknown => "unknown",
        }
    }

    pub fn from_name(name: &str) -> Option<Origin> {
        Origin::ALL.into_iter().find(|o| o.name() == name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Hello,
    Get(ReadOptions),
    GetConfig {
        db: Db,
        options: ReadOptions,
    },
    GetData(GetData),
    EditConfig(Payload),
    EditData(Payload),
    CopyConfig(Payload),
    /// `--rpc FILE`: the file holds one operation, wrapped in `<rpc>` by the tool.
    Rpc(InputSource),
    /// `<filename>`: the file holds a whole NETCONF session and is sent as is.
    SessionFile(InputSource),
    KillSession(String),
    DiscardChanges,
    Commit,
    Validate(Db),
    CopyRunningToStartup,
    GetSchema(String),
    CreateSubscription(Subscription),
    EstablishSubscription(Subscription),
    KillSubscription(u32),
    Interactive,
}

/// Options shared by `get` and `get-config`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadOptions {
    pub filter: Option<Filter>,
    pub with_defaults: Option<WithDefaults>,
    pub with_inactive: bool,
}

/// NMDA `get-data`; unlike `get`/`get-config` it accepts an xpath and a subtree filter together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetData {
    pub db: Db,
    pub xpath: Option<String>,
    pub subtree: Option<InputSource>,
    /// `Some(true)` config only, `Some(false)` operational only, `None` both.
    pub config_filter: Option<bool>,
    /// Sent verbatim: `unbounded` or a number.
    pub max_depth: Option<String>,
    pub with_origin: bool,
    pub origin_filter: Vec<Origin>,
    pub negated_origin_filter: Vec<Origin>,
    pub with_defaults: Option<WithDefaults>,
    pub with_inactive: bool,
}

/// `edit-config`, `edit-data` and `copy-config`: the input is the content of `<config>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payload {
    pub db: Db,
    pub input: InputSource,
    pub trace_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subscription {
    pub stream: String,
    pub filter: Option<Filter>,
    pub start_time: Option<UtcTimestamp>,
    pub stop_time: Option<UtcTimestamp>,
}

/// A `YYYY-MM-DDTHH:MM:SS` timestamp in UTC. The text is kept as typed because it is sent to the
/// server unchanged; the epoch seconds are used for the client-side stop time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UtcTimestamp {
    text: String,
    epoch_secs: i64,
}

impl UtcTimestamp {
    /// Accepts the same shape as Python's `strptime("%Y-%m-%dT%H:%M:%S")`: a four digit year and
    /// one or two digits for the other fields.
    pub fn parse(text: &str) -> Option<Self> {
        let (date, time) = text.split_once('T')?;
        let [year, month, day] = date.split('-').collect::<Vec<_>>()[..] else {
            return None;
        };
        let [hour, minute, second] = time.split(':').collect::<Vec<_>>()[..] else {
            return None;
        };
        let number = |s: &str, max_digits: usize| -> Option<i64> {
            let plain =
                !s.is_empty() && s.len() <= max_digits && s.bytes().all(|b| b.is_ascii_digit());
            plain.then(|| s.parse().ok()).flatten()
        };
        if year.len() != 4 {
            return None;
        }
        let (year, month, day) = (number(year, 4)?, number(month, 2)?, number(day, 2)?);
        let (hour, minute, second) = (number(hour, 2)?, number(minute, 2)?, number(second, 2)?);
        let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        let days_in_month = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => return None,
        };
        // 60 and 61 are leap seconds, which strptime accepts too.
        if !(1..=days_in_month).contains(&day) || hour > 23 || minute > 59 || second > 61 {
            return None;
        }
        Some(UtcTimestamp {
            text: text.to_string(),
            epoch_secs: days_from_civil(year, month, day) * 86_400
                + hour * 3600
                + minute * 60
                + second,
        })
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn epoch_secs(&self) -> i64 {
        self.epoch_secs
    }
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_match_pythons_timegm() {
        for (text, epoch) in [
            ("2017-01-10T12:05:21", 1_484_049_921),
            ("2000-03-01T00:00:00", 951_868_800),
            ("1970-01-01T00:00:00", 0),
            ("2024-02-29T23:59:59", 1_709_251_199),
            ("1999-12-31T23:59:60", 946_684_800),
            ("2017-1-9T2:5:1", 1_483_927_501),
        ] {
            let ts = UtcTimestamp::parse(text).unwrap_or_else(|| panic!("{text}"));
            assert_eq!((ts.epoch_secs(), ts.as_str()), (epoch, text));
        }
    }

    #[test]
    fn malformed_timestamps_are_rejected() {
        for bad in [
            "",
            "2017-01-10",
            "2017-01-10 12:05:21",
            "2017-13-10T12:05:21",
            "2017-02-29T00:00:00",
            "2017-01-10T24:00:00",
            "2017-01-10T12:60:00",
            "17-01-10T12:05:21",
            "2017-01-10T12:05",
            "2017-01-10T12:05:21Z",
            "2017-01-10T-1:05:21",
            "2017-01-10T12:05:21+00:00",
        ] {
            assert_eq!(UtcTimestamp::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn default_db_is_running() {
        assert_eq!(Db::default(), Db("running".to_string()));
    }

    #[test]
    fn db_names_must_be_usable_as_element_names() {
        for ok in ["running", "candidate", "startup", "my-ds.1", "_x"] {
            assert_eq!(Db::parse(ok), Some(Db(ok.to_string())), "{ok}");
        }
        for bad in ["", "1a", "a b", "a/>", "a:b", "-a", "ä"] {
            assert_eq!(Db::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn get_config_defaults_describe_the_plain_python_invocation() {
        let inv = Invocation {
            connection: ConnectionSpec {
                host: "localhost".into(),
                port: 2022,
                username: "admin".into(),
                kind: TransportKind::Ssh {
                    password: "admin".into(),
                    private_key: None,
                },
                call_home: None,
            },
            action: Action::GetConfig {
                db: Db::default(),
                options: ReadOptions::default(),
            },
            style: None,
            iterations: 1,
            version: None,
            rpc_attribute: String::new(),
            debug: false,
        };
        assert_eq!(inv.clone(), inv);
    }
}
