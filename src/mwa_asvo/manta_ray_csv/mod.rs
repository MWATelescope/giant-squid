// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The CSV job files of `manta-ray-client` (`mwa_client -c file.csv`), so
//! that the same files submit jobs with giant-squid.
//!
//! A file has one job on each line. A line is a list of `key=value` cells
//! that a comma separates, for example:
//!
//! ```text
//! obs_id=1349904240, job_type=c, avg_time_res=1.0, output=uvfits
//! ```
//!
//! A blank line and a line that starts with `#` are ignored. The keys of
//! each job type are the ones in the README of `manta-ray-client`; the
//! values are checked against the limits of the MWA ASVO OpenAPI schema,
//! as the arguments of the `submit-*` commands are.
//!
//! This parser differs from `manta-ray-client` where that client would
//! send a mistake to the server without a word:
//!
//! - A key that the job type does not have is an error, not a value that
//!   the server ignores. A typo cannot change a job unseen.
//! - A key that a line gives twice is an error. (`manta-ray-client` uses
//!   the last.)
//! - A cell with a key and no value (`delivery=`) is ignored, as it is in
//!   `manta-ray-client`: the key has its default.
//! - A value cannot be in quotes, and the spaces around a key or a value
//!   are removed.
//! - `job_type=p` (an image of a conversion job) is an error: the format
//!   has no key for the conversion job. Use `submit-image-from-job`.
//!
//! A key that a line does not give has the default of the schema. The
//! exceptions are `delivery`, `delivery_format` and `allow_resubmit`,
//! which [`CsvDefaults`] can set for every line that does not give them.

use std::collections::BTreeMap;
use std::str::FromStr;

use serde::Serialize;
use thiserror::Error;

use crate::mwa_asvo::api::client::{
    AsvoClient, ENDPOINT_BEAMFORMER_JOB, ENDPOINT_CONVERSION_JOB, ENDPOINT_DOWNLOAD_VIS_JOB,
    ENDPOINT_IMAGING_JOB, ENDPOINT_VOLTAGE_JOB,
};
use crate::mwa_asvo::api::error::AsvoApiError;
use crate::mwa_asvo::api::job_params::{
    beamformer_params, conversion_params, download_params, imaging_params, set_channel_range,
    voltage_params,
};
use crate::mwa_asvo::api::openapi::{
    BeamformerJobParams, Centre, ConversionJobParams, Delivery, DeliveryFormat, DownloadJobParams,
    DownloadType, ImagingJobFlow1Params, JobSubmittedResponse, Output, OutputMode,
    VoltageJobParams, Weighting,
};
use crate::mwa_asvo::api::schema_enums::SchemaEnum;
use crate::mwa_asvo::api::validate::{
    self, validate_beamformer_params, validate_conversion_params, validate_download_params,
    validate_imaging_params, validate_voltage_params,
};
use crate::obs_id::ObsId;

#[cfg(test)]
mod tests;

/// The key that gives the job type of a line.
const KEY_JOB_TYPE: &str = "job_type";
/// The key of the Obs ID of a line. Every job type needs it.
const KEY_OBS_ID: &str = "obs_id";

/// The `job_type` of a conversion job.
const JOB_TYPE_CONVERSION: &str = "c";
/// The `job_type` of a visibility or metadata download job.
const JOB_TYPE_DOWNLOAD: &str = "d";
/// The `job_type` of a voltage job.
const JOB_TYPE_VOLTAGE: &str = "v";
/// The `job_type` of a beamformer job.
const JOB_TYPE_BEAMFORMER: &str = "b";
/// The `job_type` of an imaging job (from an Obs ID).
const JOB_TYPE_IMAGING: &str = "i";
/// The `job_type` of an imaging job from a conversion job, which the format
/// cannot describe.
const JOB_TYPE_IMAGING_FROM_JOB: &str = "p";

/// The `download_type` of a visibility download.
const DOWNLOAD_TYPE_VIS: &str = "vis";
/// The `download_type` of a metadata download.
const DOWNLOAD_TYPE_META: &str = "vis_meta";

