// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Tests of the job arguments and the request bodies made from them.

use super::*;
use crate::test_config::test_obs_id;

/// With no argument set, a body is the schema's defaults and the obsid.
#[test]
fn no_argument_gives_the_schema_defaults() {
    let body = ConversionArgs::default()
        .into_params(test_obs_id())
        .expect("a body");
    let defaults: ConversionJobParams = ConversionJobParams::builder()
        .obs_id(i64::from(test_obs_id()))
        .try_into()
        .expect("the defaults");
    assert_eq!(body, defaults);
}

/// A set argument reaches the body.
#[test]
fn a_set_argument_reaches_the_body() {
    let body = ImagingArgs {
        image_size: Some(4096),
        nmiter: Some(5),
        pol: Some(Polarization::Xx),
        ..ImagingArgs::default()
    }
    .into_params(test_obs_id())
    .expect("a body");
    assert_eq!(i64::from(body.image_size), 4096);
    assert_eq!(body.nmiter.get(), 5);
    assert_eq!(body.pol, Polarization::Xx);
}

/// A value that has no schema type of its own is refused with the
/// library's message.
#[test]
fn a_value_the_schema_does_not_allow_is_refused() {
    let err = ImagingArgs {
        image_size: Some(1000),
        ..ImagingArgs::default()
    }
    .into_params(test_obs_id())
    .expect_err("1000 is not an image size");
    assert!(
        matches!(
            err,
            AsvoApiError::InvalidParameter {
                name: "image_size",
                ..
            }
        ),
        "{err:?}"
    );

    let err = ImageFromJobArgs::default()
        .into_params(test_obs_id(), 0)
        .expect_err("0 is not a job ID");
    assert!(
        matches!(
            err,
            AsvoApiError::InvalidParameter {
                name: "source_job_id",
                ..
            }
        ),
        "{err:?}"
    );
}

/// `channel_range` is set exactly when a channel bound is given.
#[test]
fn channel_range_is_derived_from_the_channels() {
    let body = |from_channel, to_channel| {
        VoltageArgs {
            offset: 0,
            duration: 8,
            from_channel,
            to_channel,
            ..VoltageArgs::default()
        }
        .into_params(test_obs_id())
        .expect("a body")
    };
    assert_eq!(body(None, None).channel_range, Some(false));
    assert_eq!(body(Some(109), None).channel_range, Some(true));
    assert_eq!(body(None, Some(132)).channel_range, Some(true));
}

/// The download type is the caller's.
#[test]
fn the_download_type_is_set() {
    let body = DownloadArgs::default()
        .into_params(test_obs_id(), DownloadType::Meta)
        .expect("a body");
    assert_eq!(body.download_type, DownloadType::Meta);
}
