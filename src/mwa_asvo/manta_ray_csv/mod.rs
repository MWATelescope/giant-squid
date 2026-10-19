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
//!   the server ignores. A typo cannot change a job unseen. The keys of a
//!   job type are the ones in the README of `manta-ray-client` and the other
//!   fields of the request body of the MWA ASVO OpenAPI schema.
//! - A key that a line gives twice, and a line that makes the same job as an
//!   earlier line, are logged as warnings. As in `manta-ray-client`, the last
//!   value of a key is used.
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

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use log::warn;
use serde::Serialize;
use thiserror::Error;

use crate::helpers::BYTE_ORDER_MARK;
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
    BeamformerJobParams, BeamformerJobParamsMode, Centre, ConversionJobParams, Delivery,
    DeliveryFormat, DownloadJobParams, DownloadType, ImagingJobFlow1Params, JobSubmittedResponse,
    Output, OutputMode, Polarization, VoltageJobParams, Weighting,
};
use crate::mwa_asvo::api::schema_enums::{sanitize_identifier, SchemaEnum};
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
/// A key that a line gives twice (the last value is used), and a line that
/// makes the same job as an earlier line, are logged as warnings.
///
/// # Errors
///
/// [`CsvError`] with every line that has a problem.
pub fn parse_manta_ray_csv(text: &str, defaults: &CsvDefaults) -> Result<Vec<CsvJob>, CsvError> {
    let (jobs, warnings) = parse_with_warnings(text, defaults)?;
    for warning in &warnings {
        warn!("{warning}");
    }
    Ok(jobs)
}

/// What is not wrong with a line, but is probably not what the writer of
/// the file wants.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("line {line}: {message}")]
struct CsvRowWarning {
    /// The line of the file, from 1.
    line: usize,
    /// What is odd, in words.
    message: String,
}

/// As [`parse_manta_ray_csv`], but the warnings are returned, not logged.
fn parse_with_warnings(
    text: &str,
    defaults: &CsvDefaults,
) -> Result<(Vec<CsvJob>, Vec<CsvRowWarning>), CsvError> {
    let text = text.strip_prefix(BYTE_ORDER_MARK).unwrap_or(text);
    let mut jobs: Vec<CsvJob> = Vec::new();
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    // The line of the first job with each request body.
    let mut first_line_of_body: HashMap<String, usize> = HashMap::new();

    for (index, line_text) in text.lines().enumerate() {
        let line = index + 1;
        let trimmed = line_text.trim();
        if trimmed.is_empty() || trimmed.starts_with(COMMENT_CHAR) {
            continue;
        }
        match parse_line(trimmed, defaults) {
            Ok((obs_id, params, line_warnings)) => {
                warnings.extend(
                    line_warnings
                        .into_iter()
                        .map(|message| CsvRowWarning { line, message }),
                );
                // The body is JSON that the schema types make, so equal
                // bodies are equal texts. A body that cannot be made into
                // text (it cannot happen) is not compared: it must not be
                // reported as equal to another one.
                if let Ok(body) = serde_json::to_string(&params) {
                    if let Some(first) = first_line_of_body.get(&body) {
                        warnings.push(CsvRowWarning {
                            line,
                            message: format!("this line makes the same job as line {first}"),
                        });
                    } else {
                        first_line_of_body.insert(body, line);
                    }
                }
                jobs.push(CsvJob {
                    line,
                    obs_id,
                    params,
                });
            }
            Err(message) => errors.push(CsvRowError { line, message }),
        }
    }

    if errors.is_empty() {
        Ok((jobs, warnings))
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
    /// What is odd in the line (a key that it gives twice).
    warnings: Vec<String>,
}

impl Row {
    /// Split a line into its cells.
    fn parse(line: &str) -> Result<Self, String> {
        let mut cells = BTreeMap::new();
        let mut warnings = Vec::new();
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
                warnings.push(format!(
                    "the key `{key}` is given twice; the last value is used"
                ));
            }
        }
        Ok(Self {
            cells,
            known: Vec::new(),
            warnings,
        })
    }

    /// The text of the cell `key`, if the line has it.
    fn text(&mut self, key: &'static str) -> Option<String> {
        self.known.push(key);
        self.cells.remove(key)
    }

    /// The cell `key` as a number with a fraction.
    fn float(&mut self, key: &'static str) -> Result<Option<f64>, String> {
        self.text(key)
            .map(|value| {
                value
                    .parse::<f64>()
                    .map_err(|_| format!("{key}: expected a number, got `{value}`"))
            })
            .transpose()
    }

    /// The cell `key` as a whole number of type `T`. A value that is a whole
    /// number but not one of `T` is reported with the range of `T`.
    fn integer<T: WholeNumber>(&mut self, key: &'static str) -> Result<Option<T>, String> {
        self.text(key)
            .map(|value| {
                value.parse::<T>().map_err(|_| {
                    if value.parse::<i128>().is_ok() {
                        format!(
                            "{key}: expected a whole number from {}, got `{value}`",
                            T::range()
                        )
                    } else {
                        format!("{key}: expected a whole number, got `{value}`")
                    }
                })
            })
            .transpose()
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

    /// An error for the keys that the job type does not have, if the line has
    /// any. Call it when the job type has asked for all of its keys.
    fn finish(self, job: &str) -> Result<(), String> {
        if self.cells.is_empty() {
            return Ok(());
        }
        let unknown: Vec<String> = self.cells.keys().map(|key| format!("`{key}`")).collect();
        let noun = if unknown.len() == 1 { "key" } else { "keys" };
        let mut known = self.known;
        known.sort_unstable();
        known.dedup();
        Err(format!(
            "unknown {noun} {} for a {job} job (the keys of this job are {})",
            unknown.join(", "),
            known.join(", ")
        ))
    }
}