/// The character that starts a comment line.
const COMMENT_CHAR: char = '#';
/// The character that separates the cells of a line.
const CELL_SEPARATOR: char = ',';
/// The character that separates the key and the value of a cell.
const KEY_VALUE_SEPARATOR: char = '=';
/// The mark of the start of a UTF-8 text file, which some programs write.
const BYTE_ORDER_MARK: char = '\u{feff}';

/// What the lines of a file do not give, for the values that the
/// environment or a command line can set for a whole file.
///
/// A key that a line gives wins. Every other key that a line does not give
/// has the schema's default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CsvDefaults {
    /// The `delivery` of a line that has none.
    pub delivery: Option<Delivery>,
    /// The `delivery_format` of a line that has none.
    pub delivery_format: Option<DeliveryFormat>,
    /// The `allow_resubmit` of a line that has none.
    pub allow_resubmit: Option<bool>,
}

/// The request body of one job of a file.
///
/// It is serialized as the request body alone (it has no tag), as the
/// `submit-*` commands send it.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum CsvJobParams {
    /// A conversion job (`job_type=c`).
    Conversion(Box<ConversionJobParams>),
    /// An imaging job from an Obs ID (`job_type=i`).
    Imaging(Box<ImagingJobFlow1Params>),
    /// A visibility or metadata download job (`job_type=d`).
    Download(DownloadJobParams),
    /// A voltage job (`job_type=v`).
    Voltage(VoltageJobParams),
    /// A beamformer job (`job_type=b`).
    Beamformer(BeamformerJobParams),
}

impl CsvJobParams {
    /// The endpoint that the request goes to.
    pub fn endpoint(&self) -> &'static str {
        match self {
            Self::Conversion(_) => ENDPOINT_CONVERSION_JOB,
            Self::Imaging(_) => ENDPOINT_IMAGING_JOB,
            Self::Download(_) => ENDPOINT_DOWNLOAD_VIS_JOB,
            Self::Voltage(_) => ENDPOINT_VOLTAGE_JOB,
            Self::Beamformer(_) => ENDPOINT_BEAMFORMER_JOB,
        }
    }

    /// The kind of job, in words: for example `conversion`.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Conversion(_) => "conversion",
            Self::Imaging(_) => "imaging",
            Self::Download(p) if p.download_type == DownloadType::Meta => "metadata download",
            Self::Download(_) => "visibility download",
            Self::Voltage(_) => "voltage",
            Self::Beamformer(_) => "beamformer",
        }
    }

    /// Check the request body against the limits of the schema, as the
    /// client does before it sends the request.
    ///
    /// # Errors
    ///
    /// [`AsvoApiError::InvalidParameter`] if a value is refused.
    pub fn validate(&self) -> Result<(), AsvoApiError> {
        match self {
            Self::Conversion(p) => validate_conversion_params(p),
            Self::Imaging(p) => validate_imaging_params(p),
            Self::Download(p) => validate_download_params(p),
            Self::Voltage(p) => validate_voltage_params(p),
            Self::Beamformer(p) => validate_beamformer_params(p),
        }
    }
}

/// One job of a file.
#[derive(Debug, Clone)]
pub struct CsvJob {
    /// The line of the file, from 1.
    pub line: usize,
    /// The Obs ID of the job.
    pub obs_id: ObsId,
    /// The request body.
    pub params: CsvJobParams,
}

/// What is wrong with one line of a file.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("line {line}: {message}")]
pub struct CsvRowError {
    /// The line of the file, from 1.
    pub line: usize,
    /// What is wrong, in words.
    pub message: String,
}

/// What is wrong with a file: every line that has a problem.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub struct CsvError {
    /// The problems, in the order of the lines. There is at least one.
    pub errors: Vec<CsvRowError>,
}

impl std::fmt::Display for CsvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self.errors.len();
        let noun = if count == 1 { "problem" } else { "problems" };
        write!(f, "{count} {noun} in the CSV file:")?;
        for error in &self.errors {
            write!(f, "\n  {error}")?;
        }
        Ok(())
    }
}

