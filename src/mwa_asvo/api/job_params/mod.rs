// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The request body of each job type, at the schema's defaults.
//!
//! Each `*_params` function returns the schema type of one job type with
//! every field at the default that the OpenAPI schema gives it. The caller
//! then sets the fields it wants; the fields have the schema's names and
//! types. The CLI and the Python module do this, so the defaults are the
//! schema's in every layer: they show in the value, not in a hidden "unset".
//!
//! # Null
//!
//! A field that the schema allows to be `null` is an `Option`, and `None`
//! means `null`. It does not mean "use the API's default": the default is
//! the `Some` value that these functions set. `None` in a field whose
//! default is not `null` (for example `avg_freq_res`, `allow_resubmit`) is
//! sent as `null`. `None` in a field whose default is `null` (for example
//! `custom_centre_ra`) leaves the field out of the body, which is the same.
//! The schema does not say what the API does with a `null` for each field.
//!
//! The limits of the numbers are checked when the job is submitted
//! ([`super::validate`]); the functions there make a schema value from a
//! plain number (for example [`super::validate::image_size`]).

use std::num::NonZeroU64;

use super::openapi::{
    BeamformerJobParams, ConversionJobParams, DownloadJobParams, DownloadType,
    ImagingJobFlow1Params, ImagingJobFlow2Params, VoltageJobParams,
};
use crate::obs_id::ObsId;

/// A request body that the schema's defaults make: the generated builder
/// starts from them, and fails only if a required field is missing.
macro_rules! schema_defaults {
    ($ty:ident { $($required:ident: $value:expr),* $(,)? }) => {
        $ty::builder()
            $(.$required($value))*
            .try_into()
            .expect(concat!("BUG: a required ", stringify!($ty), " field is missing"))
    };
}

/// A [`DownloadJobParams`] at the schema's defaults. `obs_id` is required
/// by the schema, so it is a placeholder (0), not a valid Obs ID: it is for
/// code that needs only the defaults. [`download_params`] sets it.
pub(crate) fn download_defaults() -> DownloadJobParams {
    schema_defaults!(DownloadJobParams { obs_id: 0_i64 })
}

/// A [`ConversionJobParams`] at the schema's defaults. `obs_id` is a
/// placeholder (0); see [`download_defaults`].
pub(crate) fn conversion_defaults() -> ConversionJobParams {
    schema_defaults!(ConversionJobParams { obs_id: 0_i64 })
}

/// An [`ImagingJobFlow1Params`] at the schema's defaults. `obs_id` is a
/// placeholder (0); see [`download_defaults`].
pub(crate) fn imaging_defaults() -> ImagingJobFlow1Params {
    schema_defaults!(ImagingJobFlow1Params { obs_id: 0_i64 })
}

/// An [`ImagingJobFlow2Params`] at the schema's defaults. `obs_id` is a
/// placeholder (0) and `source_job_id` a placeholder (1); see
/// [`download_defaults`].
pub(crate) fn image_from_job_defaults() -> ImagingJobFlow2Params {
    schema_defaults!(ImagingJobFlow2Params {
        obs_id: 0_i64,
        source_job_id: NonZeroU64::MIN,
    })
}

/// A [`VoltageJobParams`] at the schema's defaults. `obs_id`, `offset` and
/// `duration` are placeholders (0); see [`download_defaults`].
pub(crate) fn voltage_defaults() -> VoltageJobParams {
    schema_defaults!(VoltageJobParams {
        obs_id: 0_i64,
        offset: 0_i64,
        duration: 0_u64,
    })
}

/// A [`BeamformerJobParams`] at the schema's defaults. `obs_id` is a
/// placeholder (0); see [`download_defaults`].
pub(crate) fn beamformer_defaults() -> BeamformerJobParams {
    schema_defaults!(BeamformerJobParams { obs_id: 0_i64 })
}

/// The request body of a visibility or metadata download job for `obs_id`,
/// a download of `download_type`. (The client's submit methods set the
/// `download_type` of their own job type in any case.)
pub fn download_params(obs_id: ObsId, download_type: DownloadType) -> DownloadJobParams {
    DownloadJobParams {
        obs_id: i64::from(obs_id),
        download_type,
        ..download_defaults()
    }
}

/// The request body of a conversion job for `obs_id`.
pub fn conversion_params(obs_id: ObsId) -> ConversionJobParams {
    ConversionJobParams {
        obs_id: i64::from(obs_id),
        ..conversion_defaults()
    }
}

/// The request body of an imaging job (flow 1: from an Obs ID) for `obs_id`.
pub fn imaging_params(obs_id: ObsId) -> ImagingJobFlow1Params {
    ImagingJobFlow1Params {
        obs_id: i64::from(obs_id),
        ..imaging_defaults()
    }
}

/// The request body of an imaging job (flow 2) for an image of the
/// conversion job `source_job_id` of `obs_id`.
pub fn image_from_job_params(obs_id: ObsId, source_job_id: NonZeroU64) -> ImagingJobFlow2Params {
    ImagingJobFlow2Params {
        obs_id: i64::from(obs_id),
        source_job_id,
        ..image_from_job_defaults()
    }
}

/// The request body of a voltage download job for `obs_id`. `offset` and
/// `duration` are required by the schema. `channel_range` is derived from
/// `from_channel` and `to_channel` ([`set_channel_range`]), which the client
/// does when it submits the job.
pub fn voltage_params(obs_id: ObsId, offset: i64, duration: u64) -> VoltageJobParams {
    VoltageJobParams {
        obs_id: i64::from(obs_id),
        offset,
        duration,
        ..voltage_defaults()
    }
}

/// Set `channel_range` of a voltage body: `true` when `from_channel` or
/// `to_channel` is set, and `false` when neither is. The API needs the field
/// until it removes it, and any value that the caller put there is replaced.
/// [`AsvoClient::submit_voltage_job`](crate::mwa_asvo::AsvoClient::submit_voltage_job)
/// calls this, and a caller that shows the body before it is sent (a dry
/// run) can call it too.
pub fn set_channel_range(params: &mut VoltageJobParams) {
    params.channel_range = Some(params.from_channel.is_some() || params.to_channel.is_some());
}

/// The request body of a beamformer download job for `obs_id`.
pub fn beamformer_params(obs_id: ObsId) -> BeamformerJobParams {
    BeamformerJobParams {
        obs_id: i64::from(obs_id),
        ..beamformer_defaults()
    }
}

#[cfg(test)]
mod tests;
