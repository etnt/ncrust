//! Pure translation of an `Action` into a `Plan` (request source, reply count, style, close behaviour).
//!
//! May depend on: config.

use crate::config::{Action, InputSource, Invocation, OutputStyle};
use crate::error::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestSource {
    /// Nothing is sent after the hello exchange.
    None,
    /// One generated request per reply.
    Generated,
    /// The operation is streamed from this input inside an `<rpc>`.
    RpcFile(InputSource),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub request: RequestSource,
    /// Replies to read; negative means until the server closes the connection.
    pub replies: i64,
    pub style: OutputStyle,
    /// Print the server hello as the result (`--hello`).
    pub show_hello: bool,
}

pub fn plan(invocation: &Invocation) -> Result<Plan, Error> {
    let style = invocation.style.unwrap_or(OutputStyle::Pretty);
    if !matches!(style, OutputStyle::Raw | OutputStyle::Plain) {
        return Err(Error::Stderr(format!(
            "output style '{}' is not implemented yet; use -s raw or -s plain",
            style_name(style)
        )));
    }

    let (request, replies, show_hello) = match &invocation.action {
        // In raw style the hello reply has already been printed from the wire trace.
        Action::Hello => (RequestSource::None, 0, style != OutputStyle::Raw),
        Action::Get(_) | Action::GetConfig { .. } => {
            (RequestSource::Generated, invocation.iterations, false)
        }
        Action::Rpc(input) => (RequestSource::RpcFile(input.clone()), 1, false),
        _ => {
            return Err(Error::Stderr(
                "this operation is not implemented yet".into(),
            ));
        }
    };
    Ok(Plan {
        request,
        replies,
        style,
        show_hello,
    })
}

fn style_name(style: OutputStyle) -> &'static str {
    match style {
        OutputStyle::Raw => "raw",
        OutputStyle::Plain => "plain",
        OutputStyle::Pretty => "pretty",
        OutputStyle::All => "all",
        OutputStyle::Last => "last",
        OutputStyle::NoAaa => "noaaa",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ConnectionSpec, ReadOptions, TransportKind};

    fn invocation(action: Action, style: Option<OutputStyle>, iterations: i64) -> Invocation {
        Invocation {
            connection: ConnectionSpec {
                host: "localhost".into(),
                port: 2023,
                username: "admin".into(),
                kind: TransportKind::Tcp {
                    groups: String::new(),
                    sup_gids: String::new(),
                },
                call_home: None,
            },
            action,
            style,
            iterations,
            version: None,
            rpc_attribute: String::new(),
            debug: false,
        }
    }

    fn planned(action: Action, style: OutputStyle, iterations: i64) -> Plan {
        plan(&invocation(action, Some(style), iterations)).unwrap()
    }

    #[test]
    fn generated_requests_expect_one_reply_per_iteration() {
        let p = planned(Action::Get(ReadOptions::default()), OutputStyle::Plain, 3);
        assert_eq!(
            (p.request, p.replies, p.show_hello),
            (RequestSource::Generated, 3, false)
        );
    }

    #[test]
    fn rpc_file_expects_exactly_one_reply() {
        let p = planned(Action::Rpc(InputSource::Stdin), OutputStyle::Raw, 5);
        assert_eq!(
            (p.request, p.replies),
            (RequestSource::RpcFile(InputSource::Stdin), 1)
        );
    }

    #[test]
    fn hello_sends_nothing_and_is_shown_unless_raw() {
        let plain = planned(Action::Hello, OutputStyle::Plain, 1);
        assert_eq!(
            (plain.request, plain.replies, plain.show_hello),
            (RequestSource::None, 0, true)
        );
        assert!(!planned(Action::Hello, OutputStyle::Raw, 1).show_hello);
    }

    #[test]
    fn unimplemented_styles_and_actions_are_rejected_with_a_hint() {
        let default_style = plan(&invocation(Action::Hello, None, 1)).unwrap_err();
        assert!(
            default_style.to_string().contains("'pretty'"),
            "{default_style}"
        );
        assert!(plan(&invocation(Action::Commit, Some(OutputStyle::Raw), 1)).is_err());
    }
}
