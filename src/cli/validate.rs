//! Turns parsed arguments into an `Invocation`, applying every validation rule of the
//! Python `validate_options()` and `main()` dispatch.
//!
//! May depend on: config, error.

use super::Cli;
use crate::config::{
    Action, ConnectionSpec, Db, Filter, GetData, InputSource, Invocation, NetconfVersion, Origin,
    OutputStyle, Payload, ReadOptions, Subscription, TransportKind, UtcTimestamp, WithDefaults,
};
use crate::error::Error;

fn bad_value(option: &str, value: &str) -> Error {
    Error::Stdout(format!("Bad --{option} value: {value}"))
}

/// Options that Python validates for every command, whether the command uses them or not.
struct Checked {
    with_defaults: Option<WithDefaults>,
    origin_filter: Vec<Origin>,
    negated_origin_filter: Vec<Origin>,
}

pub fn validate(cli: &Cli) -> Result<Invocation, Error> {
    let checked = Checked {
        with_defaults: with_defaults(cli.with_defaults.as_deref())?,
        origin_filter: origins(&cli.origin_filter)?,
        negated_origin_filter: origins(&cli.negated_origin_filter)?,
    };
    let action = select_action(cli, checked)?;
    Ok(Invocation {
        connection: connection(cli)?,
        action,
        style: style(cli.output_style.as_deref())?,
        iterations: cli.iter,
        version: version(cli.version.as_deref())?,
        rpc_attribute: cli.rpc_attribute.clone(),
        debug: cli.debug,
    })
}

fn non_empty(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|s| !s.is_empty())
}

/// The first matching command wins, in the order of the Python script.
fn select_action(cli: &Cli, checked: Checked) -> Result<Action, Error> {
    if let Some(name) = non_empty(&cli.filename) {
        return Ok(Action::SessionFile(input_source(name)));
    }
    if cli.get {
        return Ok(Action::Get(read_options(cli, &checked)));
    }
    if let Some(db_argument) = &cli.get_config {
        return Ok(Action::GetConfig {
            db: db(database_name(cli, db_argument.as_deref()))?,
            options: read_options(cli, &checked),
        });
    }
    if let Some(db_argument) = &cli.get_data {
        return Ok(Action::GetData(get_data(
            cli,
            checked,
            database_name(cli, db_argument.as_deref()),
        )?));
    }
    if let Some(source) = non_empty(&cli.rpc) {
        return Ok(Action::Rpc(input_source(source)));
    }
    if let Some(source) = non_empty(&cli.edit_config) {
        return Ok(Action::EditConfig(payload(cli, source)?));
    }
    if let Some(source) = non_empty(&cli.edit_data) {
        return Ok(Action::EditData(payload(cli, source)?));
    }
    if let Some(source) = non_empty(&cli.copy_config) {
        return Ok(Action::CopyConfig(payload(cli, source)?));
    }
    if let Some(id) = non_empty(&cli.kill_session) {
        return Ok(Action::KillSession(id.to_string()));
    }
    if cli.discard_changes {
        return Ok(Action::DiscardChanges);
    }
    if cli.commit {
        return Ok(Action::Commit);
    }
    if cli.validate {
        return Ok(Action::Validate(db(&cli.db)?));
    }
    if cli.copy_running_to_startup {
        return Ok(Action::CopyRunningToStartup);
    }
    if let Some(identifier) = &cli.get_schema {
        return Ok(Action::GetSchema(identifier.clone()));
    }
    if let Some(stream) = &cli.create_subscription {
        return Ok(Action::CreateSubscription(subscription(cli, stream)?));
    }
    if let Some(stream) = &cli.establish_subscription {
        return Ok(Action::EstablishSubscription(subscription(cli, stream)?));
    }
    if let Some(id) = cli.kill_subscription {
        return Ok(Action::KillSubscription(id));
    }
    if cli.hello {
        return Ok(Action::Hello);
    }
    if cli.interactive {
        return Ok(Action::Interactive);
    }
    Err(Error::Usage(
        "error: a filename or a command option is required".into(),
    ))
}

/// The database of `--get-config [DB]` / `--get-data [DB]`; the literal "default" means: use --db.
fn database_name<'a>(cli: &'a Cli, argument: Option<&'a str>) -> &'a str {
    match argument {
        Some(name) if name != "default" => name,
        _ => &cli.db,
    }
}

/// `-x` wins over `--subtree-filter`; the words after `-x` are joined by spaces as in Python.
fn filter(cli: &Cli) -> Option<Filter> {
    let xpath = cli.xpath.join(" ");
    let xpath = xpath.trim();
    if !xpath.is_empty() {
        return Some(Filter::XPath(xpath.to_string()));
    }
    non_empty(&cli.subtree_filter).map(|s| Filter::Subtree(input_source(s)))
}

