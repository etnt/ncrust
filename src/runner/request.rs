//! Turns an `Action` into the operation to send, reading filter files on the way.
//!
//! May depend on: config, error, input, protocol.

use crate::config::{Action, Filter, ReadOptions};
use crate::error::Error;
use crate::input;
use crate::protocol::filter::ResolvedFilter;
use crate::protocol::ops::get::Get;
use crate::protocol::ops::get_config::GetConfig;
use crate::protocol::ops::{Operation, ReadParams};

/// The operation for actions that send one generated request; `None` for the others
/// (`Hello`, and actions that stream a payload from a file).
pub fn build_operation(action: &Action) -> Result<Option<Box<dyn Operation>>, Error> {
    Ok(match action {
        Action::Get(options) => Some(Box::new(Get(read_params(options)?))),
        Action::GetConfig { db, options } => Some(Box::new(GetConfig {
            source: db.clone(),
            params: read_params(options)?,
        })),
        _ => None,
    })
}

fn read_params(options: &ReadOptions) -> Result<ReadParams, Error> {
    let filter = match &options.filter {
        None => None,
        Some(Filter::XPath(select)) => Some(ResolvedFilter::XPath(select.clone())),
        Some(Filter::Subtree(source)) => {
            Some(ResolvedFilter::Subtree(input::read_to_string(source)?))
        }
    };
    Ok(ReadParams {
        filter,
        with_defaults: options.with_defaults,
        with_inactive: options.with_inactive,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Db, InputSource};
    use crate::protocol::ops::rpc;

    #[test]
    fn only_request_building_actions_have_an_operation() {
        assert!(build_operation(&Action::Hello).unwrap().is_none());
        assert!(build_operation(&Action::Commit).unwrap().is_none());
        assert!(
            build_operation(&Action::Get(ReadOptions::default()))
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn get_config_reads_the_subtree_filter_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("filter.xml");
        std::fs::write(&path, "<a/>").unwrap();
        let action = Action::GetConfig {
            db: Db::default(),
            options: ReadOptions {
                filter: Some(Filter::Subtree(InputSource::File(path))),
                ..ReadOptions::default()
            },
        };
        let msg = rpc(&*build_operation(&action).unwrap().unwrap(), 1, "");
        assert!(
            msg.contains("<filter type=\"subtree\"><a/></filter>"),
            "{msg}"
        );
    }

    #[test]
    fn a_missing_filter_file_is_an_error() {
        let action = Action::Get(ReadOptions {
            filter: Some(Filter::Subtree(InputSource::File(
                "/no/such/filter.xml".into(),
            ))),
            ..ReadOptions::default()
        });
        assert!(build_operation(&action).is_err());
    }
}
