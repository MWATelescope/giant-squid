// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Code to interface with the MWA ASVO.

mod asvo_serde;
mod error;
mod types;

use asvo_serde::{parse_asvo_json, AsvoSubmitJobResponse};
pub use error::AsvoError;
pub use types::{AsvoJob, AsvoJobID, AsvoJobMap, AsvoJobState, AsvoJobType, AsvoJobVec, Delivery};

use std::collections::BTreeMap;
use std::env::{current_dir, var};
use std::fs::{rename, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::time::Instant;

use backoff::{retry, Error, ExponentialBackoff};
use log::{debug, info};
use reqwest::blocking::{Client, ClientBuilder};
use sha1::{Digest, Sha1};
use tar::Archive;

use crate::obsid::Obsid;

use self::types::AsvoFilesArray;

pub fn get_asvo_server_address() -> String {
    format!(
        "https://{}",
        std::env::var("MWA_ASVO_HOST").unwrap_or_else(|_| String::from("asvo.mwatelescope.org:443"))
    )
}

lazy_static::lazy_static! {
    /// Default parameters for conversion jobs. Generate a measurement set with
    /// 4s time integration, 40kHz frequency channels, flag 160kHz from the
    /// edges of each coarse band, allow missing gpubox files and flag the
    /// centre channel of each coarse band.
    pub static ref DEFAULT_CONVERSION_PARAMETERS: BTreeMap<&'static str, &'static str> = {
        let mut m = BTreeMap::new();
        m.insert("output",  "uvfits");
        m.insert("avg_freq_res",    "80");
        m.insert("flag_edge_width", "80");
        m
    };
}

pub struct AsvoClient {
    /// The `reqwest` [Client] used to interface with the ASVO web service.
    client: Client,
}

impl AsvoClient {
    /// Get a new reqwest [Client] which has authenticated with the MWA ASVO.
    /// Uses the `MWA_ASVO_API_KEY` environment variable for login.
    pub fn new() -> Result<AsvoClient, AsvoError> {
        let api_key = var("MWA_ASVO_API_KEY").map_err(|_| AsvoError::MissingAuthKey)?;

        // Interfacing with the ASVO server requires specifying the client
        // version. As this is not the manta-ray-client, we need to lie here.
        // Use a user-specified value if available, or the hard-coded one here.
        let client_version =
            var("MWA_ASVO_VERSION").unwrap_or_else(|_| "mantaray-clientv1.2".to_string());
        // Connect and return the cookie jar.
        debug!("Connecting to ASVO...");
        let client = ClientBuilder::new()
            .cookie_store(true)
            .connection_verbose(true)
            .danger_accept_invalid_certs(true) // Required for the ASVO.
            .build()?;
        let response = client
            .post(format!("{}/api/api_login", get_asvo_server_address()))
            .basic_auth(client_version, Some(&api_key))
            .send()?;
        if response.status().is_success() {
            debug!("Successfully authenticated with ASVO");
            Ok(AsvoClient { client })
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
            .client
            .get(format!("{}/api/get_jobs", get_asvo_server_address()))
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
        keep_tar: bool,
        hash: bool,
        download_dir: &str
    ) -> Result<(), AsvoError> {
        let mut jobs = self.get_jobs()?;
        debug!("Attempting to download job {}", jobid);
        // Filter all jobs but the one we're interested in.
        jobs.0.retain(|j| j.jobid == jobid);
        match jobs.0.len() {
            0 => Err(AsvoError::NoAsvoJob(jobid)),
            1 => self.download(&jobs.0[0], keep_tar, hash, download_dir),
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
        keep_tar: bool,
        hash: bool,
        download_dir: &str
    ) -> Result<(), AsvoError> {
        let mut jobs = self.get_jobs()?;
        debug!("Attempting to download obsid {}", obsid);
        // Filter all ASVO jobs by obsid. If we don't have exactly one match, we
        // have to bug out.
        jobs.0.retain(|j| j.obsid == obsid);
        match jobs.0.len() {
            0 => Err(AsvoError::NoObsid(obsid)),
            1 => self.download(&jobs.0[0], keep_tar, hash, download_dir),
            _ => Err(AsvoError::TooManyObsids(obsid)),
        }
    }

    /// Private function to actually do the work.
    fn download(&self, job: &AsvoJob, keep_tar: bool, hash: bool, download_dir: &str) -> Result<(), AsvoError> {
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

        let total_bytes = files.iter().map(|f| f.size).sum();
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
            match f.r#type {
                Delivery::Acacia => match f.url.as_deref() {
                    Some(url) => {
                        debug!("Downloading file {:?}", &url);

                        let op = || {
                            self.try_download(url, keep_tar, hash, f, job, download_dir)
                                .map_err(|e| match &e {
                                    &AsvoError::IO(_) => Error::permanent(e),
                                    _ => Error::transient(e),
                                })
                        };

                        if let Err(Error::Permanent(err)) = retry(ExponentialBackoff::default(), op)
                        {
                            return Err(err);
                        }

                        info!(
                            "Completed download in {} (average rate: {}/s)",
                            if start_time.elapsed().as_secs() > 60 {
                                format!(
                                    "{}min{:.2}s",
                                    start_time.elapsed().as_secs() / 60,
                                    (start_time.elapsed().as_millis() as f64 / 1e3) % 60.0
                                )
                            } else {
                                format!("{}s", start_time.elapsed().as_millis() as f64 / 1e3)
                            },
                            bytesize::ByteSize(
                                (total_bytes as u128 * 1000 / start_time.elapsed().as_millis())
                                    as u64
                            )
                            .to_string_as(true)
                        );
                    }
                    None => return Err(AsvoError::NoUrl { job_id: job.jobid }),
                },
                Delivery::Scratch => {
                    match &f.path {
                        Some(path) => {
                            //If it's an /astro or /scratch job, and the files are reachable from the current host, move them into the current working directory
                            let path_obj = Path::new(&path);
                            let folder_name = path_obj
                                .components()
                                .last()
                                .unwrap()
                                .as_os_str()
                                .to_str()
                                .unwrap();

                            if !Path::exists(path_obj) {
                                info!(
                                    "Files for Job {} are not reachable from the current host.",
                                    job.jobid
                                );
                            } else {
                                info!("Files for Job {} are reachable from the current host. Copying to current directory.", job.jobid);

                                let mut current_path = current_dir()?;
                                current_path.push(folder_name);
                                rename(path, current_path)?;
                            }
                        }
                        None => return Err(AsvoError::NoPath { job_id: job.jobid }),
                    }
                }
            }
        }

        Ok(())
    }

    pub fn try_download(
        &self,
        url: &str,
        keep_tar: bool,
        hash: bool,
        f: &AsvoFilesArray,
        job: &AsvoJob,
        download_dir: &str
    ) -> Result<(), AsvoError> {
        // How big should our in-memory download buffer be [MiB]?
        let buffer_size = match var("GIANT_SQUID_BUF_SIZE") {
            Ok(s) => s.parse()?,
            Err(_) => 100, // 100 MiB by default.
        } * 1024
            * 1024;

        // parse out path from url
        let url_obj = reqwest::Url::parse(url).unwrap();
        let out_path = Path::new(url_obj.path_segments().unwrap().last().unwrap());

        let response = self.client.get(url).send()?;

        let mut tee = tee_readwrite::TeeReader::new(response, Sha1::new(), false);

        if keep_tar {
            // Simply dump the response to the appropriate file name. Use a
            // buffer to avoid doing frequent writes.

            info!("Writing archive to {:?}", out_path);

            let mut out_file = File::create(out_path)?;
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
            // Stream-untar the response.
            let unpack_path = Path::new(download_dir);
            info!("Untarring to {:?}", unpack_path);
            let mut tar = Archive::new(&mut tee);
            tar.set_preserve_mtime(false);            
            tar.unpack(unpack_path)?;
        }

        // If we were told to hash the download, compare our hash against
        // the upstream hash. Stream untarring may not read all of the
        // bytes; read the tee to the end.
        {
            let mut final_bytes = vec![];
            tee.read_to_end(&mut final_bytes)?;
        }

        if hash {
            match &f.sha1 {
                Some(sha) => {
                    debug!("Upstream hash: {}", sha);
                    let (_, hasher) = tee.into_inner();
                    let hash = format!("{:x}", hasher.finalize());
                    debug!("Our hash: {}", &hash);
                    if !hash.eq_ignore_ascii_case(sha) {
                        return Err(AsvoError::HashMismatch {
                            jobid: job.jobid,
                            file: url.to_string(),
                            calculated_hash: hash,
                            expected_hash: sha.to_string(),
                        });
                    }
                }
                _ => {
                    panic!("Product does not include a hash to compare.")
                }
            }
        }

        Ok(())
    }

    /// Submit an ASVO job for visibility download.
    pub fn submit_vis(&self, obsid: Obsid, delivery: Delivery, allow_resubmit: bool) -> Result<AsvoJobID, AsvoError> {
        debug!("Submitting a vis job to ASVO");

        let obsid_str = format!("{}", obsid);
        let d_str = format!("{}", delivery);
        let allow_resubmit_str: String = format!("{}", allow_resubmit);

        let mut form = BTreeMap::new();
        form.insert("obs_id", obsid_str.as_str());
        form.insert("delivery", &d_str);
        form.insert("download_type", "vis");
        form.insert("allow_resubmit", &allow_resubmit_str);
        self.submit_asvo_job(&AsvoJobType::DownloadVisibilities, form)
    }

    /// Submit an ASVO job for voltage download.
    pub fn submit_volt(
        &self,
        obsid: Obsid,
        delivery: Delivery,
        offset: i32,
        duration: i32,                
        allow_resubmit: bool,

    ) -> Result<AsvoJobID, AsvoError> {
        debug!("Submitting a voltage job to ASVO");

        let obsid_str = format!("{}", obsid);
        let d_str = format!("{}", delivery);
        let offset_str: String = format!("{}", offset);
        let duration_str: String = format!("{}", duration);
        let allow_resubmit_str: String = format!("{}", allow_resubmit);        

        let mut form = BTreeMap::new();
        form.insert("obs_id", obsid_str.as_str());
        form.insert("delivery", &d_str);
        form.insert("offset", &offset_str);
        form.insert("duration", &duration_str);
        form.insert("download_type", "volt");
        form.insert("allow_resubmit", &allow_resubmit_str);        
        self.submit_asvo_job(&AsvoJobType::DownloadVoltage, form)
    }

    /// Submit an ASVO job for conversion.
    pub fn submit_conv(
        &self,
        obsid: Obsid,
        delivery: Delivery,
        parameters: &BTreeMap<&str, &str>,
        allow_resubmit: bool,
    ) -> Result<AsvoJobID, AsvoError> {
        debug!("Submitting a conversion job to ASVO");

        let obsid_str = format!("{}", obsid);
        let d_str = format!("{}", delivery);
        let allow_resubmit_str: String = format!("{}", allow_resubmit);        

        let mut form = BTreeMap::new();
        form.insert("obs_id", obsid_str.as_str());
        for (&k, &v) in DEFAULT_CONVERSION_PARAMETERS.iter() {
            form.insert(k, v);
        }

        // Add the user's conversion parameters. If the user has specified an
        // option that is in common with the defaults, then it overrides the
        // default.
        for (&k, &v) in parameters.iter() {
            form.insert(k, v);
        }
        // Insert the CLI delivery last. This ensures that if the user
        // incorrectly specified it as part of the `parameters`, it is ignored.
        form.insert("delivery", &d_str);
        form.insert("allow_resubmit", &allow_resubmit_str);

        self.submit_asvo_job(&AsvoJobType::Conversion, form)
    }

    /// Submit an ASVO job for metadata download.
    pub fn submit_meta(&self, obsid: Obsid, delivery: Delivery, allow_resubmit: bool) -> Result<AsvoJobID, AsvoError> {
        debug!("Submitting a metafits job to ASVO");

        let obsid_str = format!("{}", obsid);
        let d_str = format!("{}", delivery);
        let allow_resubmit_str: String = format!("{}", allow_resubmit);
        
        let mut form = BTreeMap::new();
        form.insert("obs_id", obsid_str.as_str());
        form.insert("delivery", &d_str);
        form.insert("download_type", "vis_meta");
        form.insert("allow_resubmit", &allow_resubmit_str);
        self.submit_asvo_job(&AsvoJobType::DownloadMetadata, form)
    }

    /// This low-level function actually submits jobs to the ASVO.
    fn submit_asvo_job(
        &self,
        job_type: &AsvoJobType,
        form: BTreeMap<&str, &str>,
    ) -> Result<AsvoJobID, AsvoError> {
        debug!("Submitting an ASVO job");
        let api_path = match job_type {
            AsvoJobType::Conversion => "conversion_job",
            AsvoJobType::DownloadVisibilities | AsvoJobType::DownloadMetadata => "download_vis_job",
            AsvoJobType::DownloadVoltage => "voltage_job",
            jt => return Err(AsvoError::UnsupportedType(jt.clone())),
        };

        // Send a POST request to the ASVO.
        let response = self
            .client
            .post(format!("{}/api/{}", get_asvo_server_address(), api_path))
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

            Ok(AsvoSubmitJobResponse::ErrorWithCode { error_code, error }) => {
                Err(AsvoError::BadRequest {
                    code: error_code,
                    message: error,
                })
            }

            Ok(AsvoSubmitJobResponse::GenericError { error }) => match error.as_str() {
                // If the server comes back with the error "already queued,
                // processing or complete", proceed like it wasn't an error.
                "Job already queued, processing or complete." => {
                    let jobs = self.get_jobs()?;
                    // This approach is flawed; the first job ID with the
                    // same obsid as that submitted by this function is
                    // returned, but it's not necessarily the right job ID.
                    let j = jobs
                        .0
                        .iter()
                        .find(|j| j.obsid == form["obs_id"].parse().unwrap())
                        .unwrap();
                    Ok(j.jobid)
                }

                _ => Err(AsvoError::BadRequest {
                    code: 666,
                    message: error,
                }),
            },

            Err(e) => {
                debug!("bad response: {}", response_text);
                Err(AsvoError::BadJson(e))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::AsvoError;
    use crate::Delivery;
    use crate::{AsvoClient, Obsid};

    #[test]
    fn test_create_asvo_client() {
        let client = AsvoClient::new();
        assert!(client.is_ok());
    }

    #[test]
    fn test_get_jobs() {
        let client = AsvoClient::new();
        let jobs = client.unwrap().get_jobs();
        assert!(jobs.is_ok());
    }

    #[test]
    fn test_submit_download() {
        let client = AsvoClient::new().unwrap();
        let obs_id = Obsid::validate(1343457784).unwrap();
        let delivery = Delivery::Acacia;
        let allow_resubmit: bool=false;

        let vis_job = client.submit_vis(obs_id, delivery, allow_resubmit);
        match vis_job {
            Ok(_) => (),
            Err(error) => match error {
                AsvoError::BadStatus {
                    code: _,
                    message: _,
                } => (),
                _ => panic!("Unexpected error has occured."),
            },
        }

        let meta_job = client.submit_meta(obs_id, delivery, true);
        match meta_job {
            Ok(_) => (),
            Err(error) => match error {
                AsvoError::BadStatus {
                    code: _,
                    message: _,
                } => (),
                _ => panic!("Unexpected error has occured."),
            },
        }
    }

    #[test]
    fn test_submit_conv() {
        let client = AsvoClient::new().unwrap();
        let obs_id = Obsid::validate(1343457784).unwrap();
        let delivery = Delivery::Acacia;
        let job_params = BTreeMap::new();
        let allow_resubmit: bool = false;

        let conv_job = client.submit_conv(obs_id, delivery, &job_params, allow_resubmit);
        match conv_job {
            Ok(_) => (),
            Err(error) => match error {
                AsvoError::BadStatus { code, message: _ } => println!("Got return code {}", code),
                _ => panic!("Unexpected error has occured."),
            },
        }
    }

    #[test]
    fn test_submit_volt() {
        let client = AsvoClient::new().unwrap();
        // NOTE: this obs_id is a voltage observation, however for this test to pass,
        // You must have your pawsey_group set in your MWA ASVO profile to mwaops or mwavcs (contact an Admin to have this done).
        let obs_id = Obsid::validate(1290094336).unwrap();
        let offset: i32 = 0; // This will attempt to get data from GPS TIME: 1290094336
        let duration: i32 = 1; // This will attempt to get data up to GPS TIME: 1290094336
        let delivery = Delivery::Scratch;
        let allow_resubmit: bool = false;

        let volt_job = client.submit_volt(obs_id, delivery, offset, duration, allow_resubmit);
        match volt_job {
            Ok(_) => (),
            Err(error) => match error {
                AsvoError::BadStatus { code, message: _ } => println!("Got return code {}", code),
                _ => panic!("Unexpected error has occured."),
            },
        }
    }
}
