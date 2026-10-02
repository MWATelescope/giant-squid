// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The `giant-squid` program. The command is in the library
//! (`mwa_giant_squid::cli::run`), which the Python module runs as well.

use std::process::ExitCode;

fn main() -> ExitCode {
    let code = mwa_giant_squid::cli::run::run_cli(std::env::args_os());
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}
