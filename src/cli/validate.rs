//! Turns parsed arguments into an `Invocation`, applying every validation rule of the
//! Python `validate_options()` and `main()` dispatch.
//!
//! May depend on: config, error.

use super::Cli;
use crate::config::{
    Action, ConnectionSpec, Db, Filter, InputSource, Invocation, NetconfVersion, OutputStyle,
    ReadOptions, TransportKind, WithDefaults,
};
use crate::error::Error;

fn bad_value(option: &str, value: &str) -> Error {
    Error::Stdout(format!("Bad --{option} value: {value}"))
}

pub fn validate(cli: &Cli) -> Result<Invocation, Error> {
    let action = select_action(cli)?;
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

/// The first matching command wins, in the order of the Python script.
fn select_action(cli: &Cli) -> Result<Action, Error> {
    if cli.get {
        return Ok(Action::Get(read_options(cli)?));
    }
    if let Some(db_argument) = &cli.get_config {
        // The literal "default" means: use --db.
        let name = match db_argument.as_deref() {
            Some(name) if name != "default" => name,
            _ => &cli.db,
        };
        return Ok(Action::GetConfig {
            db: db(name)?,
            options: read_options(cli)?,
        });
    }
    if let Some(source) = cli.rpc.as_deref().filter(|s| !s.is_empty()) {
        return Ok(Action::Rpc(input_source(source)));
    }
    if cli.hello {
        return Ok(Action::Hello);
    }
    Err(Error::Usage(
        "error: a filename or a command option is required".into(),
    ))
}

fn read_options(cli: &Cli) -> Result<ReadOptions, Error> {
    // `-x` swallowed all words up to the next option; as in Python they are joined by spaces.
    let xpath = cli.xpath.join(" ");
    let xpath = xpath.trim();
    let subtree = cli.subtree_filter.as_deref().filter(|s| !s.is_empty());
    let filter = if !xpath.is_empty() {
        Some(Filter::XPath(xpath.to_string()))
    } else {
        subtree.map(|s| Filter::Subtree(input_source(s)))
    };
    Ok(ReadOptions {
        filter,
        with_defaults: with_defaults(cli.with_defaults.as_deref())?,
        with_inactive: cli.with_inactive,
    })
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
