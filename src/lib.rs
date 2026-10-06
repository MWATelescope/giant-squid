// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! An alternative, efficient and easy-to-use interface for the MWA ASVO.

pub mod mwa_asvo;
// The CLI definition needs clap, which only the binary feature pulls in. It
// lives here rather than in src/bin so that tests can parse argument
// vectors directly - see docs/TESTING.md.
#[cfg(feature = "bin")]
pub mod cli;
mod helpers;
pub mod obs_id;
// The Python module. Built by maturin; see pyproject.toml.
#[cfg(feature = "python")]
mod python;
/// Collects the Python stub information for the `stub_gen` binary.
#[cfg(feature = "python-stubgen")]
#[doc(hidden)]
pub use python::stub_info;
#[cfg(test)]
mod test_common;
#[cfg(test)]
mod test_config;

// Re-exports.
pub use helpers::*;
/// The jiff crate, for the dates and times in this crate's API (for example
/// the `created` field of a job), so that a program uses the same jiff version.
pub use jiff;
pub use mwa_asvo::*;
pub use obs_id::ObsId;
