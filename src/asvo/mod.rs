// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

/*!
 * Code to interface with the MWA ASVO.
*/

mod asvo_serde;
pub mod error;
pub mod types;

use std::collections::BTreeMap;
use std::env::var;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::time::Instant;

use log::{debug, info};
use reqwest::blocking::{Client, ClientBuilder};
use sha1::{Digest, Sha1};
use zip::read::read_zipfile_from_stream;

use crate::obsid::Obsid;
use asvo_serde::{parse_asvo_json, AsvoSubmitJobResponse};
pub use error::AsvoError;
pub use types::{AsvoJob, AsvoJobID, AsvoJobMap, AsvoJobState, AsvoJobType, AsvoJobVec};

/// The address of the MWA ASVO.
const ASVO_ADDRESS: &str = "https://asvo.mwatelescope.org:8778";

lazy_static::lazy_static! {
    /// Default parameters for conversion jobs. Generate a measurement set with
    /// 4s time integration, 40kHz frequency channels, flag 160kHz from the
    /// edges of each coarse band, allow missing gpubox files and flag the
    /// centre channel of each coarse band.
    pub static ref DEFAULT_CONVERSION_PARAMETERS: BTreeMap<&'static str, &'static str> = {
        let mut m = BTreeMap::new();
        m.insert("download_type" , "conversion");
        m.insert("conversion"    , "ms");
        m.insert("timeres"       , "4");
        m.insert("freqres"       , "40");
        m.insert("edgewidth"     , "160");
        m.insert("allowmissing"  , "true");
        m.insert("flagdcchannels", "true");
        m
    };
}

pub struct AsvoClient(Client);

impl AsvoClient {
    /// Get a new reqwest [Client] which has authenticated with the MWA ASVO.
    /// Uses the `MWA_ASVO_API_KEY` environment variable for login.
    pub fn new() -> Result<Self, AsvoError> {
        let api_key = var("MWA_ASVO_API_KEY").map_err(|_| AsvoError::MissingAuthKey)?;

        // Interfacing with the ASVO server requires specifying the client
        // version. As this is not the manta-ray-client, we need to lie here.
        // Use a user-specified value if available, or the hard-coded one here.
        let client_version =
            var("MWA_ASVO_VERSION").unwrap_or_else(|_| "mantaray-clientv1.0".to_string());
        // Connect and return the cookie jar.
        debug!("Connecting to ASVO...");
        let client = ClientBuilder::new()
            .cookie_store(true)
            .connection_verbose(true)
            .build()?;
        let response = client
            .post(&format!("{}/api/login", ASVO_ADDRESS))
            .basic_auth(&client_version, Some(&api_key))
            .send()?;
        if response.status().is_success() {
            Ok(Self(client))
        } else {
            Err(AsvoError::BadStatus {
                code: response.status(),
                message: response.text()?,
            })
        }
    }

    pub fn get_jobs(&self) -> Result<AsvoJobVec, AsvoError> {
        debug!("Retrieving job statuses from the ASVO...");
        // Send a GET request to the ASVO.
        let response = self
            .0
            .get(&format!("{}/api/get_jobs", ASVO_ADDRESS))
            .send()?;
        if !response.status().is_success() {
            return Err(AsvoError::BadStatus {
                code: response.status(),
                message: response.text()?,
            });
        }

        let body = response.text()?;
        parse_asvo_json(&body).map_err(AsvoError::from)
    }

    /// Download the specified ASVO job ID.
    pub fn download_job(
        &self,
        jobid: AsvoJobID,
        keep_zip: bool,
        hash: bool,
    ) -> Result<(), AsvoError> {
        let mut jobs = self.get_jobs()?;
        debug!("Attempting to download job {}", jobid);
        // Filter all jobs but the one we're interested in.
        jobs.0.retain(|j| j.jobid == jobid);
        match jobs.0.len() {
            0 => Err(AsvoError::NoAsvoJob(jobid)),
            1 => self.download(&jobs.0[0], keep_zip, hash),
            // Hopefully there's never multiples of the same ASVO job ID in a
            // user's job listing...
            _ => unreachable!(),
        }
    }

