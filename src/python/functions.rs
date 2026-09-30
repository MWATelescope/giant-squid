// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The module functions: `parse_many_job_ids_or_obs_ids`, and one `*_params`
//! function per job type.
//!
//! A `*_params` function takes the same arguments as the `AsvoClient` submit
//! method of its name (without `submit_`), and returns the request body
//! that the method would send, as a `dict`. It makes no request, so a caller
//! can use it for its own dry run. It builds the body with the same argument
//! struct as the submit method (see `super::params`), so it applies the same
//! schema defaults and the same checks. A pytest test checks that each
//! function has the same signature as its submit method.

use pyo3::exceptions::{PyOSError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde::Serialize;

use super::params::{
    BeamformerArgs, ConversionArgs, DownloadArgs, ImageFromJobArgs, ImagingArgs, VoltageArgs,
};
use super::types::{
    PyCentre, PyDelivery, PyDeliveryFormat, PyOutput, PyOutputMode, PyPolarization, PyWeighting,
};
use crate::asvo::apiv2::openapi::DownloadType;
use crate::asvo::AsvoJobId;
use crate::ParseError;

/// A request body as a Python `dict`, with the same keys and values as the
/// JSON that the client sends.
fn to_dict<'py>(py: Python<'py>, params: &impl Serialize) -> PyResult<Bound<'py, PyDict>> {
    let json = serde_json::to_string(params).map_err(|e| PyValueError::new_err(e.to_string()))?;
    py.import("json")?
        .call_method1("loads", (json,))?
        .cast_into::<PyDict>()
        .map_err(PyErr::from)
}

/// A Python `OSError` for an IO error on `file`, as Python's own file
/// functions raise it: `OSError(errno, strerror, filename)`, which Python
/// makes the subclass for the errno (for example `FileNotFoundError`), with
/// `filename` set. An error with no OS error number keeps the Rust message.
fn os_error(py: Python<'_>, file: &std::path::Path, source: &std::io::Error) -> PyErr {
    let filename = file.display().to_string();
    let Some(errno) = source.raw_os_error() else {
        return PyOSError::new_err(format!("{filename}: {source}"));
    };
    let strerror = py
        .import("os")
        .and_then(|os| os.call_method1("strerror", (errno,)))
        .and_then(|s| s.extract::<String>())
        .unwrap_or_else(|_| source.to_string());
    PyOSError::new_err((errno, strerror, filename))
}

/// Sort job IDs and obsids, as the CLI does with its arguments.
///
/// A string that is an integer is an obsid if it is a valid obsid, and a job
/// ID if not. Any other string is the path of a file of job IDs and obsids,
/// separated by whitespace.
///
/// Args:
///     strings: The job IDs, obsids and file paths.
///
/// Returns:
///     The job IDs and the obsids, each in the order given.
///
/// Raises:
///     ValueError: Text in a file is not an integer.
///     OSError: A file cannot be read (for example `FileNotFoundError`).
#[pyfunction]
pub fn parse_many_job_ids_or_obs_ids(
    py: Python<'_>,
    strings: Vec<String>,
) -> PyResult<(Vec<AsvoJobId>, Vec<u64>)> {
    match crate::parse_many_job_ids_or_obs_ids(&strings) {
        Ok((job_ids, obs_ids)) => Ok((job_ids, obs_ids.into_iter().map(u64::from).collect())),
        Err(ParseError::IO { file, source }) => Err(os_error(py, &file, &source)),
        Err(e @ ParseError::InsideFile { .. }) => Err(PyValueError::new_err(e.to_string())),
    }
}

/// The request body of `AsvoClient.submit_download_vis_job`, as a `dict`.
/// Makes no request.
///
/// Raises:
///     ValueError: `obs_id` is not a valid obsid.
#[pyfunction]
#[pyo3(signature = (obs_id, *, delivery=None, delivery_format=None, allow_resubmit=None))]
pub fn download_vis_job_params<'py>(
    py: Python<'py>,
    obs_id: u64,
    delivery: Option<PyDelivery>,
    delivery_format: Option<PyDeliveryFormat>,
    allow_resubmit: Option<bool>,
) -> PyResult<Bound<'py, PyDict>> {
    let mut params = DownloadArgs {
        delivery,
        delivery_format,
        allow_resubmit,
    }
    .into_params(obs_id)?;
    // The client sets this when it submits; set it here so that the body
    // is the one that is sent.
    params.download_type = DownloadType::Vis;
    to_dict(py, &params)
}

/// The request body of `AsvoClient.submit_download_meta_job`, as a
/// `dict`. Makes no request.
///
/// Raises:
///     ValueError: `obs_id` is not a valid obsid.
#[pyfunction]
#[pyo3(signature = (obs_id, *, delivery=None, delivery_format=None, allow_resubmit=None))]
pub fn download_meta_job_params<'py>(
    py: Python<'py>,
    obs_id: u64,
    delivery: Option<PyDelivery>,
    delivery_format: Option<PyDeliveryFormat>,
    allow_resubmit: Option<bool>,
) -> PyResult<Bound<'py, PyDict>> {
    let mut params = DownloadArgs {
        delivery,
        delivery_format,
        allow_resubmit,
    }
    .into_params(obs_id)?;
    params.download_type = DownloadType::Meta;
    to_dict(py, &params)
}

