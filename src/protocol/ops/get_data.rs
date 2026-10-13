//! NMDA `<get-data>` operation.
//!
//! May depend on: config, error, protocol::{ns, xmlw}.

use super::{Operation, legacy_with_defaults_attr, with_defaults_mode, write_with_inactive};
use crate::config::{Db, Origin, WithDefaults};
use crate::protocol::ns::{NMDA_DS_NS, NMDA_NS, NMDA_ORIGIN_NS};
use crate::protocol::xmlw::XmlWriter;

/// `get-data` with filters already read from their files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetData {
    pub db: Db,
    pub xpath: Option<String>,
    /// Well-formed XML, inserted as is.
    pub subtree: Option<String>,
    pub config_filter: Option<bool>,
    pub max_depth: Option<String>,
    pub with_origin: bool,
    pub origin_filter: Vec<Origin>,
    pub negated_origin_filter: Vec<Origin>,
    pub with_defaults: Option<WithDefaults>,
    pub with_inactive: bool,
}

impl Operation for GetData {
    fn rpc_attrs(&self) -> Vec<(&'static str, &'static str)> {
        legacy_with_defaults_attr(self.with_defaults)
    }

    /// Children come in the order the Python script emits them.
    fn write_body(&self, w: &mut XmlWriter) {
        let uses_origin = self.with_origin
            || !self.origin_filter.is_empty()
            || !self.negated_origin_filter.is_empty();
        let mut attrs = vec![("xmlns", NMDA_NS), ("xmlns:ds", NMDA_DS_NS)];
        if uses_origin {
            attrs.push(("xmlns:or", NMDA_ORIGIN_NS));
        }
        w.start("get-data", &attrs);
        w.element("datastore", &[], &format!("ds:{}", self.db.0));
        if let Some(subtree) = &self.subtree {
            w.start("subtree-filter", &[]).raw(subtree).end();
        }
        if let Some(xpath) = &self.xpath {
            w.element("xpath-filter", &[], xpath);
        }
        if let Some(config) = self.config_filter {
            w.element("config-filter", &[], if config { "true" } else { "false" });
        }
        if let Some(depth) = &self.max_depth {
            w.element("max-depth", &[], depth);
        }
        if let Some(mode) = with_defaults_mode(self.with_defaults) {
            w.element("with-defaults", &[], mode);
        }
        write_with_inactive(w, self.with_inactive);
        for origin in &self.origin_filter {
            w.element("origin-filter", &[], &format!("or:{}", origin.name()));
        }
        for origin in &self.negated_origin_filter {
            w.element(
                "negated-origin-filter",
                &[],
                &format!("or:{}", origin.name()),
            );
        }
        if self.with_origin {
            w.empty("with-origin", &[]);
        }
        w.end();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ops::rpc;

    fn minimal() -> GetData {
        GetData {
            db: Db("operational".into()),
            xpath: None,
            subtree: None,
            config_filter: None,
            max_depth: None,
            with_origin: false,
            origin_filter: vec![],
            negated_origin_filter: vec![],
            with_defaults: None,
            with_inactive: false,
        }
    }

    #[test]
    fn minimal_request_names_the_datastore_with_the_ds_prefix() {
        let msg = rpc(&minimal(), 1, "");
        assert!(
            msg.contains("<get-data xmlns=\"urn:ietf:params:xml:ns:yang:ietf-netconf-nmda\" xmlns:ds=\"urn:ietf:params:xml:ns:yang:ietf-datastores\"><datastore>ds:operational</datastore></get-data>"),
            "{msg}"
        );
    }

    #[test]
    fn origin_namespace_is_declared_only_when_used() {
        assert!(!rpc(&minimal(), 1, "").contains("xmlns:or"));
        let op = GetData {
            origin_filter: vec![Origin::Intended, Origin::System],
            ..minimal()
        };
        let msg = rpc(&op, 1, "");
        assert!(msg.contains("xmlns:or="), "{msg}");
        assert!(
            msg.contains(
                "<origin-filter>or:intended</origin-filter><origin-filter>or:system</origin-filter>"
            ),
            "{msg}"
        );
    }

    #[test]
    fn xpath_is_escaped() {
        let op = GetData {
            xpath: Some("/a[b < 3]".into()),
            ..minimal()
        };
        assert!(rpc(&op, 1, "").contains("<xpath-filter>/a[b &lt; 3]</xpath-filter>"));
    }
}
