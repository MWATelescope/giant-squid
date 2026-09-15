// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Code to interface with the MWA ASVO.
pub mod apiv2;
mod error;
mod token_store;
mod types;

use crate::check_file_sha1_hash;
use crate::obsid::Obsid;
pub use apiv2::client::AsvoClientv2;
pub use apiv2::Apiv2Error;
pub use error::AsvoError;
pub use token_store::StoredTokens;
pub use types::{
    AsvoFilesArray, AsvoJob, AsvoJobID, AsvoJobMap, AsvoJobState, AsvoJobType, AsvoJobVec, Delivery,
};

use std::env::{current_dir, var, VarError};
use std::fs::{rename, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use backoff::{retry, Error, ExponentialBackoff};
use indicatif::ProgressBar;
use log::{debug, error, info, warn};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, RANGE};
use sha1::{Digest, Sha1};
use tar::Archive;
use tee_readwrite::TeeReader;

const CONST_ENV_MWA_ASVO_HOST: &str = "MWA_ASVO_HOST";
const CONST_ENV_GIANT_SQUID_BUF_SIZE: &str = "GIANT_SQUID_BUF_SIZE";
const CONST_DEFAULT_URL: &str = "https://asvo.mwatelescope.org:443";

// Returns a custom MWA ASVO host address (via a set env var)
// or returns VarError::NotPresent error when not set
pub fn get_asvo_server_address_env() -> Result<String, VarError> {
    std::env::var(CONST_ENV_MWA_ASVO_HOST)
}

pub fn get_asvo_server_address() -> String {
    get_asvo_server_address_env()
        .unwrap_or_else(|_| String::from(CONST_DEFAULT_URL))
        .to_string()
}

/// Download the specified MWA ASVO job ID.
#[allow(clippy::too_many_arguments)]
pub fn download_jobid(
    http_client: &Client,
    jobs: AsvoJobVec,
    jobid: AsvoJobID,
    keep_tar: bool,
    no_resume: bool,
    hash: bool,
    download_dir: &str,
    progress_bar: &ProgressBar,
    download_number: usize,
    download_count: usize,
) -> Result<(), AsvoError> {
    let mut jobs = jobs;
    debug!("Attempting to download job {}", jobid);
    // Filter all jobs but the one we're interested in.
    jobs.0.retain(|j| j.jobid == jobid);
    match jobs.0.len() {
        0 => Err(AsvoError::NoAsvoJob(jobid)),
        1 => download(
            http_client,
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
    http_client: &Client,
    jobs: AsvoJobVec,
    obsid: Obsid,
    keep_tar: bool,
    no_resume: bool,
    hash: bool,
    download_dir: &str,
    progress_bar: &ProgressBar,
    download_number: usize,
    download_count: usize,
) -> Result<(), AsvoError> {
    let mut all_jobs = jobs.clone();

    debug!("Attempting to download obsid {}", obsid);
    // Filter all MWA ASVO jobs by obsid.
    // If we don't have exactly one match for ready jobs, we
    // have to bug out. Make a clone of all jobs so we can use
    // it below, without needing to go back to the web server
    let mut all_ready_jobs: AsvoJobVec = jobs;

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
        1 => download(
            http_client,
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
    http_client: &Client,
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
                    debug!("{} Downloading from url {}", log_prefix, url);

                    // parse out path from url
                    let url_obj = reqwest::Url::parse(url).unwrap();
                    let out_path = Path::new(&download_dir)
                        .join(url_obj.path_segments().unwrap().next_back().unwrap());

                    let op = || {
                        try_download(
                            http_client,
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
                            // if we get 404 we should not retry AND we should provide a nicer error message
                            AsvoError::HttpError { status: 404, .. } => {
                                Error::permanent(AsvoError::Http404Error { job_id: job.jobid })
                            }
                            // If we get 401, 403 or 404 we should not retry
                            AsvoError::HttpError {
                                status: 401 | 403, ..
                            } => Error::permanent(e),
                            _ => Error::transient(e),
                        })
                    };

                    // This next if is a bit counterintuitive to read, but it means:
                    // Run the operation with exponential backoff retrying on transient errors. If it ultimately fails with a permanent error, return that error to the caller.
                    match retry(ExponentialBackoff::default(), op) {
                        Ok(()) => {}
                        Err(Error::Permanent(err)) => return Err(err),
                        Err(Error::Transient { err, .. }) => return Err(err),
                    }

                    let elapsed = start_time.elapsed();
                    let elapsed_ms = elapsed.as_millis() as u64;

                    let throughput_str = if elapsed_ms == 0 {
                        "N/A".to_string()
                    } else {
                        bytesize::ByteSize(
                            (f.size * 1000).checked_div(elapsed_ms).unwrap_or_default(),
                        )
                        .display()
                        .iec()
                        .to_string()
                    };

                    let duration_str = if elapsed.as_secs() > 60 {
                        format!(
                            "{} min {:.2} s",
                            elapsed.as_secs() / 60,
                            (elapsed.as_millis() as f64 / 1e3) % 60.0
                        )
                    } else {
                        format!("{:.3} s", elapsed.as_millis() as f64 / 1e3)
                    };

                    info!(
                        "{} Completed download of {} in {} ({}/s)",
                        log_prefix,
                        bytesize::ByteSize(f.size).display().iec(),
                        duration_str,
                        throughput_str
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
    http_client: &Client,
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
    let buffer_size = match var(CONST_ENV_GIANT_SQUID_BUF_SIZE) {
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
        bytesize::ByteSize(file_info.size).display().iec(),
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
                    log_prefix, out_path
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
        // from where we left off. If file_size_bytes == 0 then we'll start from the start!
        let mut headers = HeaderMap::new();
        headers.insert(
            RANGE,
            HeaderValue::from_str(&format!(
                "Range: bytes={}-{}",
                file_size_bytes, file_info.size
            ))
            .unwrap(),
        );

        let raw_response = http_client.get(url).headers(headers).send()?;

        // Check HTTP status before attempting to read the response body
        let status = raw_response.status();
        if !status.is_success() {
            let body = raw_response.text().unwrap_or_default();
            error!(
                "{} HTTP error {} downloading tar {:?}: {}",
                log_prefix, status, out_path, body
            );
            return Err(AsvoError::HttpError {
                status: status.as_u16(),
                message: body,
            });
        }

        response = raw_response;
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
            out_path,
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

        let raw_response = http_client.get(url).send()?;

        // Check HTTP status before attempting to parse body as a tar archive
        let status = raw_response.status();
        if !status.is_success() {
            let body = raw_response.text().unwrap_or_default();
            error!(
                "{} HTTP error {} downloading and untarring to {}: {}",
                log_prefix,
                status,
                unpack_path.display(),
                body
            );
            return Err(AsvoError::HttpError {
                status: status.as_u16(),
                message: body,
            });
        }

        response = raw_response;
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
            log_prefix, out_path
        );
        debug!("{} MWA ASVO hash: {}", log_prefix, mwa_asvo_hash);
        let (_, hasher) = tee.into_inner();
        let hash = format!("{:x}", hasher.finalize());
        debug!("{} Our hash: {}", log_prefix, hash);
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