    /// Download the job associated with an obsid. If more than one job is
    /// associated with the obsid, we must abort, because we don't know which
    /// job to download.
    pub fn download_obsid(
        &self,
        obsid: Obsid,
        keep_zip: bool,
        hash: bool,
    ) -> Result<(), AsvoError> {
        let mut jobs = self.get_jobs()?;
        debug!("Attempting to download obsid {}", obsid);
        // Filter all ASVO jobs by obsid. If we don't have exactly one match, we
        // have to bug out.
        jobs.0.retain(|j| j.obsid == obsid);
        match jobs.0.len() {
            0 => Err(AsvoError::NoObsid(obsid)),
            1 => self.download(&jobs.0[0], keep_zip, hash),
            _ => Err(AsvoError::TooManyObsids(obsid)),
        }
    }

    /// Private function to actually do the work.
    fn download(&self, job: &AsvoJob, keep_zip: bool, hash: bool) -> Result<(), AsvoError> {
        // How big should our in-memory download buffer be [MiB]?
        let buffer_size = match var("GIANT_SQUID_BUF_SIZE") {
            Ok(s) => s.parse()?,
            Err(_) => 100, // 100 MiB by default.
        } * 1024
            * 1024;

        // Is the job ready to download?
        if job.state != AsvoJobState::Ready {
            return Err(AsvoError::NotReady {
                jobid: job.jobid,
                state: job.state.clone(),
            });
        }

        // Handle any silly cases.
        let files = match &job.files {
            None => return Err(AsvoError::NoFiles(job.jobid)),
            Some(f) => {
                if f.is_empty() {
                    return Err(AsvoError::NoFiles(job.jobid));
                }
                f
            }
        };

        let total_bytes = files.iter().map(|f| f.file_size).sum();
        info!(
            "Downloading ASVO job ID {} (obsid: {}, type: {}, {})",
            job.jobid,
            job.obsid,
            job.jtype,
            bytesize::ByteSize(total_bytes).to_string_as(true)
        );
        let start_time = Instant::now();
        // Download each file.
        for f in files {
            debug!("Downloading file {}", f.file_name);
            let response = self
                .0
                .get(&format!("{}/api/download", ASVO_ADDRESS))
                .query(&[
                    ("job_id", format!("{}", job.jobid)),
                    ("file_name", f.file_name.clone()),
                ])
                .send()?;
            let mut tee = tee_readwrite::TeeReader::new(response, Sha1::new(), false);

            if keep_zip {
                // Simply dump the response to the appropriate file name. Use a
                // buffer to avoid doing frequent writes.
                let mut out_file = File::create(&f.file_name)?;
                let mut file_buf = BufReader::with_capacity(buffer_size, tee.by_ref());

                loop {
                    let buffer = file_buf.fill_buf()?;
                    out_file.write_all(buffer)?;

                    let length = buffer.len();
                    file_buf.consume(length);
                    if length == 0 {
                        break;
                    }
                }
            } else {
                // Stream-unzip the response.
                debug!("Attempting to unzip stream");
                while let Ok(Some(z)) = read_zipfile_from_stream(&mut tee) {
                    debug!("Stream unzipping file {}", z.name());
                    let mut out_file = File::create(z.name())?;
                    let mut file_buf = BufReader::with_capacity(buffer_size, z);

                    loop {
                        let buffer = file_buf.fill_buf()?;
                        out_file.write_all(buffer)?;

                        let length = buffer.len();
                        file_buf.consume(length);
                        if length == 0 {
                            break;
                        }
                    }
                }
            }

            // If we were told to hash the download, compare our hash against
            // the upstream hash. Stream unzipping does not read all of the
            // bytes; read the tee to the end.
            {
                let mut final_bytes = vec![];
                tee.read_to_end(&mut final_bytes)?;
            }

            if hash {
                debug!("Upstream hash: {}", &f.sha1);
                let (_, hasher) = tee.into_inner();
                let hash = format!("{:x}", hasher.finalize());
                debug!("Our hash: {}", &hash);
                if !hash.eq_ignore_ascii_case(&f.sha1) {
                    return Err(AsvoError::HashMismatch {
                        jobid: job.jobid,
                        file: f.file_name.clone(),
                        calculated_hash: hash,
                        expected_hash: f.sha1.clone(),
                    });
                }
            }
        }

        let d = Instant::now() - start_time;
        info!(
            "Completed download in {} (average rate: {}/s)",
            if d.as_secs() > 60 {
                format!(
                    "{}min{:.2}s",
                    d.as_secs() / 60,
                    (d.as_millis() as f64 / 1e3) % 60.0
                )
            } else {
                format!("{}s", d.as_millis() as f64 / 1e3)
            },
            bytesize::ByteSize((total_bytes as u128 * 1000 / d.as_millis()) as u64)
                .to_string_as(true)
        );

        Ok(())
    }