/// The request body of `AsvoClient.submit_conversion_job`, as a `dict`.
/// Makes no request. The arguments and errors are those of the method.
#[pyfunction]
#[pyo3(signature = (
    obs_id,
    *,
    delivery=None,
    delivery_format=None,
    output=None,
    avg_freq_res=None,
    avg_time_res=None,
    flag_edge_width=None,
    apply_di_cal=None,
    centre=None,
    custom_centre_ra=None,
    custom_centre_dec=None,
    no_apply_amps=None,
    no_digital_gains=None,
    no_flag_dc=None,
    no_geometry_delay=None,
    no_passband_gains=None,
    allow_resubmit=None,
))]
#[allow(clippy::too_many_arguments)]
pub fn conversion_job_params<'py>(
    py: Python<'py>,
    obs_id: u64,
    delivery: Option<PyDelivery>,
    delivery_format: Option<PyDeliveryFormat>,
    output: Option<PyOutput>,
    avg_freq_res: Option<f64>,
    avg_time_res: Option<f64>,
    flag_edge_width: Option<f64>,
    apply_di_cal: Option<bool>,
    centre: Option<PyCentre>,
    custom_centre_ra: Option<f64>,
    custom_centre_dec: Option<f64>,
    no_apply_amps: Option<bool>,
    no_digital_gains: Option<bool>,
    no_flag_dc: Option<bool>,
    no_geometry_delay: Option<bool>,
    no_passband_gains: Option<bool>,
    allow_resubmit: Option<bool>,
) -> PyResult<Bound<'py, PyDict>> {
    let params = ConversionArgs {
        delivery,
        delivery_format,
        output,
        avg_freq_res,
        avg_time_res,
        flag_edge_width,
        apply_di_cal,
        centre,
        custom_centre_ra,
        custom_centre_dec,
        no_apply_amps,
        no_digital_gains,
        no_flag_dc,
        no_geometry_delay,
        no_passband_gains,
        allow_resubmit,
    }
    .into_params(obs_id)?;
    to_dict(py, &params)
}

/// The request body of `AsvoClient.submit_imaging_job`, as a `dict`. Makes
/// no request. The arguments and errors are those of the method.
#[pyfunction]
#[pyo3(signature = (
    obs_id,
    *,
    delivery=None,
    delivery_format=None,
    apply_di_cal=None,
    apply_primary_beam=None,
    auto_mask=None,
    auto_threshold=None,
    abs_threshold=None,
    avg_freq_res=None,
    avg_time_res=None,
    channels_out=None,
    clean_iterations=None,
    clean_threshold=None,
    centre=None,
    custom_centre_dec=None,
    custom_centre_ra=None,
    flag_edge_width=None,
    image_size=None,
    join_channels=None,
    join_polarizations=None,
    mgain=None,
    multiscale=None,
    nmiter=None,
    no_apply_amps=None,
    nwlayers=None,
    output_mode=None,
    pixel_scale=None,
    pol=None,
    robust=None,
    uvw_max=None,
    uvw_min=None,
    weighting=None,
    wstack_nwlayers=None,
    allow_resubmit=None,
))]
#[allow(clippy::too_many_arguments)]
pub fn imaging_job_params<'py>(
    py: Python<'py>,
    obs_id: u64,
    delivery: Option<PyDelivery>,
    delivery_format: Option<PyDeliveryFormat>,
    apply_di_cal: Option<bool>,
    apply_primary_beam: Option<bool>,
    auto_mask: Option<i64>,
    auto_threshold: Option<f64>,
    abs_threshold: Option<f64>,
    avg_freq_res: Option<f64>,
    avg_time_res: Option<f64>,
    channels_out: Option<i64>,
    clean_iterations: Option<i64>,
    clean_threshold: Option<f64>,
    centre: Option<PyCentre>,
    custom_centre_dec: Option<f64>,
    custom_centre_ra: Option<f64>,
    flag_edge_width: Option<f64>,
    image_size: Option<i64>,
    join_channels: Option<bool>,
    join_polarizations: Option<bool>,
    mgain: Option<f64>,
    multiscale: Option<bool>,
    nmiter: Option<u64>,
    no_apply_amps: Option<bool>,
    nwlayers: Option<i64>,
    output_mode: Option<PyOutputMode>,
    pixel_scale: Option<f64>,
    pol: Option<PyPolarization>,
    robust: Option<f64>,
    uvw_max: Option<f64>,
    uvw_min: Option<f64>,
    weighting: Option<PyWeighting>,
    wstack_nwlayers: Option<i64>,
    allow_resubmit: Option<bool>,
) -> PyResult<Bound<'py, PyDict>> {
    let params = ImagingArgs {
        delivery,
        delivery_format,
        apply_di_cal,
        apply_primary_beam,
        auto_mask,
        auto_threshold,
        abs_threshold,
        avg_freq_res,
        avg_time_res,
        channels_out,
        clean_iterations,
        clean_threshold,
        centre,
        custom_centre_dec,
        custom_centre_ra,
        flag_edge_width,
        image_size,
        join_channels,
        join_polarizations,
        mgain,
        multiscale,
        nmiter,
        no_apply_amps,
        nwlayers,
        output_mode,
        pixel_scale,
        pol,
        robust,
        uvw_max,
        uvw_min,
        weighting,
        wstack_nwlayers,
        allow_resubmit,
    }
    .into_params(obs_id)?;
    to_dict(py, &params)
}

