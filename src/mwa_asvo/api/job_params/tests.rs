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
    assert_defaults_are_the_schemas("VisibilityJobParams", &visibility_defaults());
    assert_defaults_are_the_schemas("VoltageJobParams", &voltage_defaults());
    assert_defaults_are_the_schemas("BeamformerJobParams", &beamformer_defaults());
    assert_defaults_are_the_schemas("ConversionJobParams", &conversion_defaults());
    assert_defaults_are_the_schemas("ImagingJobFlow1Params", &imaging_defaults());
    assert_defaults_are_the_schemas("ImagingJobFlow2Params", &image_from_job_defaults());
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

    let visibility = visibility_params(obs_id);
    assert_eq!(
        VisibilityJobParams {
            obs_id: 0,
            ..visibility.clone()
        },
        visibility_defaults()
    );
    assert_eq!(visibility.obs_id, i64::from(obs_id));
    assert_eq!(visibility.download_type, DownloadType::Vis);

    let metadata = metadata_params(obs_id);
    assert_eq!(
        VisibilityJobParams {
            download_type: DownloadType::Vis,
            ..metadata.clone()
        },
        visibility
    );
    assert_eq!(metadata.download_type, DownloadType::Meta);

    let voltage = voltage_params(obs_id, 5, 8);
    assert_eq!(
        VoltageJobParams {
            obs_id: 0,
            offset: 0,
            duration: 0,
            ..voltage.clone()
        },
        voltage_defaults()
    );
    assert_eq!(
        (voltage.obs_id, voltage.offset, voltage.duration),
        (i64::from(obs_id), 5, 8)
    );

    let beamformer = beamformer_params(obs_id);
    assert_eq!(
        BeamformerJobParams {
            obs_id: 0,
            ..beamformer.clone()
        },
        beamformer_defaults()
    );
    assert_eq!(beamformer.obs_id, i64::from(obs_id));

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

/// A download body is sent as the body that it holds, with no tag. Only a
/// visibility body has `download_type`, and only a voltage body has
/// `offset` and `duration`: that is how the MWA ASVO tells them apart.
#[test]
fn a_download_body_is_sent_as_the_body_it_holds() {
    let obs_id = test_obs_id();
    let cases = [
        (
            DownloadJobParams::from(visibility_params(obs_id)),
            serde_json::to_value(visibility_params(obs_id)),
        ),
        (
            DownloadJobParams::from(voltage_params(obs_id, 1, 2)),
            serde_json::to_value(voltage_params(obs_id, 1, 2)),
        ),
        (
            DownloadJobParams::from(beamformer_params(obs_id)),
            serde_json::to_value(beamformer_params(obs_id)),
        ),
    ];
    for (params, expected) in cases {
        assert_eq!(
            serde_json::to_value(&params).expect("JSON"),
            expected.expect("JSON")
        );
    }

    let keys = |params: DownloadJobParams| {
        let json = serde_json::to_value(params).expect("JSON");
        let object = json.as_object().expect("an object").clone();
        (
            object.contains_key("download_type"),
            object.contains_key("offset") && object.contains_key("duration"),
            object.contains_key("delivery"),
        )
    };
    assert_eq!(keys(visibility_params(obs_id).into()), (true, false, true));
    assert_eq!(
        keys(voltage_params(obs_id, 1, 2).into()),
        (false, true, false)
    );
    assert_eq!(keys(beamformer_params(obs_id).into()), (false, false, true));
}

/// A download body is checked with the check of the body that it holds.
#[test]
fn a_download_body_is_checked_as_the_body_it_holds() {
    let obs_id = test_obs_id();
    assert!(DownloadJobParams::from(voltage_params(obs_id, 0, 8))
        .validate()
        .is_ok());
    assert!(matches!(
        DownloadJobParams::from(voltage_params(obs_id, -1, 8)).validate(),
        Err(AsvoApiError::InvalidParameter { name: "offset", .. })
    ));
}
