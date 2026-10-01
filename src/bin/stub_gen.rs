// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Writes the Python type stubs, `mwa_giant_squid.pyi`, from the
//! annotations in `src/python`. Run it from the repository root:
//!
//! ```text
//! cargo run --no-default-features --features python-stubgen --bin stub_gen
//! ```

fn main() -> pyo3_stub_gen::Result<()> {
    mwa_giant_squid::stub_info()?.generate()
}
