// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Tests of the versions that `openapi.rs` records.

use super::openapi::{MWA_ASVO_API_VERSION, OPENAPI_VERSION};

/// The schema file that `openapi.rs` is generated from.
const SCHEMA: &str = include_str!("openapi-schema.json");

/// The version constants of the generated file are the versions in the schema
/// file, which `tools/generate_openapi.sh` takes from the server's document. A
/// schema file that is regenerated without `openapi.rs` fails this test.
#[test]
fn the_recorded_versions_are_those_of_the_schema_file() {
    let schema: serde_json::Value = serde_json::from_str(SCHEMA).expect("the schema is JSON");
    assert_eq!(schema["openapi"], OPENAPI_VERSION);
    assert_eq!(schema["info"]["version"], MWA_ASVO_API_VERSION);
}

/// Both versions are set: a version is dotted numbers.
#[test]
fn the_recorded_versions_look_like_versions() {
    for version in [OPENAPI_VERSION, MWA_ASVO_API_VERSION] {
        let parts: Vec<&str> = version.split('.').collect();
        assert!(
            parts.len() >= 2 && parts.iter().all(|p| p.parse::<u32>().is_ok()),
            "{version:?}"
        );
    }
}