/// Read the jobs of the text of a `manta-ray-client` CSV file.
///
/// Every line is read and checked (also against the limits of the schema)
/// before the function returns, so that a file with a bad line at its end
/// submits nothing. The jobs are in the order of the lines. A file with no
/// job gives an empty list.
///
/// # Errors
///
/// [`CsvError`] with every line that has a problem.
pub fn parse_manta_ray_csv(text: &str, defaults: &CsvDefaults) -> Result<Vec<CsvJob>, CsvError> {
    let text = text.strip_prefix(BYTE_ORDER_MARK).unwrap_or(text);
    let mut jobs = Vec::new();
    let mut errors = Vec::new();

    for (index, line_text) in text.lines().enumerate() {
        let line = index + 1;
        let trimmed = line_text.trim();
        if trimmed.is_empty() || trimmed.starts_with(COMMENT_CHAR) {
            continue;
        }
        match parse_line(trimmed, defaults) {
            Ok((obs_id, params)) => jobs.push(CsvJob {
                line,
                obs_id,
                params,
            }),
            Err(message) => errors.push(CsvRowError { line, message }),
        }
    }

    if errors.is_empty() {
        Ok(jobs)
    } else {
        Err(CsvError { errors })
    }
}

/// The cells of one line, with the keys that the job type has asked for.
#[derive(Default)]
struct Row {
    cells: BTreeMap<String, String>,
    /// Every key that the job type has: the keys that the code asked for.
    known: Vec<&'static str>,
}

impl Row {
    /// Split a line into its cells.
    fn parse(line: &str) -> Result<Self, String> {
        let mut cells = BTreeMap::new();
        for cell in line.split(CELL_SEPARATOR).map(str::trim) {
            if cell.is_empty() {
                continue;
            }
            let Some((key, value)) = cell.split_once(KEY_VALUE_SEPARATOR) else {
                return Err(format!(
                    "`{cell}` is not a key{KEY_VALUE_SEPARATOR}value cell"
                ));
            };
            let (key, value) = (key.trim(), value.trim());
            if key.is_empty() {
                return Err(format!("`{cell}` has a value and no key"));
            }
            if value.is_empty() {
                continue;
            }
            if cells.insert(key.to_string(), value.to_string()).is_some() {
                return Err(format!("the key `{key}` is given twice"));
            }
        }
        Ok(Self {
            cells,
            known: Vec::new(),
        })
    }

    /// The text of the cell `key`, if the line has it.
    fn text(&mut self, key: &'static str) -> Option<String> {
        self.known.push(key);
        self.cells.remove(key)
    }

    /// The cell `key` as a number of type `T` that `expected` names.
    fn number<T: FromStr>(
        &mut self,
        key: &'static str,
        expected: &str,
    ) -> Result<Option<T>, String> {
        self.text(key)
            .map(|value| {
                value
                    .parse::<T>()
                    .map_err(|_| format!("{key}: expected {expected}, got `{value}`"))
            })
            .transpose()
    }

    /// The cell `key` as a number with a fraction.
    fn float(&mut self, key: &'static str) -> Result<Option<f64>, String> {
        self.number(key, "a number")
    }

    /// The cell `key` as a whole number.
    fn integer<T: FromStr>(&mut self, key: &'static str) -> Result<Option<T>, String> {
        self.number(key, "a whole number")
    }

    /// The cell `key` as `true` or `false`, in any case.
    fn boolean(&mut self, key: &'static str) -> Result<Option<bool>, String> {
        self.text(key)
            .map(|value| match value.to_lowercase().as_str() {
                "true" => Ok(true),
                "false" => Ok(false),
                _ => Err(format!("{key}: expected true or false, got `{value}`")),
            })
            .transpose()
    }

    /// The cell `key` as a value of the schema enum `T`, as `--delivery`
    /// and the other options read it: the case, spaces, hyphens and
    /// underscores do not matter.
    fn named<T: SchemaEnum>(&mut self, key: &'static str) -> Result<Option<T>, String> {
        self.text(key)
            .map(|value| {
                T::from_name(&value).ok_or_else(|| {
                    format!("{key}: `{value}` is not one of {}", T::names().join(", "))
                })
            })
            .transpose()
    }

