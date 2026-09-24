// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! An alternative, efficient and easy-to-use interface for the MWA ASVO.

pub mod asvo;
// The CLI definition needs clap, which only the binary feature pulls in. It
// lives here rather than in src/bin so that tests can parse argument
// vectors directly - see docs/TESTING.md.
#[cfg(feature = "bin")]
pub mod cli;
mod helpers;
pub mod obsid;
#[cfg(test)]
mod test_common;

// Re-exports.
pub use asvo::*;
pub use helpers::*;
pub use obsid::Obsid;

// Include the generated-file as a separate module
pub mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}
