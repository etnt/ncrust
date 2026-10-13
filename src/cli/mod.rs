//! Command-line layer: turns argv into a validated `config::Invocation`.
//!
//! May depend on: config, error.

pub mod legacy;
pub mod validate;

use clap::{ArgAction, Parser, error::ErrorKind};

use crate::config::Invocation;
use crate::error::Error;

const STYLE: &str = "Style Options";
const TCP: &str = "TCP Options";
const COMMANDS: &str = "NETCONF Command Options";

// Arguments as typed, before any interpretation (see `validate` for that).
// A plain comment on purpose: a doc comment would become the `--help` description.
#[derive(Parser, Debug)]
#[command(
    name = "netconf-console",
    override_usage = "netconf-console [-h | --help] [options] [cmdoptions | <filename> | -]",
    disable_version_flag = true,
    infer_long_args = true,
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

    /// Sends the same request ITER times. Useful only in test scripts
    #[arg(long, default_value_t = 1, allow_negative_numbers = true)]
    pub iter: i64,

    /// Display output in: raw, plain, pretty, all, last, or noaaa format
    #[arg(short = 's', long = "outputStyle", help_heading = STYLE)]
    pub output_style: Option<String>,

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

    /// Takes a filename (or '-' for standard input) as argument.
    #[arg(long = "subtree-filter", value_name = "FILE", help_heading = COMMANDS)]
    pub subtree_filter: Option<String>,

    /// One of 'explicit', 'trim', 'report-all' or 'report-all-tagged'.
    #[arg(long = "with-defaults", help_heading = COMMANDS)]
    pub with_defaults: Option<String>,

    /// Send with-inactive parameter.
    #[arg(long = "with-inactive", help_heading = COMMANDS)]
    pub with_inactive: bool,

    /// XPath filter to be used with --get and --get-config
    #[arg(short = 'x', long, num_args = 1.., value_name = "XPATH", help_heading = COMMANDS)]
    pub xpath: Vec<String>,

    /// Takes a filename (or '-' for standard input) as argument. The contents of the file is a
    /// single NETCONF rpc operation (w/o the surrounding <rpc>).
    #[arg(long, value_name = "FILE", help_heading = COMMANDS)]
    pub rpc: Option<String>,

    /// XML attributes to send in the rpc element
    #[arg(long = "rpc-attribute", default_value = "", hide_default_value = true, help_heading = COMMANDS)]
    pub rpc_attribute: String,
}

#[derive(Debug)]
pub enum Parsed {
    /// Help text to print; not an error.
    Help(String),
    Run(Box<Invocation>),
}

/// Parses `args` (without the program name).
pub fn parse(args: &[String]) -> Result<Parsed, Error> {
    let argv = std::iter::once("netconf-console".to_string()).chain(args.iter().cloned());
    match Cli::try_parse_from(argv) {
        Ok(cli) => Ok(Parsed::Run(Box::new(validate::validate(&cli)?))),
        Err(e) if e.kind() == ErrorKind::DisplayHelp => Ok(Parsed::Help(e.render().to_string())),
        Err(e) => Err(Error::Usage(e.render().to_string().trim_end().to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        Action, Db, Filter, InputSource, NetconfVersion, OutputStyle, TransportKind, WithDefaults,
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
                private_key: None
            }
        );
        assert_eq!(
            (inv.style, inv.iterations, inv.version, inv.debug),
            (None, 1, None, false)
        );
        assert_eq!(get_config_db(&["--get-config"]), Db::default());
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