    /// An error for a key that the job type does not have, if the line has
    /// one. Call it when the job type has asked for all of its keys.
    fn finish(self, job: &str) -> Result<(), String> {
        let Some(key) = self.cells.keys().next() else {
            return Ok(());
        };
        let mut known = self.known;
        known.sort_unstable();
        known.dedup();
        Err(format!(
            "unknown key `{key}` for a {job} job (the keys of this job are {})",
            known.join(", ")
        ))
    }
}

/// A value that the job type requires.
fn required<T>(value: Option<T>, key: &str) -> Result<T, String> {
    value.ok_or_else(|| format!("{key} is required"))
}

/// Read the job of one line.
fn parse_line(line: &str, defaults: &CsvDefaults) -> Result<(ObsId, CsvJobParams), String> {
    let mut row = Row::parse(line)?;
    let job_type = required(row.text(KEY_JOB_TYPE), KEY_JOB_TYPE)?.to_lowercase();
    let obs_id = required(row.integer::<u64>(KEY_OBS_ID)?, KEY_OBS_ID)?;
    let obs_id = ObsId::validate(obs_id).map_err(|e| format!("{KEY_OBS_ID}: {e}"))?;

    let params = match job_type.as_str() {
        JOB_TYPE_CONVERSION => {
            CsvJobParams::Conversion(Box::new(parse_conversion(&mut row, obs_id, defaults)?))
        }
        JOB_TYPE_IMAGING => {
            CsvJobParams::Imaging(Box::new(parse_imaging(&mut row, obs_id, defaults)?))
        }
        JOB_TYPE_DOWNLOAD => CsvJobParams::Download(parse_download(&mut row, obs_id, defaults)?),
        JOB_TYPE_VOLTAGE => CsvJobParams::Voltage(parse_voltage(&mut row, obs_id, defaults)?),
        JOB_TYPE_BEAMFORMER => {
            CsvJobParams::Beamformer(parse_beamformer(&mut row, obs_id, defaults)?)
        }
        JOB_TYPE_IMAGING_FROM_JOB => {
            return Err(format!(
                "{KEY_JOB_TYPE}={JOB_TYPE_IMAGING_FROM_JOB} (an image of a conversion job) is not \
                 supported; use the submit-image-from-job command"
            ))
        }
        other => {
            return Err(format!(
                "{KEY_JOB_TYPE}: `{other}` is not one of {JOB_TYPE_CONVERSION} (conversion), \
                 {JOB_TYPE_DOWNLOAD} (download), {JOB_TYPE_VOLTAGE} (voltage), \
                 {JOB_TYPE_BEAMFORMER} (beamformer), {JOB_TYPE_IMAGING} (imaging)"
            ))
        }
    };

    params.validate().map_err(|e| e.to_string())?;
    Ok((obs_id, params))
}

/// Set `delivery` and `delivery_format` of `params` (a schema type with
/// both) from the line, or from `defaults`, or leave the schema's default.
macro_rules! apply_delivery {
    ($row:expr, $defaults:expr, $params:expr) => {
        if let Some(delivery) = $row.named::<Delivery>("delivery")?.or($defaults.delivery) {
            $params.delivery = delivery;
        }
        if let Some(format) = $row
            .named::<DeliveryFormat>("delivery_format")?
            .or($defaults.delivery_format)
        {
            $params.delivery_format = format;
        }
    };
}

/// Set `allow_resubmit` of `params` from the line, or from `defaults`.
macro_rules! apply_resubmit {
    ($row:expr, $defaults:expr, $params:expr) => {
        if let Some(allow) = $row.boolean("allow_resubmit")?.or($defaults.allow_resubmit) {
            $params.allow_resubmit = Some(allow);
        }
    };
}

