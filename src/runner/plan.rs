//! Pure translation of an `Invocation` into a `Plan` (what to send, how many replies to read,
//! which style to print in, how to close).
//!
//! May depend on: config.

use crate::config::{Action, InputSource, Invocation, NetconfVersion, OutputStyle};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// Nothing is sent after the hello exchange.
    Nothing,
    /// The generated request; subscriptions send it once, the others once per reply.
    Generated,
    /// The operation is streamed from an input inside an `<rpc>`.
    Streamed,
    /// The input is a whole NETCONF session and is sent as is.
    SessionFile(InputSource),
    /// Requests are read from standard input.
    Interactive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub source: Source,
    /// Replies to read; negative means until the server closes the connection.
    pub replies: i64,
    pub style: OutputStyle,
    /// Base versions to offer in our hello.
    pub versions: Vec<NetconfVersion>,
    /// Session scripts bring their own hello.
    pub send_client_hello: bool,
    /// Print the server hello as the result (`--hello`).
    pub show_hello: bool,
    /// `--get-schema` without an explicit style: print just the schema text.
    pub schema_text: bool,
    /// Subscriptions end when this time (epoch seconds) is reached after the replay is complete.
    pub stop_time: Option<i64>,
}

pub fn plan(invocation: &Invocation) -> Plan {
    let explicit_style = invocation.style;
    let follows_stream = matches!(
        invocation.action,
        Action::CreateSubscription(_) | Action::EstablishSubscription(_) | Action::Interactive
    );
    let style = explicit_style.unwrap_or(if follows_stream {
        OutputStyle::All
    } else {
        OutputStyle::Pretty
    });

    let mut plan = Plan {
        source: Source::Generated,
        replies: invocation.iterations,
        style,
        versions: match invocation.version {
            None => vec![NetconfVersion::V1_0, NetconfVersion::V1_1],
            Some(version) => vec![version],
        },
        send_client_hello: true,
        show_hello: false,
        schema_text: false,
        stop_time: None,
    };

    match &invocation.action {
        // In raw style the hello reply has already been printed from the wire trace.
        Action::Hello => {
            plan.source = Source::Nothing;
            plan.replies = 0;
            plan.show_hello = style != OutputStyle::Raw;
        }
        Action::Rpc(_) | Action::EditConfig(_) | Action::EditData(_) | Action::CopyConfig(_) => {
            plan.source = Source::Streamed;
            plan.replies = 1;
        }
        Action::SessionFile(source) => {
            plan.source = Source::SessionFile(source.clone());
            plan.replies = -1;
            plan.send_client_hello = false;
            // Without -v a script is assumed to speak 1.0.
            if invocation.version.is_none() {
                plan.versions = vec![NetconfVersion::V1_0];
            }
        }
        Action::Interactive => {
            plan.source = Source::Interactive;
            plan.replies = -1;
        }
        Action::CreateSubscription(s) | Action::EstablishSubscription(s) => {
            plan.replies = -1;
            plan.stop_time = s.stop_time.as_ref().map(|t| t.epoch_secs());
        }
        Action::GetSchema(_) => {
            if explicit_style.is_none() {
                plan.schema_text = true;
                plan.replies = 0;
            }
        }
        Action::Get(_)
        | Action::GetConfig { .. }
        | Action::GetData(_)
        | Action::KillSession(_)
        | Action::DiscardChanges
        | Action::Commit
        | Action::Validate(_)
        | Action::CopyRunningToStartup
        | Action::KillSubscription(_) => {}
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        ConnectionSpec, Db, GetData, Payload, ReadOptions, Subscription, TransportKind,
        UtcTimestamp,
    };

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

    fn planned(action: Action) -> Plan {
        plan(&invocation(action, None, 3))
    }

    fn subscription(stop: Option<&str>) -> Subscription {
        Subscription {
            stream: "NETCONF".into(),
            filter: None,
            start_time: None,
            stop_time: stop.map(|s| UtcTimestamp::parse(s).unwrap()),
        }
    }

    fn payload() -> Payload {
        Payload {
            db: Db::default(),
            input: InputSource::Stdin,
            trace_id: None,
        }
    }

    #[test]
    fn generated_requests_expect_one_reply_per_iteration_and_default_to_pretty() {
        for action in [
            Action::Get(ReadOptions::default()),
            Action::Commit,
            Action::DiscardChanges,
            Action::KillSession("1".into()),
            Action::KillSubscription(1),
            Action::CopyRunningToStartup,
            Action::Validate(Db::default()),
        ] {
            let p = planned(action.clone());
            assert_eq!(
                (p.source, p.replies, p.style, p.send_client_hello),
                (Source::Generated, 3, OutputStyle::Pretty, true),
                "{action:?}"
            );
        }
        assert_eq!(
            planned(Action::Get(ReadOptions::default())).versions.len(),
            2
        );
    }

    #[test]
    fn streamed_operations_expect_exactly_one_reply() {
        for action in [
            Action::Rpc(InputSource::Stdin),
            Action::EditConfig(payload()),
            Action::EditData(payload()),
            Action::CopyConfig(payload()),
        ] {
            let p = planned(action);
            assert_eq!(
                (p.source, p.replies, p.style),
                (Source::Streamed, 1, OutputStyle::Pretty)
            );
        }
    }

    #[test]
    fn hello_sends_nothing_and_is_shown_unless_raw() {
        let p = planned(Action::Hello);
        assert_eq!(
            (p.source, p.replies, p.show_hello),
            (Source::Nothing, 0, true)
        );
        let raw = plan(&invocation(Action::Hello, Some(OutputStyle::Raw), 1));
        assert!(!raw.show_hello);
    }

    #[test]
    fn subscriptions_and_interactive_follow_the_stream_with_style_all() {
        for action in [
            Action::CreateSubscription(subscription(None)),
            Action::EstablishSubscription(subscription(None)),
            Action::Interactive,
        ] {
            let p = planned(action);
            assert_eq!((p.replies, p.style), (-1, OutputStyle::All));
        }
        let explicit = plan(&invocation(Action::Interactive, Some(OutputStyle::Raw), 1));
        assert_eq!(explicit.style, OutputStyle::Raw);
        assert_eq!(planned(Action::Interactive).source, Source::Interactive);
    }

    #[test]
    fn subscription_stop_time_is_kept_as_epoch_seconds() {
        let p = planned(Action::CreateSubscription(subscription(Some(
            "2017-01-10T12:05:21",
        ))));
        assert_eq!(p.stop_time, Some(1_484_049_921));
        assert_eq!(
            planned(Action::CreateSubscription(subscription(None))).stop_time,
            None
        );
    }

    #[test]
    fn session_files_bring_their_own_hello_and_default_to_1_0() {
        let p = planned(Action::SessionFile(InputSource::Stdin));
        assert_eq!(
            (
                p.source.clone(),
                p.replies,
                p.send_client_hello,
                p.versions.clone()
            ),
            (
                Source::SessionFile(InputSource::Stdin),
                -1,
                false,
                vec![NetconfVersion::V1_0]
            )
        );
        let mut inv = invocation(Action::SessionFile(InputSource::Stdin), None, 1);
        inv.version = Some(NetconfVersion::V1_1);
        assert_eq!(plan(&inv).versions, vec![NetconfVersion::V1_1]);
    }

    #[test]
    fn get_schema_prints_only_the_schema_unless_a_style_is_given() {
        let p = planned(Action::GetSchema("m".into()));
        assert!(p.schema_text && p.replies == 0);
        let styled = plan(&invocation(
            Action::GetSchema("m".into()),
            Some(OutputStyle::Pretty),
            2,
        ));
        assert!(!styled.schema_text && styled.replies == 2);
    }

    #[test]
    fn get_data_is_an_ordinary_generated_request() {
        let data = GetData {
            db: Db::default(),
            xpath: None,
            subtree: None,
            config_filter: None,
            max_depth: None,
            with_origin: false,
            origin_filter: vec![],
            negated_origin_filter: vec![],
            with_defaults: None,
            with_inactive: false,
        };
        assert_eq!(planned(Action::GetData(data)).source, Source::Generated);
    }
}
