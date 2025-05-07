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
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use self::types::AsvoFilesArray;
use crate::obsid::Obsid;
use crate::{built_info, check_file_sha1_hash};
use backoff::{retry, Error, ExponentialBackoff};
use indicatif::ProgressBar;
use log::{debug, error, info, warn};
use reqwest::blocking::{Client, ClientBuilder};
use reqwest::header::{HeaderMap, HeaderValue, RANGE};
use sha1::{Digest, Sha1};
use tar::Archive;
use tee_readwrite::TeeReader;

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

#[derive(Debug)]
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
        // version.
        let client_version = format!("giant-squidv{}", built_info::PKG_VERSION);

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
    #[allow(clippy::too_many_arguments)]
    pub fn download_jobid(
        &self,
        jobid: AsvoJobID,
        keep_tar: bool,
        no_resume: bool,
        hash: bool,
        download_dir: &str,
        progress_bar: &ProgressBar,
        download_number: usize,
        download_count: usize,
    ) -> Result<(), AsvoError> {
        let mut jobs = self.get_jobs()?;
        debug!("Attempting to download job {}", jobid);
        // Filter all jobs but the one we're interested in.
        jobs.0.retain(|j| j.jobid == jobid);
        match jobs.0.len() {
            0 => Err(AsvoError::NoAsvoJob(jobid)),
            1 => self.download(
                &jobs.0[0],
                keep_tar,
                no_resume,
                hash,
                download_dir,
                progress_bar,
                download_number,
                download_count,
            ),
            // Hopefully there's never multiples of the same MWA ASVO job ID in a
            // user's job listing...
            _ => unreachable!(),
        }
    }

    /// Download the job associated with an obsid. If more than one job is
    /// associated with the obsid, we must abort, because we don't know which
    /// job to download.
    #[allow(clippy::too_many_arguments)]
    pub fn download_obsid(
        &self,
        obsid: Obsid,
        keep_tar: bool,
        no_resume: bool,
        hash: bool,
        download_dir: &str,
        progress_bar: &ProgressBar,
        download_number: usize,
        download_count: usize,
    ) -> Result<(), AsvoError> {
        let mut all_jobs = self.get_jobs()?;

        debug!("Attempting to download obsid {}", obsid);
        // Filter all MWA ASVO jobs by obsid.
        // If we don't have exactly one match for ready jobs, we
        // have to bug out. Make a clone of all jobs so we can use
        // it below, without needing to go back to the web server
        let mut all_ready_jobs: AsvoJobVec = all_jobs.clone();

        all_ready_jobs
            .0
            .retain(|j| j.obsid == obsid && j.state == AsvoJobState::Ready);
        match all_ready_jobs.0.len() {
            // zero can be- there ar NO jobs with that obsid or zero can be no jobs with that obsid that are ready. We need to distinguish this case!
            0 => {
                all_jobs.0.retain(|j| j.obsid == obsid);
                match all_jobs.0.len() {
                    0 => Err(AsvoError::NoObsid(obsid)),
                    _ => Err(AsvoError::NoJobReadyForObsid(obsid)),
                }
            }
            1 => self.download(
                &all_ready_jobs.0[0],
                keep_tar,
                no_resume,
                hash,
                download_dir,
                progress_bar,
                download_number,
                download_count,
            ),
            _ => Err(AsvoError::TooManyObsids(obsid)),
        }
    }

    /// Private function to actually do the work.
    #[allow(clippy::too_many_arguments)]
    fn download(
        &self,
        job: &AsvoJob,
        keep_tar: bool,
        no_resume: bool,
        hash: bool,
        download_dir: &str,
        progress_bar: &ProgressBar,
        download_number: usize,
        download_count: usize,
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

        let log_prefix = format!(
            "Job ID {} (obsid: {}) [{}/{}]:",
            job.jobid, job.obsid, download_number, download_count
        );

        let start_time = Instant::now();

        // Download each file.
        for f in files {
            match f.r#type {
                Delivery::Acacia => match f.url.as_deref() {
                    Some(url) => {
                        debug!("{} Downloading from url {}", log_prefix, &url);

                        // parse out path from url
                        let url_obj = reqwest::Url::parse(url).unwrap();
                        let out_path = Path::new(&download_dir)
                            .join(url_obj.path_segments().unwrap().next_back().unwrap());

                        let op = || {
                            self.try_download(
                                url,
                                keep_tar,
                                no_resume,
                                hash,
                                f,
                                job,
                                download_dir,
                                &out_path,
                                &log_prefix,
                                progress_bar,
                            )
                            .map_err(|e| match &e {
                                AsvoError::IO(_) => Error::permanent(e),
                                _ => Error::transient(e),
                            })
                        };

                        if let Err(Error::Permanent(err)) = retry(ExponentialBackoff::default(), op)
                        {
                            return Err(err);
                        }

                        info!(
                            "{} Completed download of {} in {} ({}/s)",
                            log_prefix,
                            bytesize::ByteSize(f.size).to_string_as(true),
                            if start_time.elapsed().as_secs() > 60 {
                                format!(
                                    "{} min {:.2} s",
                                    start_time.elapsed().as_secs() / 60,
                                    (start_time.elapsed().as_millis() as f64 / 1e3) % 60.0
                                )
                            } else {
                                format!("{} s", start_time.elapsed().as_millis() as f64 / 1e3)
                            },
                            bytesize::ByteSize(f.size / start_time.elapsed().as_secs())
                                .to_string_as(true)
                        );
                    }
                    None => return Err(AsvoError::NoUrl { job_id: job.jobid }),
                },
                Delivery::Dug => {
                    error!(
                        "{} Files for Job are not reachable from the current host. You will find your job's files on the DUG filesystem.",
                        log_prefix
                    );
                }
                Delivery::Scratch => {
                    match &f.path {
                        Some(path) => {
                            //If it's a /scratch job, and the files are reachable from the current host, move them into the current working directory
                            let path_obj = Path::new(&path);
                            let folder_name = path_obj
                                .components()
                                .next_back()
                                .unwrap()
                                .as_os_str()
                                .to_str()
                                .unwrap();

                            if !Path::exists(path_obj) {
                                error!(
                                    "{} Files for Job are not reachable from the current host. You will find your jobs's files on the scratch filesystem at Pawsey.",
                                    log_prefix
                                );
                            } else {
                                info!("{} Files for Job are reachable from the current host. Copying to current directory.", log_prefix);

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

    #[allow(clippy::too_many_arguments)]
    pub fn try_download(
        &self,
        url: &str,
        keep_tar: bool,
        no_resume: bool,
        hash: bool,
        file_info: &AsvoFilesArray,
        job: &AsvoJob,
        download_dir: &str,
        out_path: &PathBuf,
        log_prefix: &str,
        progress_bar: &ProgressBar,
    ) -> Result<(), AsvoError> {
        // How big should our in-memory download buffer be [MiB]?
        let buffer_size = match var("GIANT_SQUID_BUF_SIZE") {
            Ok(s) => s.parse()?,
            Err(_) => 100, // 100 MiB by default.
        } * 1024
            * 1024;

        // Get mwa asvo hash
        let mwa_asvo_hash = match &file_info.sha1 {
            Some(h) => h,
            None => panic!("{} job does not have an Sha1 hash! Please report this to asvo_support@mwatelescope.org", log_prefix),
        };

        let response: reqwest::blocking::Response;
        let mut tee: TeeReader<reqwest::blocking::Response, _>;

        // This updates the spinner twice per second
        progress_bar.enable_steady_tick(Duration::from_millis(500));

        info!(
            "{} Download starting (type: {}, {})",
            log_prefix,
            job.jtype,
            bytesize::ByteSize(file_info.size).to_string_as(true),
        );

        if keep_tar {
            let file_size_bytes: u64;
            let mut out_file: File;

            if out_path.try_exists()? {
                // File already exists!

                if no_resume {
                    out_file = File::open(out_path)?;
                } else {
                    out_file = File::options().append(true).open(out_path)?
                }

                // Get the size of the file
                file_size_bytes = File::metadata(&out_file)?.len();

                if no_resume && file_size_bytes < file_info.size {
                    warn!(
                        "{} Partial file {:?} exists, but --no-resume was set. Skipping file.",
                        log_prefix, out_path
                    );
                    return Ok(());
                }

                // If the file size matches the expected file size, skip downloading
                // if the hash matches
                if file_size_bytes == file_info.size {
                    info!(
                        "{} Checking downloaded file hash against provided MWA ASVO hash for {:?}...",
                        log_prefix, &out_path
                    );
                    // Now check the hash
                    match check_file_sha1_hash(out_path, mwa_asvo_hash, job.jobid) {
                        Ok(()) => {
                            // We already have the file and it is the right size and matches
                            // the hash, just get out of here!
                            progress_bar.finish_and_clear();
                            info!(
                                "{} File exists, is the correct size and matches the MWA ASVO provided hash. Skipping file.", log_prefix
                            );
                            return Ok(());
                        }
                        Err(_) => {
                            // Since the hash didn't match, just truncate the file and start again
                            if no_resume {
                                warn!("{} File exists and is the correct size, but the hash does not match the provided MWA ASVO hash. Leaving file as is, since --no-resume was set.", log_prefix);
                                return Ok(());
                            } else {
                                warn!("{} File exists and is the correct size, but the hash does not match the provided MWA ASVO hash. Restarting download...", log_prefix);

                                let out_file_result = File::create(out_path);

                                if out_file_result.is_err() {
                                    error!(
                                        "{} Error- cannot create file {:?}",
                                        log_prefix,
                                        out_path.display()
                                    );
                                }

                                out_file = out_file_result?;
                            }
                        }
                    }
                }
            } else {
                file_size_bytes = 0;

                let out_file_result = File::create(out_path);

                if out_file_result.is_err() {
                    error!(
                        "{} Error- cannot create file {:?}",
                        log_prefix,
                        out_path.display()
                    );
                }

                out_file = out_file_result?;
            }

            // Set the progress bar to be the number bytes in the file
            progress_bar.set_length(file_info.size);
            progress_bar.set_position(file_size_bytes);
            progress_bar.reset_eta();
            progress_bar.set_message(log_prefix.to_string());

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
            info!(
                "{} {} tar archive {:?}",
                log_prefix,
                if file_size_bytes > 0 {
                    "Resuming download of"
                } else {
                    "Downloading"
                },
                &out_path,
            );

            let mut file_buf = BufReader::with_capacity(buffer_size, tee.by_ref());

            loop {
                let buffer = file_buf.fill_buf()?;
                out_file.write_all(buffer)?;

                let length = buffer.len();

                file_buf.consume(length);

                if length == 0 {
                    break;
                } else {
                    // Increment progress bar
                    progress_bar.inc(length.try_into().unwrap());
                }
            }
        } else {
            // Stream-untar the response.
            let unpack_path = Path::new(download_dir);
            info!(
                "{} Downloading and untarring to {}",
                log_prefix,
                unpack_path.display()
            );

            response = self.client.get(url).send()?;
            tee = tee_readwrite::TeeReader::new(response, Sha1::new(), false);

            let mut tar = Archive::new(&mut tee);
            tar.set_preserve_mtime(false);

            let tar_entries = tar.entries()?;

            // Set progress max to be the full tar size (there is no compression
            // so the extracted size will == the tar size)
            progress_bar.set_length(file_info.size);
            progress_bar.set_position(0);
            progress_bar.reset_eta();
            progress_bar.set_message(log_prefix.to_string());

            // Loop through all files in the tar and unpack each one
            for file in tar_entries {
                let file = file.unwrap();
                let out_filename = &file.path()?.to_path_buf();
                let out_full_filename = unpack_path.join(out_filename);

                // Ignore the "." tar entry
                if !out_filename.to_str().unwrap().ends_with("/") {
                    debug!(
                        "{} Writing file {}",
                        log_prefix,
                        out_full_filename.display()
                    );
                    let mut file_buf = BufReader::with_capacity(buffer_size, file);
                    let out_file_result = File::create(&out_full_filename);

                    if out_file_result.is_err() {
                        error!(
                            "{} Error- cannot create file {:?}",
                            log_prefix,
                            out_full_filename.display()
                        );
                    }

                    let mut out_file = out_file_result?;

                    loop {
                        let buffer = file_buf.fill_buf()?;
                        out_file.write_all(buffer)?;

                        let length = buffer.len();

                        file_buf.consume(length);

                        if length == 0 {
                            break;
                        } else {
                            // Increment progress bar
                            progress_bar.inc(length.try_into().unwrap());
                        }
                    }
                } else if !out_full_filename.exists() {
                    // Create the directory
                    debug!("{} Creating directory {:?}", log_prefix, out_full_filename);
                    let create_dir_result = std::fs::create_dir(&out_full_filename);
                    if create_dir_result.is_err() {
                        error!(
                            "{} Error- cannot create directory {:?}",
                            log_prefix,
                            out_full_filename.display()
                        );
                        create_dir_result?;
                    }
                } else {
                    debug!(
                        "{} Directory exists {}",
                        log_prefix,
                        out_full_filename.display()
                    );
                }
            }
        }

        // If we were told to hash the download, compare our hash against
        // the upstream hash. Stream untarring may not read all of the
        // bytes; read the tee to the end.
        {
            let mut final_bytes = vec![];
            tee.read_to_end(&mut final_bytes)?;
            debug!("{} Read final bytes: {}", log_prefix, final_bytes.len());
        }

        progress_bar.finish_and_clear();

        if hash {
            info!(
                "{} Checking downloaded file hash against provided MWA ASVO hash for {:?}...",
                log_prefix, &out_path
            );
            debug!("{} MWA ASVO hash: {}", log_prefix, mwa_asvo_hash);
            let (_, hasher) = tee.into_inner();
            let hash = format!("{:x}", hasher.finalize());
            debug!("{} Our hash: {}", log_prefix, &hash);
            if !hash.eq_ignore_ascii_case(mwa_asvo_hash) {
                return Err(AsvoError::HashMismatch {
                    jobid: job.jobid,
                    file: url.to_string(),
                    calculated_hash: hash,
                    expected_hash: mwa_asvo_hash.to_string(),
                });
            }

            info!("{} File matches the MWA ASVO provided hash.", log_prefix);
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
        self.submit_asvo_job(&obsid, &AsvoJobType::DownloadVisibilities, form)
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
        self.submit_asvo_job(&obsid, &AsvoJobType::DownloadVoltage, form)
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

        self.submit_asvo_job(&obsid, &AsvoJobType::Conversion, form)
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
        self.submit_asvo_job(&obsid, &AsvoJobType::DownloadMetadata, form)
    }

    /// This low-level function actually submits jobs to the MWA ASVO.
    /// The return can either be:
    /// Ok(Some(jobid)) - this is when a new job is submitted
    /// Ok(None) - this is when an existing job is resubmitted
    /// Err() - this is when we hit an error
    fn submit_asvo_job(
        &self,
        obsid: &Obsid,
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
                    warn!(
                        "{}. Job Id: {} ObsID: {}",
                        error.as_str(),
                        job_id,
                        &obsid.to_string()
                    );
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
                    && (error
                        .as_str()
                        .starts_with("Unable to submit job. Observation")
                        || (error.as_str().starts_with("Observation ")
                            && error.as_str().ends_with(" does not exist")))
                {
                    // Some errors already have the obsid, so provide a different error if so
                    if error.as_str().contains(&obsid.to_string()) {
                        error!("{}", error.as_str());
                    } else {
                        error!("{} (ObsID: {})", error.as_str(), &obsid.to_string());
                    }
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
    use std::thread;
    use std::time::Duration;

    use rand::seq::IteratorRandom;

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
        let job_id = 0;
        let client = AsvoClient::new().unwrap();
        let cancel_result = client.cancel_asvo_job(job_id);
        let new_jobid_or_none = cancel_result.unwrap();
        assert!(new_jobid_or_none.is_none(), "{:?}", new_jobid_or_none);
    }

    #[test]
    fn test_cancel_job_successful() {
        let client = AsvoClient::new().unwrap();

        // submit a new job (don't worry we will cancel it right away)
        //
        // Due to potentially multiple test runs happening we need to randomise
        // the job params a bit so we don't have a situation where the job submission
        // fails because there is already an identical job running!
        #[derive(Clone)]
        struct Params<'a> {
            obs_id: Obsid,
            delivery: Delivery,
            delivery_format: Option<DeliveryFormat>,
            job_params: &'a BTreeMap<&'a str, &'a str>,
        }

        // Populate the choices

        // Averaging options
        let mut birli_10_0_5 = BTreeMap::new();
        birli_10_0_5.insert("avg_freq_res", "10");
        birli_10_0_5.insert("avg_time_res", "0.5");
        birli_10_0_5.insert("flag_edge_width", "80");
        let mut birli_20_1 = BTreeMap::new();
        birli_20_1.insert("avg_freq_res", "20");
        birli_20_1.insert("avg_time_res", "1");
        birli_20_1.insert("flag_edge_width", "80");
        let mut birli_40_1 = BTreeMap::new();
        birli_40_1.insert("avg_freq_res", "40");
        birli_40_1.insert("avg_time_res", "1");
        birli_40_1.insert("flag_edge_width", "80");
        let mut birli_40_2 = BTreeMap::new();
        birli_40_2.insert("avg_freq_res", "40");
        birli_40_2.insert("avg_time_res", "2");
        birli_40_2.insert("flag_edge_width", "80");
        let mut birli_80_2 = BTreeMap::new();
        birli_80_2.insert("avg_freq_res", "80");
        birli_80_2.insert("avg_time_res", "2");
        birli_80_2.insert("flag_edge_width", "80");

        let birli_options = [birli_10_0_5, birli_20_1, birli_40_1, birli_40_2, birli_80_2];

        let obs_list = [
            Obsid::validate(1416257384).unwrap(),
            Obsid::validate(1416257328).unwrap(),
            Obsid::validate(1416257272).unwrap(),
            Obsid::validate(1416257216).unwrap(),
            Obsid::validate(1416257160).unwrap(),
        ];

        let mut param_choices: Vec<Params> = Vec::new();

        for o in obs_list.iter() {
            for b in birli_options.iter() {
                param_choices.push(Params {
                    obs_id: *o,
                    delivery: Delivery::Acacia,
                    delivery_format: None,
                    job_params: b,
                });
            }
        }

        let mut new_job_id: Option<u32> = None;

        while new_job_id.is_none() {
            // Pick random set of params
            let p = &param_choices
                .clone()
                .into_iter()
                .choose(&mut rand::rng())
                .unwrap();

            let job_to_cancel =
                client.submit_conv(p.obs_id, p.delivery, p.delivery_format, p.job_params, true);

            match job_to_cancel {
                Ok(job_id_or_none) => {
                    if let Some(j) = job_id_or_none {
                        new_job_id = Some(j)
                    }
                }
                Err(error) => match error {
                    AsvoError::BadStatus {
                        code: c,
                        message: m,
                    } => panic!("Error has occurred: {} {}", c, m),
                    _ => panic!("Unexpected error has occured."),
                },
            }

            // If this job exists, go again using new params, but also just wait a bit
            thread::sleep(Duration::from_millis(5000));
        }

        let cancel_result = client.cancel_asvo_job(new_job_id.expect("No jobid was returned!"));

        assert!(
            cancel_result.is_ok_and(|j| j.expect("No jobid was returned from cancel")
                == new_job_id.expect("No jobid was returned!"))
        )
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

    /* TODO: uncomment once MWA ASVO server supports delivery to DUG
    #[test]
    fn test_submit_meta_to_dug() {
        let client = AsvoClient::new().unwrap();
        let obs_id = Obsid::validate(1343457784).unwrap();
        let delivery = Delivery::Dug;
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
    }*/

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