/// Set the preprocessing fields of `params` (a conversion or imaging body)
/// from the line.
macro_rules! apply_preprocessing {
    ($row:expr, $params:expr) => {
        if let Some(v) = $row.float("avg_freq_res")? {
            $params.avg_freq_res = Some(v);
        }
        if let Some(v) = $row.float("avg_time_res")? {
            $params.avg_time_res = Some(v);
        }
        if let Some(v) = $row.float("flag_edge_width")? {
            $params.flag_edge_width = v;
        }
        if let Some(v) = $row.boolean("apply_di_cal")? {
            $params.apply_di_cal = v;
        }
        if let Some(v) = $row.named::<Centre>("centre")? {
            $params.centre = v;
        }
        if let Some(v) = $row.float("phase_centre_ra")? {
            $params.custom_centre_ra = Some(v);
        }
        if let Some(v) = $row.float("phase_centre_dec")? {
            $params.custom_centre_dec = Some(v);
        }
    };
}

/// The flags of the Birli preprocessor, which `manta-ray-client` documents
/// for conversion jobs.
macro_rules! apply_birli_flags {
    ($row:expr, $params:expr) => {
        if let Some(v) = $row.boolean("no_rfi")? {
            $params.no_rfi = Some(v);
        }
        if let Some(v) = $row.boolean("no_geometric_delay")? {
            $params.no_geometric_delay = Some(v);
        }
        if let Some(v) = $row.boolean("no_cable_delay")? {
            $params.no_cable_delay = Some(v);
        }
        if let Some(v) = $row.boolean("no_digital_gains")? {
            $params.no_digital_gains = Some(v);
        }
        if let Some(v) = $row.boolean("no_passband_gains")? {
            $params.no_passband_gains = Some(v);
        }
        if let Some(v) = $row.boolean("no_flag_dc")? {
            $params.no_flag_dc = Some(v);
        }
    };
}

fn parse_conversion(
    row: &mut Row,
    obs_id: ObsId,
    defaults: &CsvDefaults,
) -> Result<ConversionJobParams, String> {
    let mut params = conversion_params(obs_id);
    apply_delivery!(row, defaults, params);
    if let Some(output) = row.named::<Output>("output")? {
        params.output = output;
    }
    apply_preprocessing!(row, params);
    apply_birli_flags!(row, params);
    apply_resubmit!(row, defaults, params);
    std::mem::take(row).finish("conversion")?;
    Ok(params)
}

fn parse_imaging(
    row: &mut Row,
    obs_id: ObsId,
    defaults: &CsvDefaults,
) -> Result<ImagingJobFlow1Params, String> {
    let mut params = imaging_params(obs_id);
    apply_delivery!(row, defaults, params);
    apply_preprocessing!(row, params);
    if let Some(v) = row.integer::<i64>("image_size")? {
        params.image_size = validate::image_size(v).map_err(|e| e.to_string())?;
    }
    if let Some(v) = row.float("pixel_scale")? {
        params.pixel_scale = v;
    }
    if let Some(v) = row.named::<Weighting>("weighting")? {
        params.weighting = v;
    }
    if let Some(v) = row.float("robust")? {
        params.robust = v;
    }
    if let Some(v) = row.integer::<i64>("clean_iterations")? {
        params.clean_iterations = v;
    }
    if let Some(v) = row.integer::<u64>("nmiter")? {
        params.nmiter = validate::nmiter(v).map_err(|e| e.to_string())?;
    }
    if let Some(v) = row.float("mgain")? {
        params.mgain = v;
    }
    if let Some(v) = row.integer::<i64>("auto_mask")? {
        params.auto_mask = v;
    }
    if let Some(v) = row.float("uvw_min")? {
        params.uvw_min = v;
    }
    if let Some(v) = row.float("uvw_max")? {
        params.uvw_max = Some(v);
    }
    if let Some(v) = row.boolean("join_polarizations")? {
        params.join_polarizations = v;
    }
    if let Some(v) = row.boolean("apply_primary_beam")? {
        params.apply_primary_beam = v;
    }
    if let Some(v) = row.boolean("multiscale")? {
        params.multiscale = v;
    }
    if let Some(v) = row.integer::<i64>("channels_out")? {
        params.channels_out = v;
    }
    if let Some(v) = row.float("clean_threshold")? {
        params.clean_threshold = Some(v);
    }
    if let Some(v) = row.float("auto_threshold")? {
        params.auto_threshold = v;
    }
    if let Some(v) = row.integer::<i64>("nwlayers")? {
        params.nwlayers = Some(v);
    }
    if let Some(v) = row.named::<OutputMode>("output_mode")? {
        params.output_mode = v;
    }
    apply_resubmit!(row, defaults, params);
    std::mem::take(row).finish("imaging")?;
    Ok(params)
}

