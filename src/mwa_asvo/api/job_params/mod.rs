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
    ConversionJobParams, DownloadJobParams, ImagingJobFlow1Params, ImagingJobFlow2Params,
};
use super::validate::{
    validate_conversion_params, validate_image_from_job_params, validate_imaging_params,
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

/// An [`ImagingJobFlow2Params`] at the schema's defaults. `source_job_id` is
/// a placeholder (1); see [`download_defaults`]. `obs_id` is `None`: the
/// schema allows it to be `null`, and the MWA ASVO takes the Obs ID from the
/// source job.
pub(crate) fn image_from_job_defaults() -> ImagingJobFlow2Params {
    schema_defaults!(ImagingJobFlow2Params {
        source_job_id: NonZeroU64::MIN,
    })
}

/// The request body of a visibility or beamformer download job for
/// `obs_id`. The MWA ASVO finds the kind of download from the observation.
/// The voltage fields (`offset`, `duration`, `from_channel`, `to_channel`)
/// are `None`.
pub fn download_params(obs_id: ObsId) -> DownloadJobParams {
    DownloadJobParams {
        obs_id: i64::from(obs_id),
        ..download_defaults()
    }
}

/// The request body of a metadata download job for `obs_id`: a download job
/// with `only_get_metadata` set.
pub fn metadata_params(obs_id: ObsId) -> DownloadJobParams {
    DownloadJobParams {
        only_get_metadata: true,
        ..download_params(obs_id)
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
/// conversion job `source_job_id`. `obs_id` is `None`, which leaves it out of
/// the body (the same as `null`): the MWA ASVO takes it from the source job.
pub fn image_from_job_params(source_job_id: NonZeroU64) -> ImagingJobFlow2Params {
    ImagingJobFlow2Params {
        source_job_id,
        ..image_from_job_defaults()
    }
}

/// The request body of a voltage download job for `obs_id`, from `offset`
/// seconds after the start of the observation, for `duration` seconds. The
/// other fields are at the schema's defaults, which are those of every
/// download job: the MWA ASVO allows only some of them for a voltage job,
/// which [`validate_voltage_params`](super::validate::validate_voltage_params)
/// checks.
pub fn voltage_params(obs_id: ObsId, offset: i64, duration: u64) -> DownloadJobParams {
    DownloadJobParams {
        offset: Some(offset),
        duration: Some(duration),
        ..download_params(obs_id)
    }
}

/// The request body of a preprocessing job: one of the schema types that
/// the preprocess endpoint takes. The body has no field that names its
/// type: the MWA ASVO tells them apart by their fields (for example, only
/// an imaging body has the imaging fields), so the body is serialised as
/// the type it holds.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum PreprocessJobParams {
    /// A conversion job.
    Conversion(Box<ConversionJobParams>),
    /// An imaging job from an Obs ID (flow 1).
    Imaging(Box<ImagingJobFlow1Params>),
    /// An imaging job from a conversion job (flow 2).
    ImageFromJob(Box<ImagingJobFlow2Params>),
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
            Self::ImageFromJob(p) => validate_image_from_job_params(p),
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

impl From<ImagingJobFlow2Params> for PreprocessJobParams {
    fn from(params: ImagingJobFlow2Params) -> Self {
        Self::ImageFromJob(Box::new(params))
    }
}

#[cfg(test)]
mod tests;
