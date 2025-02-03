// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Code to interface with the MWA ASVO.

mod asvo_serde;
mod error;
mod types;

use asvo_serde::{parse_asvo_json, AsvoSubmitJobResponse};
pub use error::AsvoError;
pub use types::{
    AsvoJob, AsvoJobID, AsvoJobMap, AsvoJobState, AsvoJobType, AsvoJobVec, Delivery, DeliveryFormat,
};

use std::collections::BTreeMap;
use std::env::{current_dir, var, VarError};
use std::fs::{rename, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::time::Instant;

use crate::check_file_sha1_hash;
use crate::obsid::Obsid;
use backoff::{retry, Error, ExponentialBackoff};
use log::{debug, error, info, warn};
use reqwest::blocking::{Client, ClientBuilder};
use reqwest::header::{HeaderMap, HeaderValue, RANGE};
use sha1::{Digest, Sha1};
use tar::Archive;
use tee_readwrite::TeeReader;

use self::types::AsvoFilesArray;

// Returns a custom MWA ASVO host address (via a set env var)
// or returns VarError::NotPresent error when not set
pub fn get_asvo_server_address_env() -> Result<String, VarError> {
    std::env::var("MWA_ASVO_HOST")
}

pub fn get_asvo_server_address() -> String {
    format!(
        "https://{}",
        get_asvo_server_address_env().unwrap_or_else(|_| String::from("asvo.mwatelescope.org:443"))
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
    /// The `reqwest` [Client] used to interface with the MWA ASVO web service.
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
        // IF we are using a custom MWA ASVO host, then
        // upgrade this debug message to a warn message
        let custom_server_result = get_asvo_server_address_env();
        if custom_server_result.is_ok() {
            warn!(
                "Connecting to MWA ASVO non-default host: {}...",
                get_asvo_server_address()
            );
        } else {
            debug!("Connecting to MWA ASVO...");
        }

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
            debug!("Successfully authenticated with MWA ASVO");
            Ok(AsvoClient { client })
        } else {
            Err(AsvoError::BadStatus {
                code: response.status(),
                message: response.text()?,
            })
        }
    }

    pub fn get_jobs(&self) -> Result<AsvoJobVec, AsvoError> {
        debug!("Retrieving job statuses from the MWA ASVO...");
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

    /// Download the specified MWA ASVO job ID.
    pub fn download_jobid(
        &self,
        jobid: AsvoJobID,
        keep_tar: bool,
        hash: bool,
        download_dir: &str,
    ) -> Result<(), AsvoError> {
        let mut jobs = self.get_jobs()?;
        debug!("Attempting to download job {}", jobid);
        // Filter all jobs but the one we're interested in.
        jobs.0.retain(|j| j.jobid == jobid);
        match jobs.0.len() {
            0 => Err(AsvoError::NoAsvoJob(jobid)),
            1 => self.download(&jobs.0[0], keep_tar, hash, download_dir),
            // Hopefully there's never multiples of the same MWA ASVO job ID in a
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
        download_dir: &str,
    ) -> Result<(), AsvoError> {
        let mut jobs = self.get_jobs()?;
        debug!("Attempting to download obsid {}", obsid);
        // Filter all MWA ASVO jobs by obsid. If we don't have exactly one match, we
        // have to bug out.
        jobs.0.retain(|j| j.obsid == obsid);
        match jobs.0.len() {
            0 => Err(AsvoError::NoObsid(obsid)),
            1 => self.download(&jobs.0[0], keep_tar, hash, download_dir),
            _ => Err(AsvoError::TooManyObsids(obsid)),
        }
    }

    /// Private function to actually do the work.
    fn download(
        &self,
        job: &AsvoJob,
        keep_tar: bool,
        hash: bool,
        download_dir: &str,
    ) -> Result<(), AsvoError> {
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
            "Downloading MWA ASVO job ID {} (obsid: {}, type: {}, {})",
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
                            "Completed download {} in {} (average rate: {}/s)",
                            if hash {
                                "and hash verification"
                            } else {
                                "without hash verification"
                            },
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
                            //If it's a /scratch job, and the files are reachable from the current host, move them into the current working directory
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
        file_info: &AsvoFilesArray,
        job: &AsvoJob,
        download_dir: &str,
    ) -> Result<(), AsvoError> {
        // How big should our in-memory download buffer be [MiB]?
        let buffer_size = match var("GIANT_SQUID_BUF_SIZE") {
            Ok(s) => s.parse()?,
            Err(_) => 100, // 100 MiB by default.
        } * 1024
            * 1024;

        // parse out path from url
        let url_obj = reqwest::Url::parse(url).unwrap();
        let out_path =
            Path::new(&download_dir).join(url_obj.path_segments().unwrap().last().unwrap());

        // Get mwa asvo hash
        let mwa_asvo_hash = match &file_info.sha1 {
            Some(h) => h,
            None => panic!("MWA ASVO job {} does not have an Sha1 checksum! Please report this to asvo_support@mwatelescope.org", job.jobid),
        };

        let response: reqwest::blocking::Response;
        let mut tee: TeeReader<reqwest::blocking::Response, _>;

        if keep_tar {
            let mut out_file = if out_path.try_exists()? {
                // File already exists!
                File::options().append(true).open(&out_path)?
            } else {
                File::create(&out_path)?
            };

            // Get the size of the file
            let file_size_bytes: u64 = File::metadata(&out_file)?.len();

            // If the file size matches the expected file size, skip downloading
            // if the hash matches
            if file_size_bytes == file_info.size {
                // Now check the hash
                match check_file_sha1_hash(&out_path, mwa_asvo_hash, job.jobid) {
                    Ok(()) => {
                        // We already have the file and it is the right size and matches
                        // the hash, just get out of here!
                        info!(
                            "File exists, is the correct size and matches the checksum. Skipping file."
                        );
                        return Ok(());
                    }
                    Err(_) => {
                        // Since the checksum didn't match, just truncate the file and start again
                        warn!("File exists and is the correct size, but checksum does not match. Restarting download...");
                        out_file = File::create(&out_path)?
                    }
                }
            }

            // If file_size_bytes != 0 then we are going to try and resume the download
            // from where we left off. If file_size_bytes == 0 then we;ll start from the start!
            let mut headers = HeaderMap::new();
            headers.insert(
                RANGE,
                HeaderValue::from_str(&format!(
                    "Range: bytes={}-{}",
                    file_size_bytes, file_info.size
                ))
                .unwrap(),
            );

            response = self.client.get(url).headers(headers).send()?;
            tee = tee_readwrite::TeeReader::new(response, Sha1::new(), false);

            // Simply dump the response to the appropriate file name. Use a
            // buffer to avoid doing frequent writes.
            if file_size_bytes > 0 {
                info!("Resuming writing archive to {:?}", &out_path);
            } else {
                info!("Writing archive to {:?}", &out_path);
            }

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
            response = self.client.get(url).send()?;
            tee = tee_readwrite::TeeReader::new(response, Sha1::new(), false);

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
            debug!("MWA ASVO hash: {}", mwa_asvo_hash);
            let (_, hasher) = tee.into_inner();
            let hash = format!("{:x}", hasher.finalize());
            debug!("Our hash: {}", &hash);
            if !hash.eq_ignore_ascii_case(mwa_asvo_hash) {
                return Err(AsvoError::HashMismatch {
                    jobid: job.jobid,
                    file: url.to_string(),
                    calculated_hash: hash,
                    expected_hash: mwa_asvo_hash.to_string(),
                });
            }
        }

        Ok(())
    }

    /// Submit an MWA ASVO job for visibility download.
    pub fn submit_vis(
        &self,
        obsid: Obsid,
        delivery: Delivery,
        delivery_format: Option<DeliveryFormat>,
        allow_resubmit: bool,
    ) -> Result<Option<AsvoJobID>, AsvoError> {
        debug!("Submitting a vis job to MWA ASVO");

        let obsid_str = format!("{}", obsid);
        let d_str = format!("{}", delivery);
        let df_str: String;
        let allow_resubmit_str: String = format!("{}", allow_resubmit);

        let mut form = BTreeMap::new();
        form.insert("obs_id", obsid_str.as_str());
        form.insert("delivery", &d_str);

        if delivery_format.is_some() {
            df_str = format!("{}", delivery_format.unwrap());
            form.insert("delivery_format", &df_str);
        }

        form.insert("download_type", "vis");
        form.insert("allow_resubmit", &allow_resubmit_str);
        self.submit_asvo_job(&AsvoJobType::DownloadVisibilities, form)
    }

    /// Submit an ASVO job for voltage download.
    #[allow(clippy::too_many_arguments)]
    pub fn submit_volt(
        &self,
        obsid: Obsid,
        delivery: Delivery,
        offset: i32,
        duration: i32,
        from_channel: Option<i32>,
        to_channel: Option<i32>,
        allow_resubmit: bool,
    ) -> Result<Option<AsvoJobID>, AsvoError> {
        debug!("Submitting a voltage job to MWA ASVO");

        let obsid_str = format!("{}", obsid);
        let d_str = format!("{}", delivery);
        let offset_str: String = format!("{}", offset);
        let duration_str: String = format!("{}", duration);
        let allow_resubmit_str: String = format!("{}", allow_resubmit);
        let channel_range_str: String =
            format!("{}", from_channel.is_some() || to_channel.is_some());
        let from_channel_str: String;
        let to_channel_str: String;

        let mut form = BTreeMap::new();
        form.insert("obs_id", obsid_str.as_str());
        form.insert("delivery", &d_str);
        form.insert("offset", &offset_str);
        form.insert("duration", &duration_str);

        if from_channel.is_some() || to_channel.is_some() {
            form.insert("channel_range", &channel_range_str);
        }

        if from_channel.is_some() {
            from_channel_str = format!("{}", from_channel.unwrap());
            form.insert("from_channel", &from_channel_str);
        }

        if to_channel.is_some() {
            to_channel_str = format!("{}", to_channel.unwrap());
            form.insert("to_channel", &to_channel_str);
        }

        form.insert("download_type", "volt");
        form.insert("allow_resubmit", &allow_resubmit_str);
        self.submit_asvo_job(&AsvoJobType::DownloadVoltage, form)
    }

    /// Submit an MWA ASVO job for conversion.
    pub fn submit_conv(
        &self,
        obsid: Obsid,
        delivery: Delivery,
        delivery_format: Option<DeliveryFormat>,
        parameters: &BTreeMap<&str, &str>,
        allow_resubmit: bool,
    ) -> Result<Option<AsvoJobID>, AsvoError> {
        debug!("Submitting a conversion job to MWA ASVO");

        let obsid_str = format!("{}", obsid);
        let d_str = format!("{}", delivery);
        let df_str: String;
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

        if delivery_format.is_some() {
            df_str = format!("{}", delivery_format.unwrap());
            form.insert("delivery_format", &df_str);
        }

        form.insert("allow_resubmit", &allow_resubmit_str);

        self.submit_asvo_job(&AsvoJobType::Conversion, form)
    }

    /// Submit an MWA ASVO job for metadata download.
    pub fn submit_meta(
        &self,
        obsid: Obsid,
        delivery: Delivery,
        delivery_format: Option<DeliveryFormat>,
        allow_resubmit: bool,
    ) -> Result<Option<AsvoJobID>, AsvoError> {
        debug!("Submitting a metafits job to MWA ASVO");

        let obsid_str = format!("{}", obsid);
        let d_str = format!("{}", delivery);
        let df_str: String;
        let allow_resubmit_str: String = format!("{}", allow_resubmit);

        let mut form = BTreeMap::new();
        form.insert("obs_id", obsid_str.as_str());
        form.insert("delivery", &d_str);

        if delivery_format.is_some() {
            df_str = format!("{}", delivery_format.unwrap());
            form.insert("delivery_format", &df_str);
        }

        form.insert("download_type", "vis_meta");
        form.insert("allow_resubmit", &allow_resubmit_str);
        self.submit_asvo_job(&AsvoJobType::DownloadMetadata, form)
    }

    /// This low-level function actually submits jobs to the MWA ASVO.
    /// The return can either be:
    /// Ok(Some(jobid)) - this is when a new job is submitted
    /// Ok(None) - this is when an existing job is resubmitted
    /// Err() - this is when we hit an error
    fn submit_asvo_job(
        &self,
        job_type: &AsvoJobType,
        form: BTreeMap<&str, &str>,
    ) -> Result<Option<AsvoJobID>, AsvoError> {
        debug!("Submitting an MWA ASVO job");
        let api_path = match job_type {
            AsvoJobType::Conversion => "conversion_job",
            AsvoJobType::DownloadVisibilities | AsvoJobType::DownloadMetadata => "download_vis_job",
            AsvoJobType::DownloadVoltage => "voltage_job",
            jt => return Err(AsvoError::UnsupportedType(jt.clone())),
        };

        // Send a POST request to the MWA ASVO.
        let response = self
            .client
            .post(format!("{}/api/{}", get_asvo_server_address(), api_path))
            .form(&form)
            .send()?;

        let code = response.status().as_u16();
        let response_text = &response.text()?;
        if code != 200 && code < 400 && code > 499 {
            // Show the http code when it's not something we can handle
            warn!("http code: {} response: {}", code, &response_text)
        };
        match serde_json::from_str(response_text) {
            Ok(AsvoSubmitJobResponse::JobIDWithError {
                error,
                error_code,
                job_id,
                ..
            }) => {
                if error_code == 2 {
                    // error code 2 == job already exists
                    warn!("{}. Job Id: {}", error.as_str(), job_id);
                    Ok(None)
                } else {
                    Err(AsvoError::BadRequest {
                        code: error_code,
                        message: error,
                    })
                }
            }

            Ok(AsvoSubmitJobResponse::JobID { job_id, .. }) => Ok(Some(job_id)),

            Ok(AsvoSubmitJobResponse::ErrorWithCode { error_code, error }) => {
                // Crazy code here as MWA ASVO API does not have good error codes (yet!)
                // 0 == invalid input (most of the time!)
                if error_code == 0
                    && (error.as_str()
                        == "Unable to submit job. Observation has no files to download."
                        || (error.as_str().starts_with("Observation ")
                            && error.as_str().ends_with(" does not exist")))
                {
                    error!("{}", error.as_str());
                    Ok(None)
                } else {
                    Err(AsvoError::BadRequest {
                        code: error_code,
                        message: error,
                    })
                }
            }

            Ok(AsvoSubmitJobResponse::GenericError { error }) => Err(AsvoError::BadRequest {
                code: 999,
                message: error,
            }),

            Err(e) => {
                warn!("bad response: {}", response_text);
                Err(AsvoError::BadJson(e))
            }
        }
    }

    /// This low-level function actually cancels a job.
    /// The return can either be:        
    /// Ok(job_id) - this is when an existing job is successfully cancelled
    /// Ok(None) - this is when it failed but it's ok to continue
    /// Err() - this is when we hit an error
    pub fn cancel_asvo_job(&self, job_id: u32) -> Result<Option<u32>, AsvoError> {
        debug!("Cancelling an MWA ASVO job");

        let mut form: BTreeMap<&str, &str> = BTreeMap::new();
        let job_id_str = format!("{}", job_id);
        form.insert("job_id", &job_id_str);

        // Send a GET(?) request to the MWA ASVO.
        // Should be POST!
        let response = self
            .client
            .get(format!(
                "{}/api/{}?job_id={}",
                get_asvo_server_address(),
                "cancel_job",
                job_id
            ))
            .send()?;

        let status_code = response.status();
        let response_text = &response.text()?;
        if status_code == 200 {
            Ok(Some(job_id))
        } else if status_code == 400 {
            // Validation error
            warn!("{}", &response_text);
            Ok(None)
        } else if status_code == 404 {
            // Job id not found
            warn!("Job Id: {} not found", job_id);
            Ok(None)
        } else {
            // Show the http code when it's not something we can handle
            warn!("http code: {} response: {}", status_code, &response_text);
            return Err(AsvoError::BadStatus {
                code: status_code,
                message: response_text.to_string(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::AsvoError;
    use crate::Delivery;
    use crate::DeliveryFormat;
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
    fn test_submit_vis() {
        let client = AsvoClient::new().unwrap();
        let obs_id = Obsid::validate(1343457784).unwrap();
        let delivery = Delivery::Acacia;
        let delivery_format: Option<DeliveryFormat> = None;
        let allow_resubmit: bool = false;

        let vis_job = client.submit_vis(obs_id, delivery, delivery_format, allow_resubmit);
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
    }

    #[test]
    fn test_submit_conv() {
        let client = AsvoClient::new().unwrap();
        let obs_id = Obsid::validate(1343457784).unwrap();
        let delivery = Delivery::Acacia;
        let delivery_format: Option<DeliveryFormat> = None;
        let job_params = BTreeMap::new();
        let allow_resubmit: bool = false;

        let conv_job = client.submit_conv(
            obs_id,
            delivery,
            delivery_format,
            &job_params,
            allow_resubmit,
        );
        match conv_job {
            Ok(_) => (),
            Err(error) => match error {
                AsvoError::BadStatus { code, message: _ } => println!("Got return code {}", code),
                _ => panic!("Unexpected error has occured."),
            },
        }
    }

    #[test]
    fn test_submit_meta() {
        let client = AsvoClient::new().unwrap();
        let obs_id = Obsid::validate(1343457784).unwrap();
        let delivery = Delivery::Acacia;
        let delivery_format: Option<DeliveryFormat> = None;
        let allow_resubmit: bool = false;

        let meta_job = client.submit_meta(obs_id, delivery, delivery_format, allow_resubmit);
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
    fn test_cancel_job_not_found() {
        let job_id = 1234;
        let client = AsvoClient::new().unwrap();
        let cancel_result = client.cancel_asvo_job(job_id);

        assert!(cancel_result.is_ok_and(|j| j.is_none()))
    }

    #[test]
    fn test_cancel_job_successful() {
        let client = AsvoClient::new().unwrap();

        // submit a new job (don't worry we will cancel it right away)
        let obs_id = Obsid::validate(1416257384).unwrap();
        let delivery = Delivery::Acacia;
        let delivery_format: Option<DeliveryFormat> = None;
        let allow_resubmit: bool = false;
        let meta_job = client.submit_vis(obs_id, delivery, delivery_format, allow_resubmit);

        let new_job_id: u32;

        match meta_job {
            Ok(job_id_or_none) => match job_id_or_none {
                Some(j) => new_job_id = j,
                None => panic!("Job submitted, but no jobid returned?"),
            },
            Err(error) => match error {
                AsvoError::BadStatus {
                    code: c,
                    message: m,
                } => panic!("Error has occurred: {} {}", c, m),
                _ => panic!("Unexpected error has occured."),
            },
        }

        let cancel_result = client.cancel_asvo_job(new_job_id);

        assert!(cancel_result.is_ok_and(|j| j.unwrap() == new_job_id))
    }

    #[test]
    fn test_submit_vis_as_tar() {
        let client = AsvoClient::new().unwrap();
        let obs_id = Obsid::validate(1343457784).unwrap();
        let delivery = Delivery::Scratch;
        let delivery_format: Option<DeliveryFormat> = Some(DeliveryFormat::Tar);
        let allow_resubmit: bool = false;

        let vis_job = client.submit_vis(obs_id, delivery, delivery_format, allow_resubmit);
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
    }

    #[test]
    fn test_submit_conv_as_tar() {
        let client = AsvoClient::new().unwrap();
        let obs_id = Obsid::validate(1343457784).unwrap();
        let delivery = Delivery::Scratch;
        let delivery_format: Option<DeliveryFormat> = Some(DeliveryFormat::Tar);
        let job_params = BTreeMap::new();
        let allow_resubmit: bool = false;

        let conv_job = client.submit_conv(
            obs_id,
            delivery,
            delivery_format,
            &job_params,
            allow_resubmit,
        );
        match conv_job {
            Ok(_) => (),
            Err(error) => match error {
                AsvoError::BadStatus { code, message: _ } => println!("Got return code {}", code),
                _ => panic!("Unexpected error has occured."),
            },
        }
    }

    #[test]
    fn test_submit_meta_as_tar() {
        let client = AsvoClient::new().unwrap();
        let obs_id = Obsid::validate(1343457784).unwrap();
        let delivery = Delivery::Scratch;
        let delivery_format: Option<DeliveryFormat> = Some(DeliveryFormat::Tar);
        let allow_resubmit: bool = false;

        let meta_job = client.submit_meta(obs_id, delivery, delivery_format, allow_resubmit);
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
    fn test_submit_volt() {
        let client = AsvoClient::new().unwrap();
        // NOTE: this obs_id is a voltage observation, however for this test to pass,
        // You must have your pawsey_group set in your MWA ASVO profile to mwaops or mwavcs (contact an Admin to have this done).
        let obs_id = Obsid::validate(1290094336).unwrap();
        let offset: i32 = 0; // This will attempt to get data from GPS TIME: 1290094336
        let duration: i32 = 1; // This will attempt to get data up to GPS TIME: 1290094336
        let from_chan: Option<i32> = None;
        let to_chan: Option<i32> = None;
        let delivery = Delivery::Scratch;
        let allow_resubmit: bool = false;

        let volt_job = client.submit_volt(
            obs_id,
            delivery,
            offset,
            duration,
            from_chan,
            to_chan,
            allow_resubmit,
        );
        match volt_job {
            Ok(_) => (),
            Err(error) => match error {
                AsvoError::BadStatus { code, message: _ } => println!("Got return code {}", code),
                _ => panic!("Unexpected error has occured."),
            },
        }
    }

    #[test]
    fn test_submit_volt_range() {
        let client = AsvoClient::new().unwrap();
        // NOTE: this obs_id is a voltage observation, however for this test to pass,
        // You must have your pawsey_group set in your MWA ASVO profile to mwaops or mwavcs (contact an Admin to have this done).
        let obs_id = Obsid::validate(1370760960).unwrap();
        let offset: i32 = 0; // This will attempt to get data from GPS TIME: 1370760960
        let duration: i32 = 8; // This will attempt to get data up to GPS TIME: 1370760968
        let from_chan: Option<i32> = Some(109);
        let to_chan: Option<i32> = Some(109);
        let delivery = Delivery::Scratch;
        let allow_resubmit: bool = false;

        let volt_job = client.submit_volt(
            obs_id,
            delivery,
            offset,
            duration,
            from_chan,
            to_chan,
            allow_resubmit,
        );
        match volt_job {
            Ok(_) => (),
            Err(error) => match error {
                AsvoError::BadStatus { code, message: _ } => println!("Got return code {}", code),
                _ => panic!("Unexpected error has occured."),
            },
        }
    }
}
