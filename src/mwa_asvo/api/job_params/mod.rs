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
    ConversionJobParams, DownloadJobParams, ImageFromJobParams, ImagingJobParams,
};
use super::validate::{validate_conversion_params, validate_imaging_params};
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

/// An [`ImagingJobParams`] at the schema's defaults. `obs_id` is a
/// placeholder (0); see [`download_defaults`].
pub(crate) fn imaging_defaults() -> ImagingJobParams {
    schema_defaults!(ImagingJobParams { obs_id: 0_i64 })
}

/// An [`ImageFromJobParams`] at the schema's defaults. `source_job_id` is
/// a placeholder (1); see [`download_defaults`].
pub(crate) fn image_from_job_defaults() -> ImageFromJobParams {
    schema_defaults!(ImageFromJobParams {
        source_job_id: NonZeroU64::MIN,
    })
}

/// The request body of a download job for `obs_id`: a visibility,
/// metadata, voltage or beamformer job. The body does not say which: the
/// MWA ASVO finds the kind of data from the observation. Set
/// `only_metadata` for a metadata job, and `offset`, `duration`,
/// `from_channel` and `to_channel` for a voltage job; the MWA ASVO refuses
/// the voltage fields for an observation that has no voltages.
pub fn download_params(obs_id: ObsId) -> DownloadJobParams {
    DownloadJobParams {
        obs_id: i64::from(obs_id),
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

/// The request body of an imaging job (from an Obs ID) for `obs_id`.
///
/// `clean_threshold` is `None` (sent as `null`), not the schema's default:
/// the schema marks it as deprecated (use `abs_threshold`), so the library
/// does not send a value for it.
pub fn imaging_params(obs_id: ObsId) -> ImagingJobParams {
    ImagingJobParams {
        obs_id: i64::from(obs_id),
        clean_threshold: None,
        ..imaging_defaults()
    }
}

/// The request body of an imaging job from a job: an image of the
/// conversion job `source_job_id`. The MWA ASVO takes the Obs ID from the
/// source job.
///
/// `clean_threshold` is `None` (sent as `null`); see [`imaging_params`].
pub fn image_from_job_params(source_job_id: NonZeroU64) -> ImageFromJobParams {
    ImageFromJobParams {
        source_job_id,
        clean_threshold: None,
        ..image_from_job_defaults()
    }
}

/// The request body of a preprocessing job: one of the schema types that
/// the preprocess endpoint takes. The body has no field that names its
/// type: the MWA ASVO tells them apart by their fields (for example, only
/// an imaging body has the imaging fields), so the body is serialised as
/// the type it holds. An imaging job from a conversion job has its own
/// endpoint ([`super::client::AsvoClient::submit_image_from_job`]).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum PreprocessJobParams {
    /// A conversion job.
    Conversion(Box<ConversionJobParams>),
    /// An imaging job from an Obs ID.
    Imaging(Box<ImagingJobParams>),
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

impl From<ImagingJobParams> for PreprocessJobParams {
    fn from(params: ImagingJobParams) -> Self {
        Self::Imaging(Box::new(params))
    }
}

#[cfg(test)]
mod tests;
