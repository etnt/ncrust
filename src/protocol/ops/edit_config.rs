//! `<edit-config>` envelope around a streamed payload.
//!
//! May depend on: config, error, protocol::{ns, xmlw}.

use super::{Streamed, write_trace_id};
use crate::config::Db;
use crate::protocol::ns::NC_NS;
use crate::protocol::xmlw::XmlWriter;

/// `<edit-config>` up to and including the start of `<config>`; the payload goes inside it.
pub fn streamed(target: &Db, trace_id: Option<&str>) -> Streamed {
    let mut w = XmlWriter::new();
    w.start("edit-config", &[("xmlns:nc", NC_NS)]);
    w.start("target", &[]).empty(&target.0, &[]).end();
    write_trace_id(&mut w, trace_id);
    w.start("config", &[]);
    Streamed::from_open(w)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_goes_inside_config() {
        let s = streamed(&Db("candidate".into()), None);
        assert_eq!(
            s.prefix,
            format!("<edit-config xmlns:nc=\"{NC_NS}\"><target><candidate/></target><config>")
        );
        assert_eq!(s.suffix, "</config></edit-config>");
    }

    #[test]
    #[cfg(feature = "tailf-compat")]
    fn trace_id_follows_the_target_with_a_quoted_namespace() {
        let s = streamed(&Db::default(), Some("6b8a4c52-a4c3-4a3f-8e0e-8a2f5c1b9d11"));
        assert!(s.prefix.contains(
            "</target><trace-id xmlns=\"http://tail-f.com/ns/netconf/ncs\">6b8a4c52-a4c3-4a3f-8e0e-8a2f5c1b9d11</trace-id><config>"
        ));
    }
}
