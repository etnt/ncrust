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
    /// `YYYY-MM-DDTHH:MM:SS`, UTC.
    pub start_time: Option<String>,
    /// `YYYY-MM-DDTHH:MM:SS`, UTC.
    pub stop_time: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_db_is_running() {
        assert_eq!(Db::default(), Db("running".to_string()));
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
