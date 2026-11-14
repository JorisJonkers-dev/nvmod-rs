// SPDX-License-Identifier: GPL-3.0-only
//! Entry point of the `nvmod` binary: arguments in, `nvmod_cli::run` does the work.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    ExitCode::from(nvmod_cli::run_logged(
        &args,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    ))
}
