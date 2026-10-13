//! NMDA `<edit-data>` envelope around a streamed payload.
//!
//! May depend on: config, error, protocol::{ns, xmlw}.

use super::{Streamed, write_trace_id};
use crate::config::Db;
use crate::protocol::ns::{NMDA_DS_NS, NMDA_NS};
use crate::protocol::xmlw::XmlWriter;

/// `<edit-data>` up to and including the start of `<config>`; the payload goes inside it.
pub fn streamed(target: &Db, trace_id: Option<&str>) -> Streamed {
    let mut w = XmlWriter::new();
    w.start("edit-data", &[("xmlns", NMDA_NS), ("xmlns:ds", NMDA_DS_NS)]);
    w.element("datastore", &[], &format!("ds:{}", target.0));
    write_trace_id(&mut w, trace_id);
    w.start("config", &[]);
    Streamed::from_open(w)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_goes_inside_config_in_the_nmda_namespace() {
        let s = streamed(&Db("candidate".into()), None);
        assert_eq!(
            s.prefix,
            format!(
                "<edit-data xmlns=\"{NMDA_NS}\" xmlns:ds=\"{NMDA_DS_NS}\"><datastore>ds:candidate</datastore><config>"
            )
        );
        assert_eq!(s.suffix, "</config></edit-data>");
    }
}
