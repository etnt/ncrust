//! Command-line layer: turns argv into a validated `config::Invocation`.
//!
//! May depend on: config, error.

pub mod legacy;
pub mod validate;

use clap::{ArgAction, Parser, error::ErrorKind};

use crate::config::Invocation;
use crate::error::Error;

const STYLE: &str = "Style Options";
const SSH: &str = "SSH Options";
const TCP: &str = "TCP Options";
const COMMANDS: &str = "NETCONF Command Options";

// Arguments as typed, before any interpretation (see `validate` for that).
// A plain comment on purpose: a doc comment would become the `--help` description.
#[derive(Parser, Debug)]
#[command(
    name = "netconf-console",
    override_usage = "netconf-console [-h | --help] [options] [cmdoptions | <filename> | -]",
    about = "Where <filename> is a file containing a NETCONF XML command session.\nIf <filename> is not given, one of the Command Options must be given.\nFilename '-' means standard input.",
    help_template = "{usage-heading} {usage}\n\n{about}\n\n{all-args}",
    disable_version_flag = true,
    next_line_help = false
)]
pub struct Cli {
    /// force NETCONF version 1.0 or 1.1
    #[arg(short = 'v', long = "version", value_name = "VERSION")]
    pub version: Option<String>,

    #[arg(short = 'd', long, action = ArgAction::SetTrue)]
    pub debug: bool,

    /// username
    #[arg(short = 'u', long = "user", default_value = "admin")]
    pub user: String,

    /// password
    #[arg(short = 'p', long, default_value = "admin")]
    pub password: String,

    /// Which transport protocol to use, one of ssh or tcp
    #[arg(long, default_value = "ssh")]
    pub proto: String,

    /// NETCONF server hostname
    #[arg(long, default_value = "localhost")]
    pub host: String,

    /// NETCONF server port
    #[arg(long, default_value_t = 2022)]
    pub port: u16,

    /// Start NETCONF Call Home listener
    #[arg(long)]
    pub listen: bool,

    /// Listen to this address
    #[arg(long = "listen-address", default_value = "0.0.0.0")]
    pub listen_address: String,

    /// Listen to this port
    #[arg(long = "listen-port", default_value_t = 4334)]
    pub listen_port: u16,

    /// Sends the same request ITER times. Useful only in test scripts
    #[arg(long, default_value_t = 1, allow_negative_numbers = true)]
    pub iter: i64,

    #[arg(short = 'i', long)]
    pub interactive: bool,

