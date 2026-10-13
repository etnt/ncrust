//! Binary entry point; the only place that prints errors and sets the exit code.

use std::io::{self, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut stdout = io::stdout().lock();
    let result = ncrust::run(&args, &mut stdout).and_then(|()| Ok(stdout.flush()?));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            let code = ncrust::error::report(&err, &mut stdout, &mut io::stderr());
            ExitCode::from(code)
        }
    }
}
