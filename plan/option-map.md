# Option map

Every `add_option` of `bin/netconf-console.py` (49) and where it lands in `ncrust::config`.
Defaults are the Python defaults. "Used by" names the `Action` (or the connection) that reads the value.

## General and connection

| # | Option | Short | Python dest | Default | Type | Used by | Rust field |
|---|---|---|---|---|---|---|---|
| 1 | `--version` | `-v` | `version` | none | `1.0`\|`1.1` | all (hello) | `Invocation::version` (`Option<NetconfVersion>`) |
| 2 | `--debug` | `-d` | `debug` | off | flag | all | `Invocation::debug` |
| 3 | `--user` | `-u` | `username` | `admin` | string | ssh, tcp | `ConnectionSpec::username` |
| 4 | `--password` | `-p` | `password` | `admin` | string | ssh | `TransportKind::Ssh::password` |
| 5 | `--proto` | | `proto` | `ssh` | `ssh`\|`tcp` | connection | variant of `TransportKind` |
| 6 | `--host` | | `host` | `localhost` | string | connection | `ConnectionSpec::host` |
| 7 | `--port` | | `port` | `2022` | int | connection | `ConnectionSpec::port` |
| 8 | `--listen` | | `listen` | off | flag | ssh | `ConnectionSpec::call_home` is `Some` |
| 9 | `--listen-address` | | `listen_address` | `0.0.0.0` | string | `--listen` | `CallHome::address` |
| 10 | `--listen-port` | | `listen_port` | `4334` | int | `--listen` | `CallHome::port` |
| 11 | `--iter` | | `iter` | `1` | int | request-built actions | `Invocation::iterations` |
| 12 | `--interactive` | `-i` | `interactive` | off | flag | interactive | `Action::Interactive` |
| 13 | `--outputStyle` | `-s` | `style` | `default` | string | all | `Invocation::style` (`None` = default) |

## SSH and TCP

| # | Option | Short | Python dest | Default | Type | Used by | Rust field |
|---|---|---|---|---|---|---|---|
| 14 | `--privKeyType` | | `privKeyType` | empty | string | ssh | `PrivateKeySpec::key_type` (hint) |
| 15 | `--privKeyFile` | | `privKeyFile` | empty | path | ssh | `PrivateKeySpec::file` |
| 16 | `--groups` | `-g` | `groups` | empty | comma string | tcp | `TransportKind::Tcp::groups` |
| 17 | `--sup-groups` | `-G` | `supgroups` | empty | comma string of ints | tcp | `TransportKind::Tcp::sup_gids` |

Options that only ncrust has (P3; the Python script has no counterpart):

| Option | Default | Used by | Rust field |
|---|---|---|---|
| `--known-hosts FILE` | none (any host key is accepted) | ssh | `HostKeyCheck::KnownHosts` |
| `--host-key-fingerprint SHA256:...` | none | ssh; not with `--known-hosts` | `HostKeyCheck::Fingerprint` |
| `--ssh-kex LIST` | library default (classical) | ssh | `AlgorithmSpecs::kex` |
| `--ssh-host-key-algs LIST` | library default | ssh | `AlgorithmSpecs::host_key` |
| `--ssh-ciphers LIST` | library default | ssh | `AlgorithmSpecs::ciphers` |
| `--ssh-macs LIST` | library default | ssh | `AlgorithmSpecs::macs` |

A `LIST` is a comma separated list that replaces the default, or starts with `+` (append), `-` (remove) or `^` (put first), with `*` and `?` patterns, as in OpenSSH.

## Commands

