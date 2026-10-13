//! `<copy-config>` envelope around a streamed payload.
//!
//! May depend on: config, error, protocol::{ns, xmlw}.

use super::{Streamed, write_trace_id};
use crate::config::Db;
use crate::protocol::xmlw::XmlWriter;

/// `<copy-config>` up to and including the start of `<config>`; the payload goes inside it.
/// The trace id is placed inside `<source>`, where the Python script puts it.
pub fn streamed(target: &Db, trace_id: Option<&str>) -> Streamed {
    let mut w = XmlWriter::new();
    w.start("copy-config", &[]);
    w.start("target", &[]).empty(&target.0, &[]).end();
    w.start("source", &[]);
    write_trace_id(&mut w, trace_id);
    w.start("config", &[]);
    Streamed::from_open(w)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_goes_inside_source_config() {
        let s = streamed(&Db("startup".into()), None);
        assert_eq!(
            s.prefix,
            "<copy-config><target><startup/></target><source><config>"
        );
        assert_eq!(s.suffix, "</config></source></copy-config>");
    }
}
