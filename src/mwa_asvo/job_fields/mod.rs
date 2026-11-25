// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The fields of a listed job that a person can ask to see, for example
//! `list --fields avg_time_res,error_text`.
//!
//! A field is a name from the OpenAPI schema: either a key of the job's
//! `job_params` or a top-level key of the job that the job table does not
//! already show. The job's type decides which of the schema's parameter
//! types it has, so a field that the type does not have has no value (for
//! example, a visibility job has no `avg_time_res`).
//!
//! The `job_params` of a listed job are the schema type, so a key that the
//! server did not send has the schema's default here.

use std::sync::LazyLock;

use serde_json::Value;

use super::api::schema_enums::sanitize_identifier;
use super::{JobType, MwaAsvoJob};

/// The schema, which gives the keys of each type.
const SCHEMA_JSON: &str = include_str!("../api/openapi-schema.json");

/// The top-level keys of a job that the job table does not show. The others
/// (`job_id`, `obs_id`, `job_type`, `job_state`, `product`, `completed`) are
/// in the table, and `job_params` is shown by its own keys.
pub(crate) const TOP_LEVEL_FIELDS: [&str; 8] = [
    "user_id",
    "first_name",
    "last_name",
    "created",
    "started",
    "modified",
    "error_code",
    "error_text",
];

/// The schema types of `job_params`, in the order in which their keys are
/// listed.
const PARAMS_TYPES: [&str; 4] = [
    "DownloadJobParams",
    "ConversionJobParams",
    "ImageFromJobParams",
    "ImagingJobParams",
];

/// The text of a field that the job has, with no value (a JSON `null`).
const NULL_TEXT: &str = "null";

/// The keys of each of the [`PARAMS_TYPES`], and every field name.
struct Fields {
    /// Every field name: [`TOP_LEVEL_FIELDS`], then the keys of the
    /// parameter types that are not a column of the table already, once.
    names: Vec<String>,
    /// The keys of each of the [`PARAMS_TYPES`], in the same order.
    params_keys: Vec<Vec<String>>,
}

static FIELDS: LazyLock<Fields> = LazyLock::new(|| {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("BUG: the schema is not JSON");
    let keys_of = |definition: &str| -> Vec<String> {
        schema["definitions"][definition]["properties"]
            .as_object()
            .unwrap_or_else(|| panic!("BUG: the schema has no properties for {definition}"))
            .keys()
            .cloned()
            .collect()
    };
    let params_keys: Vec<Vec<String>> = PARAMS_TYPES.iter().map(|t| keys_of(t)).collect();

    // A key of `job_params` that a top-level key or the table has already
    // (`obs_id`) is not listed again.
    let mut names: Vec<String> = TOP_LEVEL_FIELDS.iter().map(ToString::to_string).collect();
    for key in params_keys.iter().flatten() {
        if key != "obs_id" && !names.contains(key) {
            names.push(key.clone());
        }
    }
    Fields { names, params_keys }
});

/// Every field name that can be asked for: the top-level fields of a job
/// that the job table does not show, then the keys of the parameter types.
pub fn job_field_names() -> &'static [String] {
    &FIELDS.names
}

/// The field name that `text` means, if there is one. The case, hyphens and
/// underscores do not matter, as for every named value.
///
/// # Errors
///
/// The text of the error says that `text` is not a field, and lists the
/// field names.
pub fn parse_job_field(text: &str) -> Result<&'static str, String> {
    let wanted = sanitize_identifier(text);
    job_field_names()
        .iter()
        .find(|name| sanitize_identifier(name) == wanted)
        .map(String::as_str)
        .ok_or_else(|| {
            format!(
                "`{text}` is not a job field: expected one of: {}",
                job_field_names().join(", ")
            )
        })
}

/// The schema type of the `job_params` of `job` (an index into
/// [`PARAMS_TYPES`]) and its value as JSON. A job whose type has no
/// parameters (`cancel`), or whose `job_params` the schema type cannot read
/// (for example an old job), has none.
fn job_params_of(job: &MwaAsvoJob) -> Option<(usize, Value)> {
    let params = &job.job_params;
    let (index, value) = match job.job_type {
        JobType::Visibility | JobType::Metadata | JobType::Voltage | JobType::Beamformer => {
            (0, serde_json::to_value(params.subtype_0.as_ref()?))
        }
        JobType::Conversion => (1, serde_json::to_value(params.subtype_1.as_ref()?)),
        // An image from a conversion job has a `source_job_id`; an image
        // from an Obs ID does not.
        JobType::Imaging => match &params.subtype_2 {
            Some(from_job) => (2, serde_json::to_value(from_job)),
            None => (3, serde_json::to_value(params.subtype_3.as_ref()?)),
        },
        JobType::Cancel => return None,
    };
    Some((index, value.ok()?))
}

/// The value of the field `name` of `job`, as text: `None` if the job has no
/// such field (the job's type does not have it, or `name` is not a field),
/// and `null` for a field that the job has with no value.
///
/// A top-level field is the job's own value; `created`, `started` and
/// `modified` are RFC 3339 times in UTC.
pub fn job_field_value(job: &MwaAsvoJob, name: &str) -> Option<String> {
    let name = parse_job_field(name).ok()?;
    let value = if TOP_LEVEL_FIELDS.contains(&name) {
        serde_json::to_value(job).ok()?.get(name).cloned()
    } else {
        let (index, params) = job_params_of(job)?;
        if !FIELDS.params_keys[index].iter().any(|key| key == name) {
            return None;
        }
        params.get(name).cloned()
    };
    Some(match value {
        None | Some(Value::Null) => NULL_TEXT.to_string(),
        Some(Value::String(text)) => text,
        Some(other) => other.to_string(),
    })
}

#[cfg(test)]
mod tests;