fn read_options(cli: &Cli, checked: &Checked) -> ReadOptions {
    ReadOptions {
        filter: filter(cli),
        with_defaults: checked.with_defaults,
        with_inactive: cli.with_inactive,
    }
}

fn get_data(cli: &Cli, checked: Checked, database: &str) -> Result<GetData, Error> {
    let xpath = cli.xpath.join(" ");
    let xpath = xpath.trim();
    let config_filter = match non_empty(&cli.config_filter) {
        None => None,
        Some("true") => Some(true),
        Some("false") => Some(false),
        Some(other) => return Err(bad_value("config-filter", other)),
    };
    Ok(GetData {
        db: db(database)?,
        xpath: (!xpath.is_empty()).then(|| xpath.to_string()),
        subtree: non_empty(&cli.subtree_filter).map(input_source),
        config_filter,
        max_depth: non_empty(&cli.max_depth).map(str::to_string),
        with_origin: cli.with_origin,
        origin_filter: checked.origin_filter,
        negated_origin_filter: checked.negated_origin_filter,
        with_defaults: checked.with_defaults,
        with_inactive: cli.with_inactive,
    })
}

fn payload(cli: &Cli, source: &str) -> Result<Payload, Error> {
    Ok(Payload {
        db: db(&cli.db)?,
        input: input_source(source),
        trace_id: cli.trace_id.clone(),
    })
}

fn subscription(cli: &Cli, stream: &str) -> Result<Subscription, Error> {
    Ok(Subscription {
        stream: stream.to_string(),
        filter: filter(cli),
        start_time: timestamp("start-time", &cli.start_time)?,
        stop_time: timestamp("stop-time", &cli.stop_time)?,
    })
}

fn timestamp(option: &str, value: &Option<String>) -> Result<Option<UtcTimestamp>, Error> {
    match non_empty(value) {
        None => Ok(None),
        Some(text) => UtcTimestamp::parse(text)
            .map(Some)
            .ok_or_else(|| bad_value(option, text)),
    }
}

fn origins(names: &[String]) -> Result<Vec<Origin>, Error> {
    names
        .iter()
        .map(|name| {
            Origin::from_name(name).ok_or_else(|| {
                let valid: Vec<String> = Origin::ALL
                    .iter()
                    .map(|o| format!("'{}'", o.name()))
                    .collect();
                Error::Stdout(format!(
                    "Origin must be one of {}.\nUse --rpc to use your own origin identity.",
                    valid.join(" | ")
                ))
            })
        })
        .collect()
}

fn with_defaults(value: Option<&str>) -> Result<Option<WithDefaults>, Error> {
    Ok(match value {
        None | Some("") => None,
        Some("explicit") => Some(WithDefaults::Explicit),
        Some("trim") => Some(WithDefaults::Trim),
        Some("report-all") => Some(WithDefaults::ReportAll),
        Some("report-all-tagged") => Some(WithDefaults::ReportAllTagged),
        Some("true") => Some(WithDefaults::LegacyTrue),
        Some("false") => Some(WithDefaults::LegacyFalse),
        Some(other) => return Err(bad_value("with-defaults", other)),
    })
}

fn connection(cli: &Cli) -> Result<ConnectionSpec, Error> {
    let kind = match cli.proto.as_str() {
        "ssh" => TransportKind::Ssh {
            password: cli.password.clone(),
            private_key: None,
        },
        "tcp" => TransportKind::Tcp {
            groups: cli.groups.clone().unwrap_or_default(),
            sup_gids: cli.sup_groups.clone().unwrap_or_default(),
        },
        other => return Err(bad_value("proto", other)),
    };
    Ok(ConnectionSpec {
        host: cli.host.clone(),
        port: cli.port,
        username: cli.user.clone(),
        kind,
        call_home: None,
    })
}

fn style(value: Option<&str>) -> Result<Option<OutputStyle>, Error> {
    Ok(match value {
        None | Some("default") => None,
        Some("raw") => Some(OutputStyle::Raw),
        Some("plain") => Some(OutputStyle::Plain),
        Some("pretty") => Some(OutputStyle::Pretty),
        Some("all") => Some(OutputStyle::All),
        Some("last") => Some(OutputStyle::Last),
        Some("noaaa") => Some(OutputStyle::NoAaa),
        Some(other) => return Err(bad_value("outputStyle", other)),
    })
}

fn version(value: Option<&str>) -> Result<Option<NetconfVersion>, Error> {
    Ok(match value {
        None => None,
        Some("1.0") => Some(NetconfVersion::V1_0),
        Some("1.1") => Some(NetconfVersion::V1_1),
        Some(other) => return Err(bad_value("version", other)),
    })
}

fn db(name: &str) -> Result<Db, Error> {
    Db::parse(name).ok_or_else(|| bad_value("db", name))
}

fn input_source(name: &str) -> InputSource {
    if name == "-" {
        InputSource::Stdin
    } else {
        InputSource::File(name.into())
    }
}
