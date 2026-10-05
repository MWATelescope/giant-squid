// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The MWA ASVO API (version 2): the client, its errors, the limits of the
//! request bodies, and the types generated from the API's OpenAPI schema.
//!
//! # What the library uses of the API
//!
//! Only the endpoints an end user needs: login and refresh, the job list, the
//! job submissions (visibilities and metadata, conversion, imaging, imaging
//! from a job, voltage, beamformer) and the cancellation of a job. The API
//! has other endpoints (`/v2/job_staged`, `/v2/scheduler/*` and
//! `/v2/calibration_ready`) that are for the MWA ASVO's own processors and
//! scheduler. The library does not call them, and does not wrap them. Their
//! request and response types stay in [`openapi`], because that file is
//! generated from the whole schema, but nothing here uses them.
//!
//! The job submission bodies also have a `staging_count` field (the number of
//! times that a processor restaged the job). It is for the processors too, and
//! the API will remove it from the submission bodies in a later release. The
//! library never sets it, and has no option or argument for it, in Rust, in
//! the CLI or in Python. With it unset, the generated types leave it out of
//! the body, which is the same as `null`, the API's default. The tests in
//! `client/tests.rs`, `cli/tests.rs` and `tests/python` keep it that way.

pub mod client;
mod error;
// Generated code. Lints are allowed here, on the module declaration, so
// that the allows survive regeneration of openapi.rs itself:
// - `dead_code`: typify emits a serde default helper for every schema
//   default, and not every one of them is reachable from the types we
//   actually use (e.g. `default_i64`).
// - `clippy::derivable_impls`: typify writes `impl Default` by hand for an
//   enum with a default variant (checked with the current typify: still so).
// `rustfmt::skip` keeps `cargo fmt` off the file: it must stay exactly as
// build.rs writes it, or the openapi-drift-check CI job fails.
#[allow(dead_code, clippy::derivable_impls)]
#[rustfmt::skip]
pub mod openapi;
pub mod schema_enums;
pub mod validate;

pub use error::AsvoApiError;
