//! qx-dwg: Quantix's reader for DWG and DXF drawings. The Quantix service runs it as a separate process, so a
//! drawing the young reader library can't cope with only ever stops this process, never the service.
//!
//! - `qx-dwg read <drawing> <folder>` writes the drawing's objects to the folder (see `read`).
//! - `qx-dwg faces` finds the closed regions a set of segments encloses (see `faces`).
//! - `qx-dwg sample <spec.json> <drawing>` writes a synthetic drawing for tests (see `sample`).
//!
//! Exit codes: 0 done, 2 the drawing can't be read (the message says why, in plain words), 1 anything else.

mod faces;
mod geometry;
mod read;
mod sample;

use std::process::ExitCode;

pub enum Failure {
    Unreadable(String),
    Other(String),
    Usage,
}

impl Failure {
    fn io(error: std::io::Error) -> Failure {
        Failure::Other(error.to_string())
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("read") if args.len() == 3 => read::run(&args[1], &args[2]),
        Some("faces") if args.len() == 1 => faces::run(),
        Some("sample") if args.len() == 3 => sample::run(&args[1], &args[2]),
        Some("version") => {
            println!("{} {}", read::FORMAT, read::READER);
            Ok(())
        }
        _ => Err(Failure::Usage),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Unreadable(message)) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
        Err(Failure::Other(message)) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
        Err(Failure::Usage) => {
            eprintln!("usage: qx-dwg read <drawing> <folder> | qx-dwg faces | qx-dwg sample <spec.json> <drawing> | qx-dwg version");
            ExitCode::from(1)
        }
    }
}
