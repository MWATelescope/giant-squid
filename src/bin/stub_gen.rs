// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Writes the Python type stubs, `mwa_giant_squid.pyi`, from the
//! annotations in `src/python`. Run it with `tools/generate_stubs.sh`, or
//! build with `--features python-stubgen` and run `target/debug/stub_gen`
//! from the crate directory.

/* use std::env;

fn main() -> pyo3_stub_gen::Result<()> {
    // pyo3-stub-gen reads CARGO_MANIFEST_DIR when it runs, to find where to
    // write the stub. `cargo run` sets it; running the binary directly does
    // not, so use the directory of this crate when it is missing.
    if env::var_os("CARGO_MANIFEST_DIR").is_none() {
        // SAFETY: set before any other thread starts.
        unsafe { env::set_var("CARGO_MANIFEST_DIR", env!("CARGO_MANIFEST_DIR")) };
    }
    mwa_giant_squid::stub_info()?.generate()
}
 */

use std::env;

fn main() -> pyo3_stub_gen::Result<()> {
    generate_stubs()?;

    Ok(())
}

fn generate_stubs() -> pyo3_stub_gen::Result<()> {
    // Generating the stub requires the below env variable to be set for some reason?
    env::set_var("CARGO_MANIFEST_DIR", env::current_dir()?);
    println!("Generating Python type stubs...");
    let stub = mwa_giant_squid::stub_info()?;
    stub.generate()?;

    Ok(())
}
