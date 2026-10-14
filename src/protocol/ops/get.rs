//! `<get>` operation.
//!
//! May depend on: config, error, protocol::{filter, ns, xmlw}.

use super::{Operation, ReadParams, legacy_with_defaults_attr};
use crate::protocol::xmlw::XmlWriter;

pub struct Get(pub ReadParams);

impl Operation for Get {
    fn rpc_attrs(&self) -> Vec<(&'static str, &'static str)> {
        legacy_with_defaults_attr(self.0.with_defaults)
    }

    fn write_body(&self, w: &mut XmlWriter) {
        w.start("get", &[]);
        self.0.write(w);
        w.end();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ops::rpc;

    #[test]
    fn plain_get() {
        let msg = rpc(&Get(ReadParams::default()), 1, "");
        assert!(msg.ends_with("message-id=\"1\"><get></get></rpc>"), "{msg}");
    }

    #[test]
    #[cfg(feature = "tailf-compat")]
    fn filter_defaults_and_inactive_come_in_that_order() {
        use crate::config::WithDefaults;
        use crate::protocol::filter::ResolvedFilter;
        use crate::protocol::ns::{NC_NS, WITH_DEFAULTS_NS, WITH_INACTIVE_NS};
        let op = Get(ReadParams {
            filter: Some(ResolvedFilter::XPath("/a".into())),
            with_defaults: Some(WithDefaults::Trim),
            with_inactive: true,
        });
        let msg = rpc(&op, 3, "");
        let expected = format!(
            "<rpc xmlns=\"{NC_NS}\" message-id=\"3\"><get><filter type=\"xpath\" select=\"/a\"/>\
             <with-defaults xmlns=\"{WITH_DEFAULTS_NS}\">trim</with-defaults>\
             <with-inactive xmlns=\"{WITH_INACTIVE_NS}\"/></get></rpc>"
        );
        assert!(msg.ends_with(&expected), "{msg}");
    }

    #[test]
    #[cfg(feature = "tailf-compat")]
    fn legacy_with_defaults_is_an_rpc_attribute() {
        use crate::config::WithDefaults;
        let op = Get(ReadParams {
            with_defaults: Some(WithDefaults::LegacyTrue),
            ..ReadParams::default()
        });
        let msg = rpc(&op, 1, "");
        assert!(
            msg.contains("with-defaults=\"true\" message-id=\"1\"><get></get>"),
            "{msg}"
        );
    }
}
