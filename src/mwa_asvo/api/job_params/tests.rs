// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Tests of the request bodies at the schema's defaults.

use serde_json::Value;

use super::*;
use crate::test_config::test_obs_id;

/// The OpenAPI schema that `openapi.rs` is generated from.
const SCHEMA: &str = include_str!("../openapi-schema.json");

/// Whether two JSON values are the same: numbers compare by value, so that
/// the schema's `20` is the `20.0` of an `f64` field.
fn same(a: &Value, b: &Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(a), Some(b)) => a == b,
        _ => a == b,
    }
}

/// Every field that the schema gives a default has that default in the body
/// that the `*_defaults` function makes. This is the check that the
/// defaults, which the CLI and Python show, are the schema's.
fn assert_defaults_are_the_schemas(schema_name: &str, body: &impl serde::Serialize) {
    let schema: Value = serde_json::from_str(SCHEMA).expect("the schema is JSON");
    let properties = schema["definitions"][schema_name]["properties"]
        .as_object()
        .unwrap_or_else(|| panic!("{schema_name} has no properties in the schema"));
    let body = serde_json::to_value(body).expect("the body is JSON");

    let mut checked = 0;
    for (field, property) in properties {
        let Some(default) = property.get("default") else {
            continue;
        };
        checked += 1;
        assert!(
            same(&body[field], default),
            "{schema_name}.{field}: the body has {}, the schema's default is {default}",
            body[field]
        );
    }
    assert!(checked > 0, "{schema_name} has no default in the schema");
}

#[test]
fn the_defaults_are_the_schemas() {
    assert_defaults_are_the_schemas("DownloadJobParams", &download_defaults());
    assert_defaults_are_the_schemas("ConversionJobParams", &conversion_defaults());
    assert_defaults_are_the_schemas("ImagingJobParams", &imaging_defaults());
    assert_defaults_are_the_schemas("ImageFromJobParams", &image_from_job_defaults());
}

/// `None` in a field that the schema allows to be `null`, and whose default
/// is not `null`, is sent as `null`; it is not "the API's default".
#[test]
fn none_in_a_field_with_a_default_is_sent_as_null() {
    let mut body = conversion_params(test_obs_id());
    assert_eq!(body.avg_freq_res, Some(40.0));
    assert_eq!(body.allow_resubmit, Some(false));

    body.avg_freq_res = None;
    body.allow_resubmit = None;
    let json = serde_json::to_value(&body).expect("JSON");
    assert_eq!(json["avg_freq_res"], Value::Null);
    assert_eq!(json["allow_resubmit"], Value::Null);
    assert!(json
        .as_object()
        .expect("an object")
        .contains_key("avg_freq_res"));
}

/// `None` in a field whose default is `null` leaves the field out, which is
/// the same as `null` for the API.
#[test]
fn none_in_a_field_whose_default_is_null_is_left_out() {
    let body = conversion_params(test_obs_id());
    assert_eq!(body.custom_centre_ra, None);
    let json = serde_json::to_value(&body).expect("JSON");
    assert!(!json
        .as_object()
        .expect("an object")
        .contains_key("custom_centre_ra"));
}

/// Each constructor sets the fields that the caller gives, and no other.
#[test]
fn the_constructors_set_the_required_fields() {
    let obs_id = test_obs_id();

    let download = download_params(obs_id);
    assert_eq!(
        DownloadJobParams {
            obs_id: 0,
            ..download.clone()
        },
        download_defaults()
    );
    assert_eq!(download.obs_id, i64::from(obs_id));

    let source_job_id = NonZeroU64::new(77).expect("not zero");
    let image = image_from_job_params(source_job_id);
    assert_eq!(image.source_job_id, source_job_id);

    // The deprecated `clean_threshold` is sent as null; `nwlayers` is left
    // out (its default is null).
    let imaging = serde_json::to_value(imaging_params(obs_id)).expect("JSON");
    let image = serde_json::to_value(image).expect("JSON");
    for body in [&imaging, &image] {
        let object = body.as_object().expect("an object");
        assert_eq!(object.get("clean_threshold"), Some(&Value::Null));
        assert!(!object.contains_key("nwlayers"));
    }

    assert_eq!(conversion_params(obs_id).obs_id, i64::from(obs_id));
    assert_eq!(imaging_params(obs_id).obs_id, i64::from(obs_id));
}

/// A preprocess body is sent as the body that it holds, with no tag.
#[test]
fn a_preprocess_body_is_sent_as_the_body_it_holds() {
    let obs_id = test_obs_id();
    let cases = [
        (
            PreprocessJobParams::from(conversion_params(obs_id)),
            serde_json::to_value(conversion_params(obs_id)),
        ),
        (
            PreprocessJobParams::from(imaging_params(obs_id)),
            serde_json::to_value(imaging_params(obs_id)),
        ),
    ];
    for (params, expected) in cases {
        assert_eq!(
            serde_json::to_value(&params).expect("JSON"),
            expected.expect("JSON")
        );
    }
}

/// A preprocess body is checked with the check of the body that it holds.
#[test]
fn a_preprocess_body_is_checked_as_the_body_it_holds() {
    let mut conversion = conversion_params(test_obs_id());
    assert!(PreprocessJobParams::from(conversion.clone())
        .validate()
        .is_ok());
    conversion.flag_edge_width = -1.0;
    assert!(matches!(
        PreprocessJobParams::from(conversion).validate(),
        Err(AsvoApiError::InvalidParameter {
            name: "flag_edge_width",
            ..
        })
    ));
}

/// A download body leaves out the voltage fields that are not set (their
/// default is null), and has `only_metadata` and the delivery at their
/// defaults. A voltage job sets the voltage fields.
#[test]
fn a_download_body_has_the_voltage_fields_only_when_set() {
    const VOLTAGE_FIELDS: [&str; 4] = ["offset", "duration", "from_channel", "to_channel"];
    let obs_id = test_obs_id();
    let object = |params: &DownloadJobParams| {
        serde_json::to_value(params)
            .expect("JSON")
            .as_object()
            .expect("an object")
            .clone()
    };

    let download = object(&download_params(obs_id));
    for field in VOLTAGE_FIELDS {
        assert!(!download.contains_key(field), "{field} is in the body");
    }
    assert_eq!(download.get("only_metadata"), Some(&Value::Bool(false)));
    assert!(download.contains_key("delivery"));
    assert!(download.contains_key("delivery_format"));

    let voltage = object(&DownloadJobParams {
        offset: Some(1),
        duration: Some(2),
        from_channel: Some(3),
        to_channel: Some(4),
        ..download_params(obs_id)
    });
    for field in VOLTAGE_FIELDS {
        assert!(voltage.contains_key(field), "{field} is not in the body");
    }
}
