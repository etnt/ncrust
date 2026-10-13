//! `<get-schema>` operation.
//!
//! May depend on: error, protocol::{ns, xmlw}.

use super::Operation;
use crate::protocol::ns::NETCONF_MONITORING_NS;
use crate::protocol::xmlw::XmlWriter;

/// `get-schema` for a schema identifier (typically a YANG module name).
pub struct GetSchema(pub String);

impl Operation for GetSchema {
    fn write_body(&self, w: &mut XmlWriter) {
        w.start("get-schema", &[("xmlns", NETCONF_MONITORING_NS)]);
        w.element("identifier", &[], &self.0);
        w.end();
    }
}
