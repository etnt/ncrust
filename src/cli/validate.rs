//! Turns parsed arguments into an `Invocation`, applying every validation rule of the
//! Python `validate_options()` and `main()` dispatch.
//!
//! May depend on: config, error.

use super::Cli;
use crate::config::{
    Action, AlgorithmSpecs, CallHome, ConnectionSpec, Db, Filter, GetData, HostKeyCheck,
    InputSource, Invocation, NetconfVersion, Origin, OutputStyle, Payload, PrivateKeySpec,
    ReadOptions, SshSettings, Subscription, TransportKind, UtcTimestamp, WithDefaults,
    normalize_sha256_fingerprint,
};
use crate::error::Error;

fn bad_value(option: &str, value: &str) -> Error {
    Error::Stdout(format!("Bad --{option} value: {value}"))
}

/// Options that Python validates for every command, whether the command uses them or not.
#[derive(Default)]
struct Checked {
    with_defaults: Option<WithDefaults>,
    origin_filter: Vec<Origin>,
    negated_origin_filter: Vec<Origin>,
}

/// Set (to anything) to turn `netconf-console` into `netconf-console-tcp`.
const FORCE_TCP_VARIABLE: &str = "TAILF_NETCONF_FORCE_TCP";

/// The port of the NETCONF-over-TCP IPC listener that `FORCE_TCP_VARIABLE` selects.
const FORCED_TCP_PORT: u16 = 2023;

pub fn validate(cli: &Cli) -> Result<Invocation, Error> {
    validate_with(cli, std::env::var_os(FORCE_TCP_VARIABLE).is_some())
}

/// The errors come in the order in which the Python script finds them: a missing command, call
/// home without ssh, then the option values it checks for every command.
fn validate_with(cli: &Cli, force_tcp: bool) -> Result<Invocation, Error> {
    let (checked, bad_option) = match check(cli) {
        Ok(checked) => (checked, None),
        Err(error) => (Checked::default(), Some(error)),
    };
    let action = select_action(cli, checked)?;
    if cli.listen && cli.proto != "ssh" {
        return Err(Error::Usage(
            "error: can only listen for call home over ssh".into(),
        ));
    }
    if let Some(error) = bad_option {
        return Err(error);
    }
    Ok(Invocation {
        connection: connection(cli, force_tcp)?,
        action,
        style: style(cli.output_style.as_deref())?,
        iterations: cli.iter,
        version: version(cli.version.as_deref())?,
        rpc_attribute: cli.rpc_attribute.clone(),
        debug: cli.debug,
    })
}

fn check(cli: &Cli) -> Result<Checked, Error> {
    Ok(Checked {
        with_defaults: with_defaults(cli.with_defaults.as_deref())?,
        origin_filter: origins(&cli.origin_filter)?,
        negated_origin_filter: origins(&cli.negated_origin_filter)?,
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

fn connection(cli: &Cli, force_tcp: bool) -> Result<ConnectionSpec, Error> {
    let proto = if force_tcp { "tcp" } else { cli.proto.as_str() };
    let port = if force_tcp { FORCED_TCP_PORT } else { cli.port };
    // Python fails with an exception here; call home needs ssh.
    if cli.listen && proto != "ssh" {
        return Err(Error::Usage(
            "error: can only listen for call home over ssh".into(),
        ));
    }
    let kind = match proto {
        "ssh" => TransportKind::Ssh {
            password: cli.password.clone(),
            private_key: non_empty(&cli.priv_key_file).map(|file| PrivateKeySpec {
                file: file.into(),
                key_type: non_empty(&cli.priv_key_type).map(str::to_string),
            }),
            settings: ssh_settings(cli)?,
        },
        "tcp" => TransportKind::Tcp {
            groups: cli.groups.clone().unwrap_or_default(),
            sup_gids: cli.sup_groups.clone().unwrap_or_default(),
        },
        other => return Err(bad_value("proto", other)),
    };
    Ok(ConnectionSpec {
        host: cli.host.clone(),
        port,
        username: cli.user.clone(),
        kind,
        call_home: cli.listen.then(|| CallHome {
            address: cli.listen_address.clone(),
            port: cli.listen_port,
        }),
    })
}

fn ssh_settings(cli: &Cli) -> Result<SshSettings, Error> {
    let host_key = match (
        non_empty(&cli.known_hosts),
        non_empty(&cli.host_key_fingerprint),
    ) {
        (Some(_), Some(_)) => {
            return Err(Error::Usage(
                "error: --known-hosts and --host-key-fingerprint cannot be combined".into(),
            ));
        }
        (Some(file), None) => HostKeyCheck::KnownHosts(file.into()),
        (None, Some(text)) => HostKeyCheck::Fingerprint(
            normalize_sha256_fingerprint(text)
                .ok_or_else(|| bad_value("host-key-fingerprint", text))?,
        ),
        (None, None) => HostKeyCheck::AcceptAny,
    };
    Ok(SshSettings {
        algorithms: AlgorithmSpecs {
            kex: cli.ssh_kex.clone(),
            host_key: cli.ssh_host_key_algs.clone(),
            ciphers: cli.ssh_ciphers.clone(),
            macs: cli.ssh_macs.clone(),
        },
        host_key,
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
        // trunk's tests use `-s` followed by another option, so the style is e.g. `--db=candidate`.
        Some(_) => Some(OutputStyle::Plain),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Raw, parse_cli};

    fn cli(args: &[&str]) -> Cli {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        match parse_cli(&args) {
            Ok(Raw::Cli(cli)) => *cli,
            other => panic!("{args:?}: {other:?}"),
        }
    }

    #[test]
    fn forcing_tcp_overrides_protocol_and_port() {
        let args = cli(&[
            "--hello", "--proto", "ssh", "--port", "1", "-u", "bob", "-g", "g",
        ]);
        let forced = validate_with(&args, true).unwrap().connection;
        assert_eq!((forced.port, forced.username.as_str()), (2023, "bob"));
        assert_eq!(
            forced.kind,
            TransportKind::Tcp {
                groups: "g".into(),
                sup_gids: String::new()
            }
        );
        let plain = validate_with(&args, false).unwrap().connection;
        assert_eq!(plain.port, 1);
        assert!(matches!(plain.kind, TransportKind::Ssh { .. }));
    }

    #[test]
    fn forcing_tcp_also_overrides_an_unknown_protocol() {
        let args = cli(&["--hello", "--proto", "udp"]);
        assert!(validate_with(&args, true).is_ok());
        assert!(validate_with(&args, false).is_err());
    }

    #[test]
    fn forcing_tcp_does_not_work_with_call_home() {
        let args = cli(&["--hello", "--listen"]);
        assert!(matches!(validate_with(&args, true), Err(Error::Usage(_))));
    }

    #[test]
    fn bad_values_are_reported_after_a_missing_command_and_call_home() {
        let args = cli(&["--with-defaults", "bad", "--proto", "tcp", "--listen"]);
        let missing = validate_with(&args, false).unwrap_err().to_string();
        assert!(missing.contains("required"), "{missing}");
        let args = cli(&[
            "--hello",
            "--with-defaults",
            "bad",
            "--proto",
            "tcp",
            "--listen",
        ]);
        let listen = validate_with(&args, false).unwrap_err().to_string();
        assert!(listen.contains("listen"), "{listen}");
    }
}
