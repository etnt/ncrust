//! One file per NETCONF operation; each writes its request body through `protocol::xmlw`.
//!
//! May depend on: config, error, protocol::{filter, ns, xmlw}.

pub mod copy_config;
pub mod edit_config;
pub mod edit_data;
pub mod get;
pub mod get_config;
pub mod get_data;
pub mod get_schema;
pub mod raw_rpc;
pub mod simple;
pub mod subscription;

use crate::config::WithDefaults;
use crate::protocol::filter::ResolvedFilter;
use crate::protocol::ns::{NC_NS, NCS_TRACE_ID_NS, WITH_DEFAULTS_NS, WITH_INACTIVE_NS};
use crate::protocol::xmlw::XmlWriter;

/// Parameters shared by `get` and `get-config`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadParams {
    pub filter: Option<ResolvedFilter>,
    pub with_defaults: Option<WithDefaults>,
    pub with_inactive: bool,
}

impl ReadParams {
    /// Filter, `with-defaults` and `with-inactive`, in the order the Python script emits them.
    fn write(&self, w: &mut XmlWriter) {
        if let Some(filter) = &self.filter {
            filter.write(w);
        }
        if let Some(mode) = with_defaults_mode(self.with_defaults) {
            w.element("with-defaults", &[("xmlns", WITH_DEFAULTS_NS)], mode);
        }
        write_with_inactive(w, self.with_inactive);
    }
}

/// The value of the standard `with-defaults` parameter, if `with_defaults` is one of the four modes.
fn with_defaults_mode(with_defaults: Option<WithDefaults>) -> Option<&'static str> {
    match with_defaults? {
        WithDefaults::Explicit => Some("explicit"),
        WithDefaults::Trim => Some("trim"),
        WithDefaults::ReportAll => Some("report-all"),
        WithDefaults::ReportAllTagged => Some("report-all-tagged"),
        WithDefaults::LegacyTrue | WithDefaults::LegacyFalse => None,
    }
}

/// The deprecated tail-f `with-defaults="true|false"` goes on `<rpc>`.
fn legacy_with_defaults_attr(
    with_defaults: Option<WithDefaults>,
) -> Vec<(&'static str, &'static str)> {
    match with_defaults {
        Some(WithDefaults::LegacyTrue) => vec![("with-defaults", "true")],
        Some(WithDefaults::LegacyFalse) => vec![("with-defaults", "false")],
        _ => Vec::new(),
    }
}

fn write_with_inactive(w: &mut XmlWriter, with_inactive: bool) {
    if with_inactive {
        w.empty("with-inactive", &[("xmlns", WITH_INACTIVE_NS)]);
    }
}

fn write_trace_id(w: &mut XmlWriter, trace_id: Option<&str>) {
    if let Some(id) = trace_id {
        w.element("trace-id", &[("xmlns", NCS_TRACE_ID_NS)], id);
    }
}

/// An operation whose payload is streamed from a file between `prefix` and `suffix`;
/// both are parts of the content of `<rpc>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Streamed {
    pub prefix: String,
    pub suffix: String,
}

impl Streamed {
    /// The payload is the whole operation (`--rpc`).
    pub fn bare() -> Self {
        Streamed {
            prefix: String::new(),
            suffix: String::new(),
        }
    }

    fn from_open(w: XmlWriter) -> Self {
        let (prefix, suffix) = w.finish_open();
        Streamed { prefix, suffix }
    }
}

/// A NETCONF operation: the content of an `<rpc>` element.
pub trait Operation {
    /// Extra attributes for the `<rpc>` element itself (the deprecated tail-f `with-defaults`).
    fn rpc_attrs(&self) -> Vec<(&'static str, &'static str)> {
        Vec::new()
    }

    fn write_body(&self, w: &mut XmlWriter);
}

/// The complete `<rpc>` message for `op`. `rpc_attribute` is the raw text of `--rpc-attribute`.
pub fn rpc(op: &dyn Operation, message_id: u32, rpc_attribute: &str) -> String {
    let id = message_id.to_string();
    let mut attrs = vec![("xmlns", NC_NS)];
    attrs.extend(op.rpc_attrs());
    attrs.push(("message-id", &id));

    let mut w = XmlWriter::new();
    w.declaration()
        .start_with_raw_attrs("rpc", &attrs, rpc_attribute);
    op.write_body(&mut w);
    w.end();
    w.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Probe;
    impl Operation for Probe {
        fn rpc_attrs(&self) -> Vec<(&'static str, &'static str)> {
            vec![("with-defaults", "true")]
        }
        fn write_body(&self, w: &mut XmlWriter) {
            w.empty("probe", &[]);
        }
    }

    #[test]
    fn envelope_has_namespace_id_extra_and_raw_attributes() {
        assert_eq!(
            rpc(&Probe, 7, "foo=\"bar\""),
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rpc xmlns=\"{NC_NS}\" with-defaults=\"true\" message-id=\"7\" foo=\"bar\"><probe/></rpc>"
            )
        );
    }
}
