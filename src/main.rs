//! Binary entry point; the only place that prints errors and sets the exit code.

use std::io::{self, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args();
    if let Some(name) = args
        .next()
        .as_deref()
        .and_then(|path| std::path::Path::new(path).file_name())
        .and_then(|name| name.to_str())
    {
        ncrust::cli::set_program_name(name);
    }
    let args: Vec<String> = args.collect();
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr();
    let result = ncrust::run(&args, &mut stdout, &mut stderr).and_then(|()| Ok(stdout.flush()?));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            let code = ncrust::error::report(&err, &mut stdout, &mut stderr);
            ExitCode::from(code)
        }
    }
}
