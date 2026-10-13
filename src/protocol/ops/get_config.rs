//! `<get-config>` operation.
//!
//! May depend on: config, error, protocol::{filter, ns, xmlw}.

use super::{Operation, ReadParams, legacy_with_defaults_attr};
use crate::config::Db;
use crate::protocol::xmlw::XmlWriter;

pub struct GetConfig {
    pub source: Db,
    pub params: ReadParams,
}

impl Operation for GetConfig {
    fn rpc_attrs(&self) -> Vec<(&'static str, &'static str)> {
        legacy_with_defaults_attr(self.params.with_defaults)
    }

    fn write_body(&self, w: &mut XmlWriter) {
        w.start("get-config", &[]).start("source", &[]);
        w.empty(&self.source.0, &[]);
        w.end();
        self.params.write(w);
        w.end();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ops::rpc;

    #[test]
    fn names_the_source_datastore() {
        let op = GetConfig {
            source: Db("candidate".into()),
            params: ReadParams::default(),
        };
        let msg = rpc(&op, 1, "");
        assert!(
            msg.ends_with("<get-config><source><candidate/></source></get-config></rpc>"),
            "{msg}"
        );
    }
}