    /// Display output in: raw, plain, pretty, all, last, or noaaa format
    #[arg(
        short = 's',
        long = "outputStyle",
        help_heading = STYLE,
        long_help = "Display output in: raw, plain, pretty, all, last, or noaaa format\n\
            raw:    print bytes received on the wire, framing is removed\n\
            plain:  skip the <hello> reply, print one reply as raw, skip the rest\n\
            pretty: skip the <hello> reply, pretty-print one reply, skip the rest\n\
            all:    skip the <hello> reply, pretty-print the rest of the replies\n\
            last:   skip the <hello> reply, pretty-print the last of the replies\n\
                    (used primarily with --iter)\n\
            noaaa:  as pretty, but remove Tail-f AAA and IETF NACM from the output"
    )]
    pub output_style: Option<String>,

    /// type of private key, rsa, dss or ed25519 (only a hint; the type is detected from the file)
    #[arg(long = "privKeyType", help_heading = SSH)]
    pub priv_key_type: Option<String>,

    /// file which contains the private key (without passphrase)
    #[arg(long = "privKeyFile", help_heading = SSH)]
    pub priv_key_file: Option<String>,

    /// OpenSSH known_hosts file; the server's host key must be listed there. By default any host
    /// key is accepted.
    #[arg(long = "known-hosts", value_name = "FILE", help_heading = SSH)]
    pub known_hosts: Option<String>,

    /// Expected host key fingerprint, like SHA256:... as printed by ssh-keygen -l
    #[arg(long = "host-key-fingerprint", value_name = "FINGERPRINT", help_heading = SSH)]
    pub host_key_fingerprint: Option<String>,

    /// Key exchange algorithms: a comma separated list, or a list starting with +, - or ^ to
    /// append to, remove from, or put first in the default list
    #[arg(long = "ssh-kex", value_name = "LIST", allow_hyphen_values = true, help_heading = SSH)]
    pub ssh_kex: Option<String>,

    /// Host key algorithms, in the same list syntax as --ssh-kex
    #[arg(long = "ssh-host-key-algs", value_name = "LIST", allow_hyphen_values = true, help_heading = SSH)]
    pub ssh_host_key_algs: Option<String>,

    /// Ciphers, in the same list syntax as --ssh-kex
    #[arg(long = "ssh-ciphers", value_name = "LIST", allow_hyphen_values = true, help_heading = SSH)]
    pub ssh_ciphers: Option<String>,

    /// MAC algorithms, in the same list syntax as --ssh-kex
    #[arg(long = "ssh-macs", value_name = "LIST", allow_hyphen_values = true, help_heading = SSH)]
    pub ssh_macs: Option<String>,

    /// Set group membership for user - comma separated string
    #[arg(short = 'g', long, help_heading = TCP)]
    pub groups: Option<String>,

    /// Set supplementary UNIX group ids for user - comma separated string of integers
    #[arg(short = 'G', long = "sup-groups", help_heading = TCP)]
    pub sup_groups: Option<String>,

    /// Connect to the server and print its capabilities
    #[arg(long, help_heading = COMMANDS)]
    pub hello: bool,

    /// Takes an optional -x argument for XPath filtering or --subtree-filter for subtree filtering.
    #[arg(long, help_heading = COMMANDS)]
    pub get: bool,

    /// Takes an optional --db argument, default is 'running', and an optional -x argument for
    /// XPath filtering or --subtree-filter for subtree filtering.
    #[arg(long = "get-config", num_args = 0..=1, value_name = "DB", help_heading = COMMANDS)]
    pub get_config: Option<Option<String>>,

    /// Database for commands that operate on a database.
    #[arg(long, default_value = "running", help_heading = COMMANDS)]
    pub db: String,

    /// Takes an optional --db argument, default is 'running', an optional --subtree-filter
    /// argument, an optional -x argument for XPath filtering, an optional --config-filter
    /// argument, an optional --max-depth argument, an optional --origin-filter argument or an
    /// optional --negated-origin-filter argument.
    #[arg(long = "get-data", num_args = 0..=1, value_name = "DB", help_heading = COMMANDS)]
    pub get_data: Option<Option<String>>,

    /// Can be used to limit the depth of the --get-data command.
    #[arg(long = "max-depth", help_heading = COMMANDS)]
    pub max_depth: Option<String>,

    /// Can be set to 'true' or 'false'. If not set, both operational and config data are returned
    /// from --get-data. If set to true, only config data is returned. If set to false, only
    /// operational data is returned.
    #[arg(long = "config-filter", help_heading = COMMANDS)]
    pub config_filter: Option<String>,

    /// If present, the 'origin' annotation for the nodes will be returned.
    #[arg(long = "with-origin", help_heading = COMMANDS)]
    pub with_origin: bool,

    /// Can be used with --get-data. If one or more origin filters are set, only nodes derived
    /// from the given origin are returned.
    #[arg(long = "origin-filter", help_heading = COMMANDS)]
    pub origin_filter: Vec<String>,

    /// Can be used with --get-data. If one or more origin filters are set, only nodes that are
    /// not derived from the given origin are returned.
    #[arg(long = "negated-origin-filter", help_heading = COMMANDS)]
    pub negated_origin_filter: Vec<String>,

    /// Takes a session-id as argument.
    #[arg(long = "kill-session", value_name = "SESSION_ID", help_heading = COMMANDS)]
    pub kill_session: Option<String>,

    #[arg(long = "discard-changes", help_heading = COMMANDS)]
    pub discard_changes: bool,

    #[arg(long, help_heading = COMMANDS)]
    pub commit: bool,

    /// Takes an optional --db argument, default is 'running'.
    #[arg(long, help_heading = COMMANDS)]
    pub validate: bool,

    #[arg(long = "copy-running-to-startup", help_heading = COMMANDS)]
    pub copy_running_to_startup: bool,

    /// Takes a filename (or '-' for standard input) as argument. The contents of the file is the
    /// data for a single NETCONF copy-config operation (put into the <config> XML element).
    /// Takes an optional --db argument, default is 'running'.
    #[arg(long = "copy-config", value_name = "FILE", help_heading = COMMANDS)]
    pub copy_config: Option<String>,

    /// Must be a trace-id in uuid_v4 format. Use with --copy-config, --edit-config or --edit-data.
    #[arg(long = "trace-id", help_heading = COMMANDS)]
    pub trace_id: Option<String>,

    /// Takes a filename (or '-' for standard input) as argument. The contents of the file is the
    /// data for a single NETCONF edit-config operation (put into the <config> XML element).
    /// Takes an optional --db argument, default is 'running'.
    #[arg(long = "edit-config", value_name = "FILE", help_heading = COMMANDS)]
    pub edit_config: Option<String>,

    /// Takes a filename (or '-' for standard input) as argument. The contents of the file is the
    /// data for a single NETCONF edit-data operation (put into the <config> XML element).
    /// Takes an optional --db argument, default is 'running'.
    #[arg(long = "edit-data", value_name = "FILE", help_heading = COMMANDS)]
    pub edit_data: Option<String>,

    /// Takes an identifier (typically YANG module name) as parameter
    #[arg(long = "get-schema", value_name = "IDENTIFIER", help_heading = COMMANDS)]
    pub get_schema: Option<String>,

    /// Takes a stream name as parameter, and an optional -x for XPath filtering or
    /// --subtree-filter for subtree filtering.
    #[arg(long = "create-subscription", value_name = "STREAM", help_heading = COMMANDS)]
    pub create_subscription: Option<String>,

    /// Takes a stream name as parameter, and an optional -x for XPath filtering or
    /// --subtree-filter for subtree filtering.
    #[arg(long = "establish-subscription", value_name = "STREAM", help_heading = COMMANDS)]
    pub establish_subscription: Option<String>,

    /// Takes a subscription id as parameter
    #[arg(long = "kill-subscription", value_name = "ID", help_heading = COMMANDS)]
    pub kill_subscription: Option<u32>,

    /// Takes a timestamp in YYYY-MM-DDTHH:MM:SS format, for example 2017-01-10T12:05:21. Assumes
    /// UTC. Used with --create-subscription or --establish-subscription.
    #[arg(long = "start-time", visible_alias = "replay-start-time", help_heading = COMMANDS)]
    pub start_time: Option<String>,

    /// Takes a timestamp in YYYY-MM-DDTHH:MM:SS format. Assumes UTC. Used with
    /// --create-subscription or --establish-subscription. The session will be closed after the
    /// stop time has been reached.
    #[arg(long = "stop-time", help_heading = COMMANDS)]
    pub stop_time: Option<String>,

    /// Takes a filename (or '-' for standard input) as argument. Can be used with --get*
    /// operations, --create-subscription and --establish-subscription.
    #[arg(long = "subtree-filter", value_name = "FILE", help_heading = COMMANDS)]
    pub subtree_filter: Option<String>,

    /// One of 'explicit', 'trim', 'report-all' or 'report-all-tagged'. Use with --get,
    /// --get-config or --get-data.
    #[arg(long = "with-defaults", help_heading = COMMANDS)]
    pub with_defaults: Option<String>,

    /// Send with-inactive parameter. Use with --get, --get-config or --get-data.
    #[arg(long = "with-inactive", help_heading = COMMANDS)]
    pub with_inactive: bool,

    /// XPath filter to be used with --get, --get-config, --get-data, --create-subscription and
    /// --establish-subscription
    #[arg(short = 'x', long, num_args = 1.., value_name = "XPATH", help_heading = COMMANDS)]
    pub xpath: Vec<String>,

    /// Takes a filename (or '-' for standard input) as argument. The contents of the file is a
    /// single NETCONF rpc operation (w/o the surrounding <rpc>).
    #[arg(long, value_name = "FILE", help_heading = COMMANDS)]
    pub rpc: Option<String>,

    /// XML attributes to send in the rpc element
    #[arg(long = "rpc-attribute", default_value = "", hide_default_value = true, help_heading = COMMANDS)]
    pub rpc_attribute: String,

    /// A file containing a NETCONF XML command session ('-' means standard input)
    #[arg(value_name = "FILE")]
    pub filename: Option<String>,
}

