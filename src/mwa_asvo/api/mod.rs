// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The MWA ASVO API (version 2): the client, its errors, the limits of the
//! request bodies, and the types generated from the API's OpenAPI schema.
//!
//! # What the library uses of the API
//!
//! The library wraps only the endpoints that giant-squid uses: login and
//! refresh, the job list, the two job submissions (the download endpoint
//! for visibility, metadata, voltage and beamformer jobs, and the
//! preprocess endpoint for conversion, imaging and imaging from a job) and
//! the cancellation of a job.
//!
//! The API has other endpoints that the library does not call and does not
//! wrap: the account and session endpoints (`/v2/user`, `/v2/update_profile`,
//! `/v2/new_api_key`, `/v2/login`, `/v2/logout`, `/v2/auth_status`,
//! `/v2/get_socket_token`), `/v2/observation/metadata/{obs_id}`,
//! `/v2/project_permissions`, `/v2/projects_per_pi`, the health checks, and
//! the endpoints for the MWA ASVO's own processors and scheduler
//! (`/v2/job_staged`, `/v2/scheduler/*` and `/v2/calibration_ready`). Their
//! request and response types stay in [`openapi`], because that file is
//! generated from the whole schema, but nothing here uses them.

pub(crate) mod client;
pub(crate) mod error;
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
pub mod job_params;
pub mod schema_enums;
pub mod validate;

pub(crate) use error::AsvoApiError;

#[cfg(test)]
mod tests;