/// A whole number type of a request body, for the message about a value
/// that is out of its range.
trait WholeNumber: FromStr {
    /// The range of the type, in words: for example `0 to 255`.
    fn range() -> String;
}

/// Implement [`WholeNumber`] for the integer types in the list.
macro_rules! impl_whole_number {
    ($($type:ty),+) => {
        $(
            impl WholeNumber for $type {
                fn range() -> String {
                    format!("{} to {}", <$type>::MIN, <$type>::MAX)
                }
            }
        )+
    };
}

impl_whole_number!(u8, u64, i64);

/// A value that the job type requires.
fn required<T>(value: Option<T>, key: &str) -> Result<T, String> {
    value.ok_or_else(|| format!("{key} is required"))
}

/// Read the job of one line.
fn parse_line(
    line: &str,
    defaults: &CsvDefaults,
) -> Result<(ObsId, CsvJobParams, Vec<String>), String> {
    let mut row = Row::parse(line)?;
    let warnings = std::mem::take(&mut row.warnings);
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
    Ok((obs_id, params, warnings))
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
        if let Some(v) = $row.boolean("no_apply_amps")? {
            $params.no_apply_amps = v;
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
/// for conversion jobs. An imaging job has them too.
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
    apply_birli_flags!(row, params);
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
    if let Some(v) = row.integer::<i64>("wstack_nwlayers")? {
        params.wstack_nwlayers = Some(v);
    }
    if let Some(v) = row.float("abs_threshold")? {
        params.abs_threshold = Some(v);
    }
    if let Some(v) = row.boolean("join_channels")? {
        params.join_channels = v;
    }
    if let Some(v) = row.named::<Polarization>("pol")? {
        params.pol = v;
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
    if let Some(mode) = row.text("mode") {
        params.mode = Some(parse_beamformer_mode(&mode)?);
    }
    apply_resubmit!(row, defaults, params);
    std::mem::take(row).finish("beamformer")?;
    Ok(params)
}

/// The `mode` of a beamformer job: the case, spaces, hyphens and underscores
/// do not matter, as for the other named values.
fn parse_beamformer_mode(text: &str) -> Result<BeamformerJobParamsMode, String> {
    let modes = [
        BeamformerJobParamsMode::MwaxBeamformer,
        BeamformerJobParamsMode::MwaxCorrBf,
    ];
    let wanted = sanitize_identifier(text);
    modes
        .into_iter()
        .find(|mode| sanitize_identifier(&mode.to_string()) == wanted)
        .ok_or_else(|| {
            let names: Vec<String> = modes.iter().map(ToString::to_string).collect();
            format!("mode: `{text}` is not one of {}", names.join(", "))
        })
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

/// A job of a file that was not submitted.
#[derive(Debug)]
pub struct CsvJobFailure {
    /// The line of the file, from 1.
    pub line: usize,
    /// The Obs ID of the job.
    pub obs_id: ObsId,
    /// Why the job was not submitted.
    pub error: AsvoApiError,
}

impl std::fmt::Display for CsvJobFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "line {}: Obs ID {}: {}",
            self.line, self.obs_id, self.error
        )
    }
}