#[derive(Debug)]
pub enum Parsed {
    /// Help text to print; not an error.
    Help(String),
    Run(Box<Invocation>),
}

/// Parses `args` (without the program name).
pub fn parse(args: &[String]) -> Result<Parsed, Error> {
    match parse_cli(args)? {
        Raw::Help(text) => Ok(Parsed::Help(text)),
        Raw::Cli(cli) => Ok(Parsed::Run(Box::new(validate::validate(&cli)?))),
    }
}

/// The arguments as typed, before any interpretation.
#[derive(Debug)]
pub enum Raw {
    Help(String),
    Cli(Box<Cli>),
}

/// Reads `args` the way Python's `optparse` does, and builds the `Cli` from them.
pub fn parse_cli(args: &[String]) -> Result<Raw, Error> {
    use clap::{CommandFactory, FromArgMatches};
    let command = Cli::command();
    let plain = match legacy::normalize(&command, args)? {
        legacy::Normalized::Help => vec!["--help".to_string()],
        legacy::Normalized::Args(plain) => plain,
    };
    let argv = std::iter::once("netconf-console".to_string()).chain(plain);
    let matches = match command.try_get_matches_from(argv) {
        Ok(matches) => matches,
        Err(e) if e.kind() == ErrorKind::DisplayHelp => {
            return Ok(Raw::Help(e.render().to_string()));
        }
        Err(e) => return Err(Error::Usage(e.render().to_string().trim_end().to_string())),
    };
    Cli::from_arg_matches(&matches)
        .map(|cli| Raw::Cli(Box::new(cli)))
        .map_err(|e| Error::Usage(e.render().to_string().trim_end().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        Action, AlgorithmSpecs, CallHome, Db, Filter, HostKeyCheck, InputSource, NetconfVersion,
        OutputStyle, PrivateKeySpec, SshSettings, TransportKind, WithDefaults,
    };

    fn invocation(args: &[&str]) -> Invocation {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        match parse(&args) {
            Ok(Parsed::Run(inv)) => *inv,
            other => panic!("{args:?}: {other:?}"),
        }
    }

    fn error(args: &[&str]) -> Error {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        parse(&args).expect_err("expected an error")
    }

    fn get_config_db(args: &[&str]) -> Db {
        match invocation(args).action {
            Action::GetConfig { db, .. } => db,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn defaults_match_the_python_script() {
        let inv = invocation(&["--get-config"]);
        assert_eq!(inv.connection.host, "localhost");
        assert_eq!(
            (inv.connection.port, inv.connection.username.as_str()),
            (2022, "admin")
        );
        assert_eq!(
            inv.connection.kind,
            TransportKind::Ssh {
                password: "admin".into(),
                private_key: None,
                settings: SshSettings::default(),
            }
        );
        assert_eq!(
            (inv.style, inv.iterations, inv.version, inv.debug),
            (None, 1, None, false)
        );
        assert_eq!(get_config_db(&["--get-config"]), Db::default());
    }

    #[test]
    fn ssh_options() {
        let inv = invocation(&[
            "--hello",
            "--privKeyFile",
            "id",
            "--privKeyType",
            "rsa",
            "--ssh-kex",
            "^mlkem768x25519-sha256",
            "--ssh-host-key-algs",
            "ssh-ed25519",
            "--ssh-ciphers",
            "-aes128-ctr",
            "--ssh-macs",
            "+hmac-sha1",
            "--known-hosts",
            "kh",
        ]);
        let TransportKind::Ssh {
            private_key,
            settings,
            ..
        } = inv.connection.kind
        else {
            panic!("not ssh");
        };
        assert_eq!(
            private_key,
            Some(PrivateKeySpec {
                file: "id".into(),
                key_type: Some("rsa".into())
            })
        );
        assert_eq!(
            settings,
            SshSettings {
                algorithms: AlgorithmSpecs {
                    kex: Some("^mlkem768x25519-sha256".into()),
                    host_key: Some("ssh-ed25519".into()),
                    ciphers: Some("-aes128-ctr".into()),
                    macs: Some("+hmac-sha1".into()),
                },
                host_key: HostKeyCheck::KnownHosts("kh".into()),
            }
        );
    }

    #[test]
    fn host_key_fingerprints_are_checked_when_parsing() {
        let fingerprint = "SHA256:nThbg6kXUpJWGl7E1IGOCspRomTxdCARLviKw6E5SY8";
        let inv = invocation(&["--hello", "--host-key-fingerprint", fingerprint]);
        let TransportKind::Ssh { settings, .. } = inv.connection.kind else {
            panic!("not ssh");
        };
        assert_eq!(
            settings.host_key,
            HostKeyCheck::Fingerprint(fingerprint.into())
        );

        assert_eq!(
            error(&["--hello", "--host-key-fingerprint", "MD5:aa"]).to_string(),
            "Bad --host-key-fingerprint value: MD5:aa"
        );
        assert!(matches!(
            error(&[
                "--hello",
                "--known-hosts",
                "kh",
                "--host-key-fingerprint",
                fingerprint
            ]),
            Error::Usage(_)
        ));
    }

    #[test]
    fn call_home_options() {
        let inv = invocation(&["--hello", "--listen"]);
        assert_eq!(
            inv.connection.call_home,
            Some(CallHome {
                address: "0.0.0.0".into(),
                port: 4334
            })
        );
        let inv = invocation(&[
            "--hello",
            "--listen",
            "--listen-address",
            "::1",
            "--listen-port",
            "99",
        ]);
        assert_eq!(
            inv.connection.call_home,
            Some(CallHome {
                address: "::1".into(),
                port: 99
            })
        );
        assert_eq!(invocation(&["--hello"]).connection.call_home, None);
    }

    #[test]
    fn call_home_needs_ssh() {
        let err = error(&["--hello", "--listen", "--proto", "tcp"]);
        assert!(
            matches!(&err, Error::Usage(m) if m.ends_with("can only listen for call home over ssh"))
        );
    }

    #[test]
    fn tcp_transport_options() {
        let inv = invocation(&[
            "--proto", "tcp", "--port", "2023", "-g", "a,b", "-G", "1,2", "-u", "bob", "--hello",
        ]);
        assert_eq!(
            inv.connection.kind,
            TransportKind::Tcp {
                groups: "a,b".into(),
                sup_gids: "1,2".into()
            }
        );
        assert_eq!(
            (inv.connection.port, inv.connection.username.as_str()),
            (2023, "bob")
        );
    }

    #[test]
    fn get_config_database_comes_from_its_value_or_from_db() {
        assert_eq!(
            get_config_db(&["--get-config", "candidate"]),
            Db("candidate".into())
        );
        assert_eq!(
            get_config_db(&["--get-config", "--db", "startup"]),
            Db("startup".into())
        );
        assert_eq!(
            get_config_db(&["--get-config", "candidate", "--db", "startup"]),
            Db("candidate".into())
        );
        assert_eq!(
            get_config_db(&["--db", "startup", "--get-config"]),
            Db("startup".into())
        );
    }

    #[test]
    fn filters_xpath_wins_over_subtree_and_xpath_words_are_joined() {
        let Action::Get(opts) = invocation(&["--get", "-x", "/a[b", "=", "'c']"]).action else {
            panic!()
        };
        assert_eq!(opts.filter, Some(Filter::XPath("/a[b = 'c']".into())));

        let Action::Get(opts) =
            invocation(&["--subtree-filter", "f.xml", "-x", "/a", "--get"]).action
        else {
            panic!()
        };
        assert_eq!(opts.filter, Some(Filter::XPath("/a".into())));

        let Action::Get(opts) = invocation(&["--get", "--subtree-filter", "-"]).action else {
            panic!()
        };
        assert_eq!(opts.filter, Some(Filter::Subtree(InputSource::Stdin)));

        let Action::Get(opts) = invocation(&["--get", "-x", ""]).action else {
            panic!()
        };
        assert_eq!(opts.filter, None);
    }

    #[test]
    fn with_defaults_and_inactive() {
        let Action::Get(opts) = invocation(&[
            "--get",
            "--with-defaults",
            "report-all-tagged",
            "--with-inactive",
        ])
        .action
        else {
            panic!()
        };
        assert_eq!(opts.with_defaults, Some(WithDefaults::ReportAllTagged));
        assert!(opts.with_inactive);
        for (value, expected) in [
            ("true", WithDefaults::LegacyTrue),
            ("false", WithDefaults::LegacyFalse),
        ] {
            let Action::Get(opts) = invocation(&["--get", "--with-defaults", value]).action else {
                panic!()
            };
            assert_eq!(opts.with_defaults, Some(expected));
        }
        assert_eq!(
            error(&["--get", "--with-defaults", "foo"]).to_string(),
            "Bad --with-defaults value: foo"
        );
    }

    #[test]
    fn rpc_input_and_attribute() {
        let inv = invocation(&["--rpc", "-", "--rpc-attribute", "a=\"b\""]);
        assert_eq!(inv.action, Action::Rpc(InputSource::Stdin));
        assert_eq!(inv.rpc_attribute, "a=\"b\"");
        assert_eq!(
            invocation(&["--rpc", "x.xml"]).action,
            Action::Rpc(InputSource::File("x.xml".into()))
        );
    }

    #[test]
    fn first_matching_command_wins_in_pythons_order() {
        assert!(matches!(
            invocation(&["--hello", "--get"]).action,
            Action::Get(_)
        ));
        assert!(matches!(
            invocation(&["--rpc", "f", "--hello"]).action,
            Action::Rpc(_)
        ));
        assert!(matches!(
            invocation(&["--rpc", "f", "--get-config"]).action,
            Action::GetConfig { .. }
        ));
    }

    #[test]
    fn style_version_and_iterations() {
        assert_eq!(
            invocation(&["--hello", "-s", "raw"]).style,
            Some(OutputStyle::Raw)
        );
        assert_eq!(
            invocation(&["--hello", "--outputStyle", "noaaa"]).style,
            Some(OutputStyle::NoAaa)
        );
        assert_eq!(invocation(&["--hello", "-s", "default"]).style, None);
        assert_eq!(
            invocation(&["--hello", "-v", "1.1"]).version,
            Some(NetconfVersion::V1_1)
        );
        assert_eq!(invocation(&["--hello", "--iter", "-1"]).iterations, -1);
        assert!(invocation(&["--hello", "-d"]).debug);
        assert_eq!(
            error(&["--hello", "-s", "bogus"]).to_string(),
            "Bad --outputStyle value: bogus"
        );
        assert_eq!(
            error(&["--hello", "-v", "2.0"]).to_string(),
            "Bad --version value: 2.0"
        );
        assert_eq!(
            error(&["--hello", "--proto", "udp"]).to_string(),
            "Bad --proto value: udp"
        );
        assert_eq!(
            error(&["--get-config", "--db", "a b"]).to_string(),
            "Bad --db value: a b"
        );
    }

    #[test]
    fn long_options_can_be_abbreviated() {
        assert!(matches!(
            invocation(&["--get-c"]).action,
            Action::GetConfig { .. }
        ));
        assert_eq!(
            invocation(&["--hello", "--outputS", "plain"]).style,
            Some(OutputStyle::Plain)
        );
    }

    #[test]
    fn a_command_is_required_and_usage_errors_exit_with_2() {
        let err = error(&[]);
        assert!(
            matches!(&err, Error::Usage(m) if m.contains("a filename or a command option is required"))
        );
        assert_eq!(err.exit_code(), 2);
        assert!(matches!(error(&["--no-such-option"]), Error::Usage(_)));
    }

    #[test]
    fn get_data_options() {
        let Action::GetData(data) = invocation(&[
            "--get-data",
            "operational",
            "-x",
            "/a",
            "--config-filter",
            "false",
            "--max-depth",
            "3",
            "--with-origin",
            "--origin-filter",
            "intended",
            "--origin-filter",
            "system",
            "--negated-origin-filter",
            "learned",
            "--with-defaults",
            "trim",
            "--with-inactive",
        ])
        .action
        else {
            panic!()
        };
        assert_eq!(data.db, Db("operational".into()));
        assert_eq!(data.xpath.as_deref(), Some("/a"));
        assert_eq!(data.subtree, None);
        assert_eq!(
            (data.config_filter, data.max_depth.as_deref()),
            (Some(false), Some("3"))
        );
        assert_eq!(
            data.origin_filter,
            [
                crate::config::Origin::Intended,
                crate::config::Origin::System
            ]
        );
        assert_eq!(data.negated_origin_filter, [crate::config::Origin::Learned]);
        assert_eq!(data.with_defaults, Some(WithDefaults::Trim));
        assert!(data.with_origin && data.with_inactive);
    }

    #[test]
    fn get_data_accepts_both_filters_and_takes_its_database_like_get_config() {
        let Action::GetData(data) =
            invocation(&["--get-data", "--subtree-filter", "-", "-x", "/a"]).action
        else {
            panic!()
        };
        assert_eq!(data.xpath.as_deref(), Some("/a"));
        assert_eq!(data.subtree, Some(InputSource::Stdin));
        assert_eq!(data.db, Db::default());
        let Action::GetData(data) = invocation(&["--get-data", "--db", "startup"]).action else {
            panic!()
        };
        assert_eq!(data.db, Db("startup".into()));
    }

    #[test]
    fn bad_get_data_values_are_reported_like_python_does() {
        assert_eq!(
            error(&["--get-data", "--config-filter", "maybe"]).to_string(),
            "Bad --config-filter value: maybe"
        );
        let origin = error(&["--get", "--origin-filter", "mine"]);
        assert_eq!(
            origin.to_string(),
            "Origin must be one of 'default' | 'dynamic' | 'intended' | 'learned' | 'system' | 'unknown'.\nUse --rpc to use your own origin identity."
        );
        assert!(matches!(origin, Error::Stdout(_)));
    }

    #[test]
    fn payload_commands_take_their_database_and_trace_id() {
        for flag in ["--edit-config", "--edit-data", "--copy-config"] {
            let inv = invocation(&[flag, "f.xml", "--db", "candidate", "--trace-id", "t"]);
            let payload = match inv.action {
                Action::EditConfig(p) if flag == "--edit-config" => p,
                Action::EditData(p) if flag == "--edit-data" => p,
                Action::CopyConfig(p) if flag == "--copy-config" => p,
                other => panic!("{flag}: {other:?}"),
            };
            assert_eq!(payload.db, Db("candidate".into()));
            assert_eq!(payload.input, InputSource::File("f.xml".into()));
            assert_eq!(payload.trace_id.as_deref(), Some("t"));
        }
    }

    #[test]
    fn simple_commands() {
        assert_eq!(
            invocation(&["--kill-session", "7"]).action,
            Action::KillSession("7".into())
        );
        assert_eq!(
            invocation(&["--discard-changes"]).action,
            Action::DiscardChanges
        );
        assert_eq!(invocation(&["--commit"]).action, Action::Commit);
        assert_eq!(
            invocation(&["--copy-running-to-startup"]).action,
            Action::CopyRunningToStartup
        );
        assert_eq!(
            invocation(&["--validate", "--db", "candidate"]).action,
            Action::Validate(Db("candidate".into()))
        );
        assert_eq!(
            invocation(&["--get-schema", "ietf-interfaces"]).action,
            Action::GetSchema("ietf-interfaces".into())
        );
        assert_eq!(
            invocation(&["--kill-subscription", "9"]).action,
            Action::KillSubscription(9)
        );
        assert_eq!(invocation(&["-i"]).action, Action::Interactive);
    }

    #[test]
    fn subscriptions_take_filters_and_times() {
        let Action::CreateSubscription(s) = invocation(&[
            "--create-subscription",
            "NETCONF",
            "-x",
            "/a",
            "--start-time",
            "2017-01-10T12:05:21",
            "--stop-time",
            "2017-01-11T00:00:00",
        ])
        .action
        else {
            panic!()
        };
        assert_eq!(s.stream, "NETCONF");
        assert_eq!(s.filter, Some(Filter::XPath("/a".into())));
        assert_eq!(s.start_time.unwrap().as_str(), "2017-01-10T12:05:21");
        assert_eq!(s.stop_time.unwrap().epoch_secs(), 1_484_092_800);

        // Both spellings set the same value, as in Python.
        let Action::EstablishSubscription(s) = invocation(&[
            "--establish-subscription",
            "S",
            "--replay-start-time",
            "2017-01-10T12:05:21",
        ])
        .action
        else {
            panic!()
        };
        assert_eq!(s.start_time.unwrap().as_str(), "2017-01-10T12:05:21");
        assert_eq!(
            error(&["--create-subscription", "S", "--stop-time", "tomorrow"]).to_string(),
            "Bad --stop-time value: tomorrow"
        );
    }

    #[test]
    fn a_filename_selects_a_session_script_and_wins_over_options() {
        assert_eq!(
            invocation(&["session.xml"]).action,
            Action::SessionFile(InputSource::File("session.xml".into()))
        );
        assert_eq!(
            invocation(&["-"]).action,
            Action::SessionFile(InputSource::Stdin)
        );
        assert!(matches!(
            invocation(&["--get", "s.xml"]).action,
            Action::SessionFile(_)
        ));
    }

    #[test]
    fn help_is_not_an_error() {
        let args = vec!["--help".to_string()];
        let Ok(Parsed::Help(text)) = parse(&args) else {
            panic!()
        };
        assert!(text.starts_with("Usage: netconf-console"), "{text}");
        for heading in ["Style Options", "TCP Options", "NETCONF Command Options"] {
            assert!(text.contains(heading), "{heading}");
        }
    }
}