    /// Submit an ASVO job for visibility download.
    pub fn submit_vis(&self, obsid: Obsid, expiry_days: u8) -> Result<AsvoJobID, AsvoError> {
        let mut form = BTreeMap::new();
        let obsid_str = format!("{}", obsid);
        form.insert("obs_id", obsid_str.as_str());
        let e_str = format!("{}", expiry_days);
        form.insert("expiry_days", &e_str);
        form.insert("download_type", "vis");
        self.submit_asvo_job(&AsvoJobType::DownloadVisibilities, form)
    }

    /// Submit an ASVO job for conversion.
    pub fn submit_conv(
        &self,
        obsid: Obsid,
        expiry_days: u8,
        parameters: &BTreeMap<&str, &str>,
    ) -> Result<AsvoJobID, AsvoError> {
        let mut form = BTreeMap::new();
        let obsid_str = format!("{}", obsid);
        form.insert("obs_id", obsid_str.as_str());
        let e_str = format!("{}", expiry_days);
        form.insert("expiry_days", &e_str);
        for (&k, &v) in DEFAULT_CONVERSION_PARAMETERS.iter() {
            form.insert(k, v);
        }

        // Add the user's conversion parameters. If the user has specified an
        // option that is in common with the defaults, then it overrides the
        // default.
        for (&k, &v) in parameters.iter() {
            form.insert(k, v);
        }

        self.submit_asvo_job(&AsvoJobType::Conversion, form)
    }

    /// Submit an ASVO job for metadata download.
    pub fn submit_meta(&self, obsid: Obsid, expiry_days: u8) -> Result<AsvoJobID, AsvoError> {
        let mut form = BTreeMap::new();
        let obsid_str = format!("{}", obsid);
        form.insert("obs_id", obsid_str.as_str());
        let e_str = format!("{}", expiry_days);
        form.insert("expiry_days", &e_str);
        form.insert("download_type", "vis_meta");
        self.submit_asvo_job(&AsvoJobType::DownloadMetadata, form)
    }

    /// This low-level function actually submits jobs to the ASVO.
    fn submit_asvo_job(
        &self,
        job_type: &AsvoJobType,
        form: BTreeMap<&str, &str>,
    ) -> Result<AsvoJobID, AsvoError> {
        let api_path = match job_type {
            AsvoJobType::Conversion => "conversion_job",
            AsvoJobType::DownloadVisibilities | AsvoJobType::DownloadMetadata => "download_vis_job",
            jt => return Err(AsvoError::UnsupportedType(jt.clone())),
        };

        // Send a POST request to the ASVO.
        let response = self
            .0
            .post(&format!("{}/api/{}", ASVO_ADDRESS, api_path))
            .form(&form)
            .send()?;
        if !response.status().is_success() {
            return Err(AsvoError::BadStatus {
                code: response.status(),
                message: response.text()?,
            });
        }
        let response_text = response.text()?;
        match serde_json::from_str(&response_text) {
            Ok(AsvoSubmitJobResponse::JobID { job_id, .. }) => Ok(job_id),
            // This shouldn't be reachable, because a non-200 code is issued
            // with it too.
            Ok(AsvoSubmitJobResponse::ErrorWithCode { error_code, error }) => {
                Err(AsvoError::BadRequest {
                    code: error_code,
                    message: error,
                })
            }
            Ok(AsvoSubmitJobResponse::GenericError { error }) => Err(AsvoError::BadRequest {
                code: 666,
                message: error,
            }),
            Err(e) => Err(AsvoError::BadJson(e)),
        }
    }
}