/// Why [`AsvoClient::submit_csv_file`] or [`AsvoClient::submit_csv_jobs`]
/// did not submit every job of a file.
#[derive(Debug, Error)]
pub enum CsvSubmitError {
    /// The file could not be read. Nothing was submitted.
    #[error("Could not read the CSV file {}: {source}", path.display())]
    Read {
        /// The file.
        path: PathBuf,
        /// Why it could not be read.
        #[source]
        source: std::io::Error,
    },
    /// A line of the file has a problem. Nothing was submitted.
    #[error("{0}")]
    Invalid(#[from] CsvError),
    /// The server refused some jobs, or a request failed. The other jobs
    /// were submitted: [`AsvoClient::submit_csv_jobs`] carries on past a
    /// failure.
    #[error("{}", failed_message(.submitted, .failures))]
    Failed {
        /// The replies for the jobs that were submitted, in the order of the
        /// file.
        submitted: Vec<JobSubmittedResponse>,
        /// The jobs that were not, in the order of the file.
        failures: Vec<CsvJobFailure>,
    },
}

/// The message of [`CsvSubmitError::Failed`]: the counts, and each failure.
fn failed_message(submitted: &[JobSubmittedResponse], failures: &[CsvJobFailure]) -> String {
    let total = submitted.len() + failures.len();
    let mut message = format!("{} of {total} job submissions failed", failures.len());
    for failure in failures {
        message.push_str(&format!("\n  {failure}"));
    }
    message
}

impl AsvoClient {
    /// Read a `manta-ray-client` CSV file (see [`parse_manta_ray_csv`]),
    /// check all of it, and submit its jobs, in the order of the file.
    ///
    /// Nothing is submitted if the file cannot be read or has a problem on
    /// any line. A job that fails (the server refuses it, or a request
    /// fails) does not stop the others.
    ///
    /// Returns the reply for each job, in the order of the file. A file with
    /// no job gives an empty list.
    ///
    /// # Errors
    ///
    /// [`CsvSubmitError::Read`] or [`CsvSubmitError::Invalid`], with nothing
    /// submitted; [`CsvSubmitError::Failed`] if any job failed, with the
    /// replies for the jobs that were submitted.
    pub fn submit_csv_file(
        &self,
        path: &Path,
        defaults: &CsvDefaults,
    ) -> Result<Vec<JobSubmittedResponse>, CsvSubmitError> {
        let text = std::fs::read_to_string(path).map_err(|source| CsvSubmitError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let jobs = parse_manta_ray_csv(&text, defaults)?;
        self.submit_csv_jobs(&jobs, |_, _| {})
    }

    /// Submit the jobs that [`parse_manta_ray_csv`] read, in order, and carry
    /// on past a job that fails. `on_result` is called for each job when its
    /// request is done, so that a caller can report it at once.
    ///
    /// # Errors
    ///
    /// [`CsvSubmitError::Failed`] if any job failed, with the replies for the
    /// jobs that were submitted. No other error is returned.
    pub fn submit_csv_jobs(
        &self,
        jobs: &[CsvJob],
        mut on_result: impl FnMut(&CsvJob, &Result<JobSubmittedResponse, AsvoApiError>),
    ) -> Result<Vec<JobSubmittedResponse>, CsvSubmitError> {
        let mut submitted = Vec::with_capacity(jobs.len());
        let mut failures = Vec::new();
        for job in jobs {
            let result = self.submit_csv_job(&job.params);
            on_result(job, &result);
            match result {
                Ok(response) => submitted.push(response),
                Err(error) => failures.push(CsvJobFailure {
                    line: job.line,
                    obs_id: job.obs_id,
                    error,
                }),
            }
        }
        if failures.is_empty() {
            Ok(submitted)
        } else {
            Err(CsvSubmitError::Failed {
                submitted,
                failures,
            })
        }
    }

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
