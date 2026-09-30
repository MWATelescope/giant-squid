// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

pub mod client;
mod error;
// Generated code. Lints are allowed here, on the module declaration, so
// that the allows survive regeneration of openapi.rs itself:
// - `dead_code`: typify emits a serde default helper for every schema
//   default, and not every one of them is reachable from the types we
//   actually use (e.g. `default_i64`).
// - `clippy::derivable_impls`: the typify used for schema v1.11 writes
//   `impl Default` by hand for enums with a default variant.
#[allow(dead_code, clippy::derivable_impls)]
pub mod openapi;
pub mod validate;

pub use error::AsvoApiError;
