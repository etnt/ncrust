//! Reading payloads (filters, edit data, rpc, session scripts) from a file or stdin.
//!
//! May depend on: config, error.

use std::fs::File;
use std::io::{self, Read};

use crate::config::InputSource;
use crate::error::Error;

/// Size of the pieces a payload is sent in.
pub const CHUNK_SIZE: usize = 16 * 1024;

fn describe(source: &InputSource) -> String {
    match source {
        InputSource::Stdin => "<stdin>".to_string(),
        InputSource::File(path) => path.display().to_string(),
    }
}

pub fn open(source: &InputSource) -> Result<Box<dyn Read>, Error> {
    match source {
        InputSource::Stdin => Ok(Box::new(io::stdin())),
        InputSource::File(path) => {
            let file =
                File::open(path).map_err(|e| Error::Stderr(format!("{}: {e}", path.display())))?;
            Ok(Box::new(file))
        }
    }
}

/// The whole input as UTF-8 text (filters).
pub fn read_to_string(source: &InputSource) -> Result<String, Error> {
    let mut bytes = Vec::new();
    open(source)?
        .read_to_end(&mut bytes)
        .map_err(|e| Error::Stderr(format!("{}: {e}", describe(source))))?;
    String::from_utf8(bytes)
        .map_err(|_| Error::Stderr(format!("{}: not valid UTF-8", describe(source))))
}

/// Calls `f` with successive pieces of at most `CHUNK_SIZE` bytes until the end of `reader`.
pub fn for_each_chunk(
    reader: &mut dyn Read,
    mut f: impl FnMut(&[u8]) -> Result<(), Error>,
) -> Result<(), Error> {
    let mut buf = vec![0u8; CHUNK_SIZE];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => return Ok(()),
            Ok(n) => f(&buf[..n])?,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn file_with(bytes: &[u8]) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(bytes).unwrap();
        f
    }

    #[test]
    fn reads_a_file_as_text() {
        let f = file_with("<a>é</a>".as_bytes());
        let source = InputSource::File(f.path().to_path_buf());
        assert_eq!(read_to_string(&source).unwrap(), "<a>é</a>");
    }

    #[test]
    fn invalid_utf8_and_missing_files_are_errors_naming_the_input() {
        let f = file_with(&[0xff, 0xfe]);
        let bad = InputSource::File(f.path().to_path_buf());
        let msg = read_to_string(&bad).unwrap_err().to_string();
        assert!(msg.ends_with("not valid UTF-8") && msg.contains(&*f.path().to_string_lossy()));

        let missing = InputSource::File("/no/such/file.xml".into());
        assert!(
            read_to_string(&missing)
                .unwrap_err()
                .to_string()
                .starts_with("/no/such/file.xml: ")
        );
    }

    #[test]
    fn large_input_arrives_in_chunks_of_at_most_16_kib() {
        let data: Vec<u8> = (0..40_000u32).map(|i| (i % 251) as u8).collect();
        let f = file_with(&data);
        let mut reader = open(&InputSource::File(f.path().to_path_buf())).unwrap();

        let mut sizes = Vec::new();
        let mut collected = Vec::new();
        for_each_chunk(&mut reader, |chunk| {
            sizes.push(chunk.len());
            collected.extend_from_slice(chunk);
            Ok(())
        })
        .unwrap();

        assert_eq!(collected, data);
        assert!(sizes.iter().all(|&n| n <= CHUNK_SIZE), "{sizes:?}");
        assert_eq!(sizes.iter().sum::<usize>(), 40_000);
    }

    #[test]
    fn chunk_callback_errors_stop_the_loop() {
        let mut reader: &[u8] = b"abc";
        let err = for_each_chunk(&mut reader, |_| Err(Error::Stderr("stop".into()))).unwrap_err();
        assert_eq!(err.to_string(), "stop");
    }
}