/// The request body of `AsvoClient.submit_image_from_job`, as a `dict`.
/// Makes no request. The arguments and errors are those of the method.
#[pyfunction]
#[pyo3(signature = (
    obs_id,
    source_job_id,
    *,
    delivery=None,
    delivery_format=None,
    apply_primary_beam=None,
    auto_mask=None,
    auto_threshold=None,
    abs_threshold=None,
    channels_out=None,
    clean_iterations=None,
    clean_threshold=None,
    image_size=None,
    join_channels=None,
    join_polarizations=None,
    mgain=None,
    multiscale=None,
    nmiter=None,
    nwlayers=None,
    output_mode=None,
    pixel_scale=None,
    pol=None,
    robust=None,
    uvw_max=None,
    uvw_min=None,
    weighting=None,
    wstack_nwlayers=None,
    allow_resubmit=None,
))]
#[allow(clippy::too_many_arguments)]
pub fn image_from_job_params<'py>(
    py: Python<'py>,
    obs_id: u64,
    source_job_id: u64,
    delivery: Option<PyDelivery>,
    delivery_format: Option<PyDeliveryFormat>,
    apply_primary_beam: Option<bool>,
    auto_mask: Option<i64>,
    auto_threshold: Option<f64>,
    abs_threshold: Option<f64>,
    channels_out: Option<i64>,
    clean_iterations: Option<i64>,
    clean_threshold: Option<f64>,
    image_size: Option<i64>,
    join_channels: Option<bool>,
    join_polarizations: Option<bool>,
    mgain: Option<f64>,
    multiscale: Option<bool>,
    nmiter: Option<u64>,
    nwlayers: Option<i64>,
    output_mode: Option<PyOutputMode>,
    pixel_scale: Option<f64>,
    pol: Option<PyPolarization>,
    robust: Option<f64>,
    uvw_max: Option<f64>,
    uvw_min: Option<f64>,
    weighting: Option<PyWeighting>,
    wstack_nwlayers: Option<i64>,
    allow_resubmit: Option<bool>,
) -> PyResult<Bound<'py, PyDict>> {
    let params = ImageFromJobArgs {
        delivery,
        delivery_format,
        apply_primary_beam,
        auto_mask,
        auto_threshold,
        abs_threshold,
        channels_out,
        clean_iterations,
        clean_threshold,
        image_size,
        join_channels,
        join_polarizations,
        mgain,
        multiscale,
        nmiter,
        nwlayers,
        output_mode,
        pixel_scale,
        pol,
        robust,
        uvw_max,
        uvw_min,
        weighting,
        wstack_nwlayers,
        allow_resubmit,
    }
    .into_params(obs_id, source_job_id)?;
    to_dict(py, &params)
}

/// The request body of `AsvoClient.submit_voltage_job`, as a `dict`. Makes
/// no request. The arguments and errors are those of the method.
#[pyfunction]
#[pyo3(signature = (
    obs_id,
    offset,
    duration,
    *,
    delivery=None,
    from_channel=None,
    to_channel=None,
    allow_resubmit=None,
))]
#[allow(clippy::too_many_arguments)]
pub fn voltage_job_params<'py>(
    py: Python<'py>,
    obs_id: u64,
    offset: i64,
    duration: u64,
    delivery: Option<String>,
    from_channel: Option<u8>,
    to_channel: Option<u8>,
    allow_resubmit: Option<bool>,
) -> PyResult<Bound<'py, PyDict>> {
    let params = VoltageArgs {
        offset,
        duration,
        delivery,
        from_channel,
        to_channel,
        allow_resubmit,
    }
    .into_params(obs_id)?;
    to_dict(py, &params)
}

/// The request body of `AsvoClient.submit_beamformer_job`, as a `dict`.
/// Makes no request.
///
/// Raises:
///     ValueError: `obs_id` is not a valid obsid.
#[pyfunction]
#[pyo3(signature = (obs_id, *, delivery=None, delivery_format=None, allow_resubmit=None))]
pub fn beamformer_job_params<'py>(
    py: Python<'py>,
    obs_id: u64,
    delivery: Option<PyDelivery>,
    delivery_format: Option<PyDeliveryFormat>,
    allow_resubmit: Option<bool>,
) -> PyResult<Bound<'py, PyDict>> {
    let params = BeamformerArgs {
        delivery,
        delivery_format,
        allow_resubmit,
    }
    .into_params(obs_id)?;
    to_dict(py, &params)
}