| # | Option | Short | Python dest | Default | Type | Used by | Rust field |
|---|---|---|---|---|---|---|---|
| 18 | `--hello` | | `hello` | off | flag | hello | `Action::Hello` |
| 19 | `--get` | | `get` | off | flag | get | `Action::Get` |
| 20 | `--get-config` | | `getConfig` | unset | optional db | get-config | `Action::GetConfig` (`db`: value or `--db`) |
| 21 | `--get-data` | | `getData` | unset | optional db | get-data | `Action::GetData` (`db`: value or `--db`) |
| 22 | `--max-depth` | | `max_depth` | empty | string | get-data | `GetData::max_depth` |
| 23 | `--config-filter` | | `config_filter` | none | `true`\|`false` | get-data | `GetData::config_filter` |
| 24 | `--subtree-filter` | | `subtree_filter` | empty | file or `-` | get, get-config, get-data, subscriptions | `Filter::Subtree` / `GetData::subtree` |
| 25 | `--with-origin` | | `with_origin` | off | flag | get-data | `GetData::with_origin` |
| 26 | `--origin-filter` | | `origin_filter` | `[]` | repeatable `Origin` | get-data | `GetData::origin_filter` |
| 27 | `--negated-origin-filter` | | `negated_origin_filter` | `[]` | repeatable `Origin` | get-data | `GetData::negated_origin_filter` |
| 28 | `--db` | | `db` | `running` | string | get-config, get-data, validate, edit-*, copy-config | `Db` |
| 29 | `--with-defaults` | | `wdefaults` | empty | `WithDefaults` | get, get-config, get-data | `ReadOptions::with_defaults`, `GetData::with_defaults` |
| 30 | `--with-inactive` | | `winactive` | off | flag | get, get-config, get-data | `ReadOptions::with_inactive`, `GetData::with_inactive` |
| 31 | `--xpath` | `-x` | `xpath` | empty | greedy string | get, get-config, get-data, subscriptions | `Filter::XPath` / `GetData::xpath` |
| 32 | `--kill-session` | | `kill_session` | empty | session id | kill-session | `Action::KillSession` |
| 33 | `--discard-changes` | | `discard_changes` | off | flag | discard-changes | `Action::DiscardChanges` |
| 34 | `--commit` | | `commit` | off | flag | commit | `Action::Commit` |
| 35 | `--validate` | | `validate` | off | flag | validate | `Action::Validate` |
| 36 | `--copy-running-to-startup` | | `copy_running_to_startup` | off | flag | copy-config | `Action::CopyRunningToStartup` |
| 37 | `--copy-config` | | `copy` | empty | file or `-` | copy-config | `Action::CopyConfig` (`Payload::input`) |
| 38 | `--trace-id` | | `trace_id` | none | uuid v4 string | edit-config, edit-data, copy-config | `Payload::trace_id` |
| 39 | `--edit-config` | | `edit` | empty | file or `-` | edit-config | `Action::EditConfig` |
| 40 | `--edit-data` | | `edit_data` | empty | file or `-` | edit-data | `Action::EditData` |
| 41 | `--get-schema` | | `get_schema` | none | identifier | get-schema | `Action::GetSchema` |
| 42 | `--create-subscription` | | `create_subscription` | none | stream name | create-subscription | `Action::CreateSubscription` |
| 43 | `--establish-subscription` | | `establish_subscription` | none | stream name | establish-subscription | `Action::EstablishSubscription` |
| 44 | `--kill-subscription` | | `kill_subscription` | none | int | kill-subscription | `Action::KillSubscription` |
| 45 | `--replay-start-time` | | `start_time` | empty | timestamp | subscriptions | `Subscription::start_time` |
| 46 | `--rpc` | | `rpc` | empty | file or `-` | rpc | `Action::Rpc` |
| 47 | `--start-time` | | `start_time` | empty | timestamp | subscriptions | `Subscription::start_time` (same as #45) |
| 48 | `--stop-time` | | `stop_time` | empty | timestamp | subscriptions | `Subscription::stop_time` |
| 49 | `--rpc-attribute` | | `rpc_attribute` | empty | raw attributes | all request-built actions, `--rpc` | `Invocation::rpc_attribute` |

Positional `<filename>` (or `-`): `Action::SessionFile`.

## Action selection

Python picks the first match in this order; `cli::validate` must keep it:
`<filename>`, `--get`, `--get-config`, `--get-data`, `--rpc`, `--edit-config`, `--edit-data`,
`--copy-config`, `--kill-session`, `--discard-changes`, `--commit`, `--validate`,
`--copy-running-to-startup`, `--get-schema`, `--create-subscription`, `--establish-subscription`,
`--kill-subscription`, `--hello`, `--interactive`. None of them: error `a filename or a command option is required`.

## Python behaviour to settle in later phases

- `--version` accepts any string; an unknown value advertises no capability. Rust rejects it (record in `DEVIATIONS.md`, P4).
- `--iter` may be negative or 0 in Python (negative loops until EOF); kept as `i64` in `Invocation::iterations`.
- Help text of `--get-data` mentions `--only-config-false`/`--only-config-true`; these options do not exist. Only `--config-filter` does.
- `--with-defaults` is validated against `trim|explicit|report-all|report-all-tagged|true|false`. Help also names `--copy-config`, `--edit-*` but the Python code only uses it for the get operations.
- `--with-inactive` help also names `--copy-config`, `--edit-config`, `--edit-data`; the Python code only uses it for the get operations.
- `--start-time` and `--replay-start-time` write the same variable (last one wins).
- Unknown `--outputStyle` values: Python prints the reply unformatted. Decide in P4.
- `--max-depth` and timestamps are not validated by Python.
