// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

pub mod client;
mod error;
// Generated code: typify emits a serde default helper for every schema
// default, and not every one of them is reachable from the types we
// actually use (e.g. `default_i64`). Allowed here, on the module
// declaration, so it survives regeneration of openapi.rs itself.
#[allow(dead_code)]
pub mod openapi;

pub use error::AsvoApiError;