fn parse_download(
    row: &mut Row,
    obs_id: ObsId,
    defaults: &CsvDefaults,
) -> Result<DownloadJobParams, String> {
    let download_type = required(row.text("download_type"), "download_type")?;
    // The format's names are `vis` and `vis_meta`; the schema's are `vis`
    // and `meta`.
    let download_type = match download_type.to_lowercase().as_str() {
        DOWNLOAD_TYPE_VIS => DownloadType::Vis,
        DOWNLOAD_TYPE_META => DownloadType::Meta,
        _ => {
            return Err(format!(
                "download_type: `{download_type}` is not one of {DOWNLOAD_TYPE_VIS}, \
                 {DOWNLOAD_TYPE_META}"
            ))
        }
    };
    let mut params = download_params(obs_id, download_type);
    apply_delivery!(row, defaults, params);
    apply_resubmit!(row, defaults, params);
    std::mem::take(row).finish("download")?;
    Ok(params)
}

fn parse_beamformer(
    row: &mut Row,
    obs_id: ObsId,
    defaults: &CsvDefaults,
) -> Result<BeamformerJobParams, String> {
    let mut params = beamformer_params(obs_id);
    apply_delivery!(row, defaults, params);
    apply_resubmit!(row, defaults, params);
    std::mem::take(row).finish("beamformer")?;
    Ok(params)
}

fn parse_voltage(
    row: &mut Row,
    obs_id: ObsId,
    defaults: &CsvDefaults,
) -> Result<VoltageJobParams, String> {
    let offset = required(row.integer::<i64>("offset")?, "offset")?;
    let duration = required(row.integer::<u64>("duration")?, "duration")?;
    let mut params = voltage_params(obs_id, offset, duration);
    // The delivery of a voltage job is text in the schema, and the only
    // valid value is `scratch`: the server says so. Any other text is
    // sent as it is.
    if let Some(delivery) = row
        .text("delivery")
        .or_else(|| defaults.delivery.map(|d| d.to_string()))
    {
        params.delivery = delivery;
    }
    params.from_channel = row.integer::<u8>("from_channel")?;
    params.to_channel = row.integer::<u8>("to_channel")?;
    apply_resubmit!(row, defaults, params);
    let channel_range = row.boolean("channel_range")?;
    set_channel_range(&mut params);
    // `channel_range` is derived from the channels. A file that gives it
    // must give the value that the channels make.
    if let Some(given) = channel_range {
        let derived = params.channel_range.unwrap_or(false);
        if given != derived {
            return Err(format!(
                "channel_range={given} does not agree with from_channel and to_channel \
                 (they make it {derived})"
            ));
        }
    }
    std::mem::take(row).finish("voltage")?;
    Ok(params)
}

impl AsvoClient {
    /// Submit the job of a line of a `manta-ray-client` CSV file (see
    /// [`parse_manta_ray_csv`]), with the submit method of its job type.
    ///
    /// # Errors
    ///
    /// The error of the submit method of the job type.
    pub fn submit_csv_job(
        &self,
        params: &CsvJobParams,
    ) -> Result<JobSubmittedResponse, AsvoApiError> {
        match params {
            CsvJobParams::Conversion(p) => self.submit_conversion_job(p),
            CsvJobParams::Imaging(p) => self.submit_imaging_job(p),
            CsvJobParams::Download(p) if p.download_type == DownloadType::Meta => {
                self.submit_download_meta_job(p)
            }
            CsvJobParams::Download(p) => self.submit_download_vis_job(p),
            CsvJobParams::Voltage(p) => self.submit_voltage_job(p),
            CsvJobParams::Beamformer(p) => self.submit_beamformer_job(p),
        }
    }
}
