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

use serde::Serialize;

use super::error::AsvoApiError;
use super::openapi::{
    BeamformerJobParams, ConversionJobParams, DownloadType, ImagingJobFlow1Params,
    ImagingJobFlow2Params, VisibilityJobParams, VoltageJobParams,
};
use super::validate::{
    validate_beamformer_params, validate_conversion_params, validate_imaging_params,
    validate_visibility_params, validate_voltage_params,
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

/// A [`VisibilityJobParams`] at the schema's defaults. `obs_id` is required
/// by the schema, so it is a placeholder (0), not a valid Obs ID: it is for
/// code that needs only the defaults. [`visibility_params`] sets it.
pub(crate) fn visibility_defaults() -> VisibilityJobParams {
    schema_defaults!(VisibilityJobParams { obs_id: 0_i64 })
}

/// A [`VoltageJobParams`] at the schema's defaults. `obs_id`, `offset` and
/// `duration` are required by the schema, so they are placeholders (0); see
/// [`visibility_defaults`].
pub(crate) fn voltage_defaults() -> VoltageJobParams {
    schema_defaults!(VoltageJobParams {
        obs_id: 0_i64,
        offset: 0_i64,
        duration: 0_u64,
    })
}

/// A [`BeamformerJobParams`] at the schema's defaults. `obs_id` is a
/// placeholder (0); see [`visibility_defaults`].
pub(crate) fn beamformer_defaults() -> BeamformerJobParams {
    schema_defaults!(BeamformerJobParams { obs_id: 0_i64 })
}

/// A [`ConversionJobParams`] at the schema's defaults. `obs_id` is a
/// placeholder (0); see [`visibility_defaults`].
pub(crate) fn conversion_defaults() -> ConversionJobParams {
    schema_defaults!(ConversionJobParams { obs_id: 0_i64 })
}

/// An [`ImagingJobFlow1Params`] at the schema's defaults. `obs_id` is a
/// placeholder (0); see [`visibility_defaults`].
pub(crate) fn imaging_defaults() -> ImagingJobFlow1Params {
    schema_defaults!(ImagingJobFlow1Params { obs_id: 0_i64 })
}

/// An [`ImagingJobFlow2Params`] at the schema's defaults. `source_job_id` is
/// a placeholder (1); see [`visibility_defaults`].
pub(crate) fn image_from_job_defaults() -> ImagingJobFlow2Params {
    schema_defaults!(ImagingJobFlow2Params {
        source_job_id: NonZeroU64::MIN,
    })
}

/// The request body of a raw visibility download job for `obs_id`
/// (`download_type` is `vis`).
pub fn visibility_params(obs_id: ObsId) -> VisibilityJobParams {
    VisibilityJobParams {
        obs_id: i64::from(obs_id),
        download_type: DownloadType::Vis,
        ..visibility_defaults()
    }
}

/// The request body of a metadata download job for `obs_id`: a visibility
/// download body whose `download_type` is `meta`.
pub fn metadata_params(obs_id: ObsId) -> VisibilityJobParams {
    VisibilityJobParams {
        download_type: DownloadType::Meta,
        ..visibility_params(obs_id)
    }
}

/// The request body of a voltage download job for `obs_id`, from `offset`
/// seconds after the start of the observation, for `duration` seconds. The
/// body has no delivery: the MWA ASVO always delivers a voltage job to
/// Scratch, as files.
pub fn voltage_params(obs_id: ObsId, offset: i64, duration: u64) -> VoltageJobParams {
    VoltageJobParams {
        obs_id: i64::from(obs_id),
        offset,
        duration,
        ..voltage_defaults()
    }
}

/// The request body of a beamformer download job for `obs_id`.
pub fn beamformer_params(obs_id: ObsId) -> BeamformerJobParams {
    BeamformerJobParams {
        obs_id: i64::from(obs_id),
        ..beamformer_defaults()
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
///
/// `clean_threshold` is `None` (sent as `null`), not the schema's default:
/// the schema marks it as deprecated (use `abs_threshold`), so the library
/// does not send a value for it.
pub fn imaging_params(obs_id: ObsId) -> ImagingJobFlow1Params {
    ImagingJobFlow1Params {
        obs_id: i64::from(obs_id),
        clean_threshold: None,
        ..imaging_defaults()
    }
}

/// The request body of an imaging job (flow 2) for an image of the
/// conversion job `source_job_id`. The MWA ASVO takes the Obs ID from the
/// source job.
///
/// `clean_threshold` is `None` (sent as `null`); see [`imaging_params`].
pub fn image_from_job_params(source_job_id: NonZeroU64) -> ImagingJobFlow2Params {
    ImagingJobFlow2Params {
        source_job_id,
        clean_threshold: None,
        ..image_from_job_defaults()
    }
}

/// The request body of a download job: one of the schema types that the
/// download endpoint takes. The body has no field that names its type: the
/// MWA ASVO tells them apart by their fields (only a voltage body has
/// `offset` and `duration`, and only a visibility body has
/// `download_type`), so the body is serialised as the type it holds.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum DownloadJobParams {
    /// A beamformer download job.
    Beamformer(BeamformerJobParams),
    /// A visibility or metadata download job.
    Visibility(VisibilityJobParams),
    /// A voltage download job.
    Voltage(VoltageJobParams),
}

impl DownloadJobParams {
    /// Check the body against the limits of the schema, with the
    /// `validate_*_params` function of its type.
    ///
    /// # Errors
    ///
    /// [`AsvoApiError::InvalidParameter`] for the first value that is not
    /// allowed.
    pub fn validate(&self) -> Result<(), AsvoApiError> {
        match self {
            Self::Beamformer(p) => validate_beamformer_params(p),
            Self::Visibility(p) => validate_visibility_params(p),
            Self::Voltage(p) => validate_voltage_params(p),
        }
    }
}

impl From<BeamformerJobParams> for DownloadJobParams {
    fn from(params: BeamformerJobParams) -> Self {
        Self::Beamformer(params)
    }
}

impl From<VisibilityJobParams> for DownloadJobParams {
    fn from(params: VisibilityJobParams) -> Self {
        Self::Visibility(params)
    }
}

impl From<VoltageJobParams> for DownloadJobParams {
    fn from(params: VoltageJobParams) -> Self {
        Self::Voltage(params)
    }
}

/// The request body of a preprocessing job: one of the schema types that
/// the preprocess endpoint takes. The body has no field that names its
/// type: the MWA ASVO tells them apart by their fields (for example, only
/// an imaging body has the imaging fields), so the body is serialised as
/// the type it holds. An imaging job from a conversion job (flow 2) has its
/// own endpoint ([`super::client::AsvoClient::submit_image_from_job`]).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum PreprocessJobParams {
    /// A conversion job.
    Conversion(Box<ConversionJobParams>),
    /// An imaging job from an Obs ID (flow 1).
    Imaging(Box<ImagingJobFlow1Params>),
}

impl PreprocessJobParams {
    /// Check the body against the limits of the schema, with the
    /// `validate_*_params` function of its type.
    ///
    /// # Errors
    ///
    /// [`AsvoApiError::InvalidParameter`] for the first value that is not
    /// allowed.
    pub fn validate(&self) -> Result<(), AsvoApiError> {
        match self {
            Self::Conversion(p) => validate_conversion_params(p),
            Self::Imaging(p) => validate_imaging_params(p),
        }
    }
}

impl From<ConversionJobParams> for PreprocessJobParams {
    fn from(params: ConversionJobParams) -> Self {
        Self::Conversion(Box::new(params))
    }
}

impl From<ImagingJobFlow1Params> for PreprocessJobParams {
    fn from(params: ImagingJobFlow1Params) -> Self {
        Self::Imaging(Box::new(params))
    }
}

#[cfg(test)]
mod tests;
