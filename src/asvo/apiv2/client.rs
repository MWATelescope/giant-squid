// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A client for the MWA ASVO v2 API.
//!
//! The authentication flow here (`new`, `get_valid_tokens`, `login`,
//! `refresh`, JWT `exp` decoding, and the on-disk token cache shared with
//! mwa-cli) uses the request/response types generated from the MWA ASVO
//! OpenAPI schema (see `super::openapi`) instead of hand-rolled structs.
//! `token_store` and `get_asvo_server_address` are shared infrastructure
//! and are used directly from `crate::asvo` rather than being duplicated
//! here.

use std::cell::RefCell;
use std::env::var;
use std::num::NonZeroU64;
use std::str::FromStr;
use std::time::Duration;

use base64::Engine;
use chrono::{DateTime, Utc};
use log::{debug, trace, warn};
use reqwest::blocking::{Client, ClientBuilder};
use reqwest::header::{HeaderMap, HeaderValue};

use crate::asvo::token_store::{self, StoredTokens};
use crate::asvo::{
    download_by_jobid, download_by_obsid, get_asvo_server_address, get_asvo_server_address_env,
    AsvoJob, AsvoJobID, AsvoJobState, AsvoJobType, AsvoJobVec, DownloadOptions,
};
use crate::built_info;
use crate::obsid::Obsid;

use super::error::AsvoApiError;
use super::openapi::{
    ApiLoginRequest, ApiLoginResponse, BeamformerJobParams, ConversionJobParams, DownloadJobParams,
    ErrorResponse, ImagingJobFlow1Params, ImagingJobFlow2Params, JobDetailResponse,
    JobSubmittedResponse, JobsByUserRequest, JobsByUserResponse, Login, TokenResponse,
    UserResponse, VoltageJobParams,
};

const CONST_ENV_MWA_ASVO_API_KEY: &str = "MWA_ASVO_API_KEY";
const CONST_ENV_MWA_ASVO_API_TIMEOUT: &str = "MWA_ASVO_API_TIMEOUT";
const CONST_DEFAULT_MWA_ASVO_API_TIMEOUT: u64 = 60;

/// User-agent string sent on every request to the MWA ASVO.
const APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

/// Error codes the MWA ASVO returns when our access token is missing or
/// rejected. These are recoverable: logging in again gets us a new token.
/// In particular this covers a cached token that is still unexpired locally
/// but was minted by a different environment (e.g. dev vs test, which sign
/// JWTs with different secrets), so the other environment rejects it.
const AUTH_ERROR_CODES: [&str; 2] = ["AUTH_INVALID_TOKEN", "AUTH_REQUIRED"];

#[derive(Debug)]
pub struct AsvoClient {
    /// The `reqwest` [Client] used to interface with the MWA ASVO v2 API.
    ///
    /// Wrapped in a [`RefCell`] so that, if the server rejects our access
    /// token (`AUTH_INVALID_TOKEN` / `AUTH_REQUIRED`), `send_authed` can
    /// re-authenticate and swap in a fresh client mid-flight. This is sound
    /// because an `AsvoClient` never crosses a thread boundary: the
    /// parallel (rayon) download path builds its own client per worker
    /// rather than sharing one.
    client: RefCell<Client>,
    /// Details needed to perform a fresh login on demand (i.e. when the
    /// current token is rejected), so we don't have to thread them back
    /// through every call site.
    api_key: String,
    client_version: String,
    api_timeout_seconds: Option<u64>,
}

/// A minimal view of an HTTP response - the pieces callers need once the
/// body has been read and logged centrally by [`execute_logged`].
struct HttpResponse {
    status: reqwest::StatusCode,
    body: String,
}

/// Execute `builder` on `client`, logging one concise line for the request
/// and one for the response at debug level (i.e. `-v`): the HTTP method,
/// URL, and (for the response) the status. The full response body is logged
/// at trace level (`-vv`) only. Reading the body is centralised here so
/// every caller gets consistent logging in one place.
///
/// NOTE: login/refresh response bodies contain access and refresh tokens,
/// so they will appear in `-vv` (trace) logs.
fn execute_logged(
    client: &Client,
    builder: reqwest::blocking::RequestBuilder,
) -> reqwest::Result<HttpResponse> {
    let request = builder.build()?;
    let method = request.method().clone();
    let url = request.url().clone();
    debug!("HTTP request:  {} {}", method, url);

    if let Some(body) = request.body().and_then(|b| b.as_bytes()) {
        trace!("HTTP request body: {}", String::from_utf8_lossy(body));
    }

    let response = client.execute(request)?;
    let status = response.status();
    debug!("HTTP response: {} {} -> {}", method, url, status);

    let body = response.text()?;
    trace!("HTTP response body: {}", body);

    Ok(HttpResponse { status, body })
}

/// Decode a JWT's payload (without verifying its signature - we're only
/// reading the `exp` claim to know when a token we've already been handed
/// by the server will expire, not authenticating anything with it) and
/// return its expiry as a UTC timestamp.
fn decode_jwt_exp(token: &str) -> Result<DateTime<Utc>, AsvoApiError> {
    #[derive(serde::Deserialize)]
    struct JwtExpClaim {
        exp: i64,
    }

    let payload_b64 =
        token
            .split('.')
            .nth(1)
            .ok_or_else(|| AsvoApiError::AuthenticationFailed {
                message: "Malformed JWT returned by MWA ASVO: no payload segment".to_string(),
            })?;

    let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64)
        .map_err(|e| AsvoApiError::AuthenticationFailed {
            message: format!("Could not base64-decode JWT payload from MWA ASVO: {}", e),
        })?;

    let claim: JwtExpClaim =
        serde_json::from_slice(&payload_bytes).map_err(|e| AsvoApiError::AuthenticationFailed {
            message: format!("Could not parse JWT payload JSON from MWA ASVO: {}", e),
        })?;

    DateTime::<Utc>::from_timestamp(claim.exp, 0).ok_or_else(|| {
        AsvoApiError::AuthenticationFailed {
            message: "JWT `exp` claim from MWA ASVO was out of range".to_string(),
        }
    })
}

impl AsvoClient {
    /// Get a new reqwest [Client] which has authenticated with the MWA ASVO
    /// v2 API. Uses the `MWA_ASVO_API_KEY` environment variable for login.
    ///
    /// A cached session (shared with mwa-cli, at
    /// `$HOME/.mwa-asvo/tokens.json`) is reused if it's still valid,
    /// refreshed if only the access token has expired, or a fresh login is
    /// performed otherwise. This is all best-effort and silent: if caching
    /// isn't available or fails for any reason, giant-squid just falls back
    /// to a fresh login.
    pub fn new() -> Result<AsvoClient, AsvoApiError> {
        let api_key = var(CONST_ENV_MWA_ASVO_API_KEY).map_err(|_| AsvoApiError::MissingAuthKey)?;

        // Parse the timeout env variable or use default
        let api_timeout_seconds: Option<u64> = match var(CONST_ENV_MWA_ASVO_API_TIMEOUT) {
            Ok(val) => match val.parse::<u64>() {
                Ok(num) => {
                    debug!(
                        "{} timeout overidden to {} seconds",
                        CONST_ENV_MWA_ASVO_API_TIMEOUT, num
                    );
                    Some(num)
                }
                Err(e) => {
                    warn!(
                        "Environment variable {}='{}' is not valid, defaulting to {}. (It should be an integer number of seconds). Error: {}",
                        CONST_ENV_MWA_ASVO_API_TIMEOUT,
                        val,
                        CONST_DEFAULT_MWA_ASVO_API_TIMEOUT,
                        e
                    );
                    None
                }
            },
            Err(_) => {
                // Env variable was not present, no worries
                None
            }
        };

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
            debug!("Connecting to MWA ASVO... {}", get_asvo_server_address());
        }

        debug!("User Agent string: {}", APP_USER_AGENT);

        // Figure out which access token we're going to use: a cached one
        // (as-is, or refreshed), or a fresh login. Whichever path we take,
        // we end up with a valid `StoredTokens` to authenticate with.
        let tokens = Self::get_valid_tokens(&client_version, &api_key, api_timeout_seconds)?;

        // Build the "real" client, with the access token attached as a
        // default header on every request. If the server later rejects this
        // token, `send_authed` re-logs-in and swaps in a new client, which
        // is why we hold on to api_key/client_version/timeout below.
        let client = Self::build_authed_client(&tokens.access_token, api_timeout_seconds)?;

        Ok(AsvoClient {
            client: RefCell::new(client),
            api_key,
            client_version,
            api_timeout_seconds,
        })
    }

    /// Build a `reqwest` [Client] that sends `access_token` as the
    /// `mwa_access_token` cookie on every request. Factored out of `new` so
    /// `reauthenticate` can rebuild the client with a freshly-minted token.
    ///
    /// The token is attached explicitly (rather than relying on the cookie
    /// jar alone) so a session loaded from the cache behaves identically to
    /// one from a fresh login.
    fn build_authed_client(
        access_token: &str,
        api_timeout_seconds: Option<u64>,
    ) -> Result<Client, AsvoApiError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::COOKIE,
            HeaderValue::from_str(&format!("mwa_access_token={}", access_token)).map_err(|e| {
                AsvoApiError::AuthenticationFailed {
                    message: format!("MWA ASVO returned an invalid access token: {}", e),
                }
            })?,
        );

        Ok(ClientBuilder::new()
            .cookie_store(true)
            .connection_verbose(true)
            .user_agent(APP_USER_AGENT)
            .https_only(true)
            .default_headers(headers)
            .timeout(Duration::from_secs(
                api_timeout_seconds.unwrap_or(CONST_DEFAULT_MWA_ASVO_API_TIMEOUT),
            ))
            .build()?)
    }

    /// Build the short-lived [Client] used purely for login/refresh calls
    /// (it needs neither the cookie jar nor default auth headers).
    fn build_auth_client(api_timeout_seconds: Option<u64>) -> Result<Client, AsvoApiError> {
        Ok(ClientBuilder::new()
            .connection_verbose(true)
            .user_agent(APP_USER_AGENT)
            .https_only(true)
            .timeout(Duration::from_secs(
                api_timeout_seconds.unwrap_or(CONST_DEFAULT_MWA_ASVO_API_TIMEOUT),
            ))
            .build()?)
    }

    /// Returns a clone of the underlying HTTP client, for use by download
    /// functions that need to make direct HTTP requests (e.g. to Ceph
    /// signed URLs) outside the ASVO API. Cloning a reqwest client is cheap
    /// (it's reference-counted internally) and shares the same connection
    /// pool.
    ///
    /// NOTE: currently has no callers - the download path now goes through
    /// `download_jobid` / `download_obsid` - so this is a candidate for
    /// deletion.
    pub fn http_client(&self) -> Client {
        self.client.borrow().clone()
    }

    /// Download the MWA ASVO job with the given job ID.
    /// Fetches the current job list, locates the job, and downloads its
    /// files according to the supplied options.
    pub fn download_jobid(&self, jobid: AsvoJobID, opts: &DownloadOptions) -> anyhow::Result<()> {
        let jobs = self.get_jobs(None)?;
        let client = self.client.borrow();
        download_by_jobid(&client, jobs, jobid, opts)?;
        Ok(())
    }

    /// Download the MWA ASVO job associated with the given obsid.
    /// Fetches the current job list, locates the single ready job for
    /// the obsid, and downloads its files according to the supplied options.
    pub fn download_obsid(&self, obsid: Obsid, opts: &DownloadOptions) -> anyhow::Result<()> {
        let jobs = self.get_jobs(None)?;
        let client = self.client.borrow();
        download_by_obsid(&client, jobs, obsid, opts)?;
        Ok(())
    }

    /// Returns a valid, ready-to-use `StoredTokens`, preferring (in order):
    /// a still-valid cached session, a refreshed cached session, or a fresh
    /// login. Successful refreshes and logins are cached to disk for next
    /// time (best-effort; failure to cache is not fatal).
    fn get_valid_tokens(
        client_version: &str,
        api_key: &str,
        api_timeout_seconds: Option<u64>,
    ) -> Result<StoredTokens, AsvoApiError> {
        // A short-lived client, used only to perform the login/refresh call
        // itself (it doesn't need the cookie jar or auth headers).
        let auth_client = Self::build_auth_client(api_timeout_seconds)?;

        if let Some(cached) = token_store::load() {
            if cached.is_access_valid() {
                debug!("Reusing cached MWA ASVO session (shared with mwa-cli)");
                return Ok(cached);
            }

            if cached.is_refresh_valid() {
                debug!("Cached MWA ASVO access token expired; refreshing session");
                match Self::refresh(&auth_client, &cached) {
                    Ok(refreshed) => {
                        token_store::save(&refreshed);
                        return Ok(refreshed);
                    }
                    Err(e) => {
                        // Refresh can fail legitimately (e.g. the refresh
                        // token was already rotated by another process, or
                        // by mwa-cli). Fall through to a fresh login rather
                        // than treating this as fatal.
                        debug!(
                            "MWA ASVO session refresh failed, falling back to fresh login: {}",
                            e
                        );
                    }
                }
            }
        }

        debug!("Performing fresh MWA ASVO login");
        let fresh = Self::login(&auth_client, client_version, api_key)?;
        token_store::save(&fresh);
        Ok(fresh)
    }

    /// Perform a fresh login against the MWA ASVO v2 API using the API key.
    fn login(
        auth_client: &Client,
        client_version: &str,
        api_key: &str,
    ) -> Result<StoredTokens, AsvoApiError> {
        let login: Login = client_version.try_into()?;

        let response = execute_logged(
            auth_client,
            auth_client
                .post(format!("{}/api/v2/api_login", get_asvo_server_address()))
                .json(&ApiLoginRequest {
                    login,
                    password: api_key.to_string(),
                }),
        )?;

        if !response.status.is_success() {
            return Err(AsvoApiError::AuthenticationFailed {
                message: response.body,
            });
        }

        let auth: ApiLoginResponse = serde_json::from_str(&response.body)?;

        Self::stored_tokens_from_auth(auth.access_token, auth.refresh_token, auth.user)
    }

    /// Refresh an existing session against the MWA ASVO v2 API. The refresh
    /// endpoint's response (`TokenResponse`) doesn't include user info, so
    /// we carry it over from the session being refreshed - it's still the
    /// same account.
    fn refresh(
        auth_client: &Client,
        previous: &StoredTokens,
    ) -> Result<StoredTokens, AsvoApiError> {
        let response = execute_logged(
            auth_client,
            auth_client
                .post(format!("{}/api/v2/refresh", get_asvo_server_address()))
                .header(
                    reqwest::header::COOKIE,
                    format!("mwa_refresh_token={}", previous.refresh_token),
                ),
        )?;

        if !response.status.is_success() {
            return Err(AsvoApiError::AuthenticationFailed {
                message: response.body,
            });
        }

        let token: TokenResponse = serde_json::from_str(&response.body)?;

        Self::stored_tokens_from_parts(
            token.access_token,
            token.refresh_token,
            previous.user_id,
            previous.user_login.clone(),
            previous.user_email.clone(),
        )
    }

    /// Force a fresh login (ignoring any cached session), persist the new
    /// tokens, and swap the freshly-authenticated client in as our active
    /// one. Called by `send_authed` when the server rejects our current
    /// access token.
    fn reauthenticate(&self) -> Result<(), AsvoApiError> {
        debug!("Re-authenticating with MWA ASVO after a rejected access token");
        let auth_client = Self::build_auth_client(self.api_timeout_seconds)?;
        let fresh = Self::login(&auth_client, &self.client_version, &self.api_key)?;
        token_store::save(&fresh);
        let new_client = Self::build_authed_client(&fresh.access_token, self.api_timeout_seconds)?;
        *self.client.borrow_mut() = new_client;
        Ok(())
    }

    /// Build a `StoredTokens` from a login response's pieces, decoding each
    /// JWT's `exp` claim to determine its expiry (the API response doesn't
    /// provide expiry timestamps directly).
    fn stored_tokens_from_auth(
        access_token: String,
        refresh_token: String,
        user: UserResponse,
    ) -> Result<StoredTokens, AsvoApiError> {
        // `StoredTokens.user_id` is a u64 (it's shared with mwa-cli's token
        // cache format), but the generated `UserResponse.id` is an i64.
        // A negative user ID from the server would be unexpected, but
        // let's not panic or silently wrap if it ever happened.
        let user_id = u64::try_from(user.id).map_err(|_| AsvoApiError::AuthenticationFailed {
            message: format!(
                "MWA ASVO returned an invalid (negative) user ID: {}",
                user.id
            ),
        })?;

        Self::stored_tokens_from_parts(access_token, refresh_token, user_id, user.login, user.email)
    }

    fn stored_tokens_from_parts(
        access_token: String,
        refresh_token: String,
        user_id: u64,
        user_login: String,
        user_email: String,
    ) -> Result<StoredTokens, AsvoApiError> {
        let access_expires_at = decode_jwt_exp(&access_token)?;
        let refresh_expires_at = decode_jwt_exp(&refresh_token)?;

        Ok(StoredTokens {
            access_token,
            refresh_token,
            access_expires_at,
            refresh_expires_at,
            user_id,
            user_login,
            user_email,
        })
    }

    /// Send an authenticated request, retrying it once if the server
    /// rejects our access token.
    ///
    /// `build` produces the request from a given [`Client`]; it may be
    /// called a second time - against a freshly-authenticated client - if
    /// the first attempt fails with an authentication error (see
    /// [`AUTH_ERROR_CODES`]). This transparently recovers a cached token
    /// that is unexpired locally but rejected by the server, e.g. after
    /// switching between the dev and test environments.
    ///
    /// On success the response body is returned for the caller to parse; a
    /// non-success status is mapped to an [`AsvoApiError`] by
    /// [`Self::error_from_body`]. All requests are logged via
    /// [`execute_logged`].
    fn send_authed<F>(&self, build: F) -> Result<String, AsvoApiError>
    where
        F: Fn(&Client) -> reqwest::blocking::RequestBuilder,
    {
        // Clone the client out of the RefCell rather than holding a borrow
        // across the (blocking) send, so `reauthenticate` is free to take a
        // mutable borrow on the retry path. Cloning a reqwest client is
        // cheap - it's reference-counted internally.
        let client = self.client.borrow().clone();
        let response = execute_logged(&client, build(&client))?;
        if response.status.is_success() {
            return Ok(response.body);
        }

        let err = Self::error_from_body(response.status, response.body);
        if !Self::is_auth_error(&err) {
            return Err(err);
        }

        // Token rejected: re-login once and retry the request against the
        // freshly-swapped client.
        self.reauthenticate()?;
        let client = self.client.borrow().clone();
        let response = execute_logged(&client, build(&client))?;
        if response.status.is_success() {
            return Ok(response.body);
        }

        Err(Self::error_from_body(response.status, response.body))
    }

    /// Map a non-success response body to an [`AsvoApiError`], preferring the
    /// structured `ErrorResponse` shape and falling back to `BadStatus` for
    /// anything that doesn't match it.
    fn error_from_body(status: reqwest::StatusCode, body: String) -> AsvoApiError {
        match serde_json::from_str::<ErrorResponse>(&body) {
            Ok(err) => AsvoApiError::ApiError {
                error_code: err.error_code,
                message: err.message,
                detail: err.detail,
                suggestion: err.suggestion,
            },
            Err(_) => AsvoApiError::BadStatus {
                code: status,
                message: body,
            },
        }
    }

    /// Whether `err` is an authentication failure that a fresh login might
    /// fix (see [`AUTH_ERROR_CODES`]).
    fn is_auth_error(err: &AsvoApiError) -> bool {
        matches!(
            err,
            AsvoApiError::ApiError { error_code, .. }
                if AUTH_ERROR_CODES.contains(&error_code.as_str())
        )
    }

    /// Fetch the caller's MWA ASVO jobs, returning an `AsvoJobVec` so that
    /// the rest of giant-squid (filtering, `--json`, table rendering) can
    /// consume the results unchanged.
    ///
    /// `days` limits the results to the last N days if given. If `None`,
    /// we explicitly send `days: null` to ask for the caller's full
    /// history - ASSUMPTION: I haven't been able to confirm the server
    /// treats a null `days` as "no limit" rather than falling back to its
    /// own default (30) regardless; please check this against the real
    /// server. Deliberately not filtering by job_state/job_type/date
    /// server-side, since the API only supports a single value for each
    /// and the existing CLI filtering (multi-value, by job ID/obsid) is
    /// staying client-side unchanged.
    ///
    /// Individual jobs that can't be reliably converted (an obs_id we
    /// can't find/parse in the untyped `job_params`, or a job_state we
    /// don't recognise) are skipped with a warning logged, rather than
    /// failing the whole listing - see `job_detail_to_asvo_job`.
    pub fn get_jobs(&self, days: Option<i64>) -> Result<AsvoJobVec, AsvoApiError> {
        const PAGE_SIZE: u64 = 100;

        let mut all_jobs = Vec::new();
        let mut offset: u64 = 0;

        loop {
            let request = JobsByUserRequest {
                date_from: None,
                date_to: None,
                days,
                job_state: None,
                job_type: None,
                limit: NonZeroU64::new(PAGE_SIZE).unwrap(),
                offset,
                sort_by: "id".to_string(),
            };

            // Confirmed via testing against the real dev server: POST to
            // /api/v2/get_jobs (my original guess of /api/v2/job_history
            // was wrong).
            let body = self.send_authed(|client| {
                client
                    .post(format!("{}/api/v2/get_jobs", get_asvo_server_address()))
                    .json(&request)
            })?;
            let page: JobsByUserResponse = serde_json::from_str(&body)?;

            let page_len = page.jobs.len() as u64;
            for mut job_value in page.jobs {
                normalize_job_value(&mut job_value);

                // Hard error: this is a basic structural mismatch against
                // the schema (an item that isn't even a JobDetailResponse
                // shape), not a content-level ambiguity like the ones
                // job_detail_to_asvo_job skips over individually.
                let detail: JobDetailResponse =
                    serde_json::from_value(serde_json::Value::Object(job_value))?;
                if let Some(job) = job_detail_to_asvo_job(detail) {
                    all_jobs.push(job);
                }
            }

            offset += page_len;
            let total_count = u64::try_from(page.total_count).unwrap_or(0);
            if page_len == 0 || offset >= total_count {
                break;
            }
        }

        Ok(AsvoJobVec(all_jobs))
    }

    /// Submit an MWA ASVO v2 imaging job. Returns the new job's ID.
    ///
    /// ASSUMPTION (unconfirmed against the real server): POST to
    /// /api/v2/imaging_job, and a 200 response body is a bare JSON
    /// integer (the job ID) - both per the original hand-written spec for
    /// this endpoint, from before real generated types existed. Given
    /// that both the job-listing endpoint's path (/job_history guessed,
    /// /get_jobs actual) and its timestamp format turned out to need
    /// correction against the real server, treat this the same way:
    /// probably needs adjusting once tried for real.
    pub fn submit_imaging_job(&self, params: &ImagingJobFlow1Params) -> Result<i64, AsvoApiError> {
        debug!("Submitting an imaging job to MWA ASVO v2");

        let body = self.send_authed(|client| {
            client
                .post(format!("{}/api/v2/imaging_job", get_asvo_server_address()))
                .json(params)
        })?;

        let job_id: i64 = serde_json::from_str(&body)?;
        Ok(job_id)
    }

    pub fn submit_image_from_job(
        &self,
        params: &ImagingJobFlow2Params,
    ) -> Result<i64, AsvoApiError> {
        debug!("Submitting an image-from-job to MWA ASVO v2");

        let body = self.send_authed(|client| {
            client
                .post(format!(
                    "{}/api/v2/image_from_job",
                    get_asvo_server_address()
                ))
                .json(params)
        })?;

        let job_id: i64 = serde_json::from_str(&body)?;
        Ok(job_id)
    }

    pub fn submit_download_vis_job(
        &self,
        params: &DownloadJobParams,
    ) -> Result<JobSubmittedResponse, AsvoApiError> {
        debug!("Submitting a download-vis job to MWA ASVO v2");

        let body = self.send_authed(|client| {
            client
                .post(format!(
                    "{}/api/v2/download_vis_job",
                    get_asvo_server_address()
                ))
                .json(params)
        })?;

        let resp: JobSubmittedResponse = serde_json::from_str(&body)?;
        Ok(resp)
    }

    pub fn submit_conversion_job(
        &self,
        params: &ConversionJobParams,
    ) -> Result<JobSubmittedResponse, AsvoApiError> {
        debug!("Submitting a conversion job to MWA ASVO v2");

        let body = self.send_authed(|client| {
            client
                .post(format!(
                    "{}/api/v2/conversion_job",
                    get_asvo_server_address()
                ))
                .json(params)
        })?;

        let resp: JobSubmittedResponse = serde_json::from_str(&body)?;
        Ok(resp)
    }

    pub fn submit_voltage_job(
        &self,
        params: &VoltageJobParams,
    ) -> Result<JobSubmittedResponse, AsvoApiError> {
        debug!("Submitting a voltage job to MWA ASVO v2");

        let body = self.send_authed(|client| {
            client
                .post(format!("{}/api/v2/voltage_job", get_asvo_server_address()))
                .json(params)
        })?;

        let resp: JobSubmittedResponse = serde_json::from_str(&body)?;
        Ok(resp)
    }

    pub fn submit_beamformer_job(
        &self,
        params: &BeamformerJobParams,
    ) -> Result<JobSubmittedResponse, AsvoApiError> {
        debug!("Submitting a beamformer job to MWA ASVO v2");

        let body = self.send_authed(|client| {
            client
                .post(format!(
                    "{}/api/v2/beamformer_job",
                    get_asvo_server_address()
                ))
                .json(params)
        })?;

        let resp: JobSubmittedResponse = serde_json::from_str(&body)?;
        Ok(resp)
    }

    pub fn cancel_job(&self, job_id: AsvoJobID) -> Result<JobSubmittedResponse, AsvoApiError> {
        debug!("Cancelling MWA ASVO v2 job {}", job_id);

        let body = self.send_authed(|client| {
            client.delete(format!(
                "{}/api/v2/jobs/{}",
                get_asvo_server_address(),
                job_id
            ))
        })?;

        let resp: JobSubmittedResponse = serde_json::from_str(&body)?;
        Ok(resp)
    }
}

/// Patches two confirmed real-server quirks into a raw job JSON object
/// before we try to deserialize it as `JobDetailResponse`, since we can't
/// fix the generated type directly (it gets overwritten on regeneration).
/// Both are worth raising with the API dev so they become unnecessary:
///
/// 1. `completed`/`started`/`modified` are sometimes omitted entirely
///    (confirmed: `modified` was absent, not null, in a real response)
///    rather than sent as JSON `null`. Those three fields lack a
///    `#[serde(default)]` in the generated type (unlike `error_text`/
///    `product`, which do), so a genuinely missing key is a hard
///    deserialize error, not a `None`. Fixed by inserting `null` for any
///    of the three that are missing.
/// 2. Timestamps (`created`, and the three above when present) come back
///    without a timezone designator (confirmed: `"created":
///    "2026-09-08T05:41:54.757232"`, no `Z`/offset) - a naive timestamp,
///    not RFC3339. `chrono::DateTime<Utc>`'s deserializer requires
///    RFC3339 and fails with "premature end of input" on a naive one.
///    Fixed by appending `Z` to any of these four fields' string values
///    that don't already have a timezone marker (assuming UTC, which
///    matches the schema's own `DateTime<Utc>` typing).
fn normalize_job_value(job_value: &mut serde_json::Map<String, serde_json::Value>) {
    for key in ["completed", "started", "modified"] {
        job_value.entry(key).or_insert(serde_json::Value::Null);
    }

    for key in ["created", "completed", "started", "modified"] {
        if let Some(serde_json::Value::String(s)) = job_value.get_mut(key) {
            if looks_like_naive_timestamp(s) {
                s.push('Z');
            }
        }
    }
}

/// Does `s` look like an ISO8601 timestamp with no timezone designator?
/// Deliberately simple: skip the `YYYY-MM-DD` date portion (which has its
/// own `-` characters that would otherwise look like a negative UTC
/// offset), then check whether what's left names a zone at all.
fn looks_like_naive_timestamp(s: &str) -> bool {
    match s.get(10..) {
        Some(rest) => !rest.is_empty() && !rest.contains(['Z', '+', '-']),
        None => false,
    }
}

/// Best-effort conversion from the v2 API's `JobDetailResponse` into the
/// existing `AsvoJob` domain type. Returns `None` (after logging a warning)
/// if a job can't be reliably converted; callers should skip that job and
/// continue rather than fail the whole listing.
///
/// - `job_type` codes we don't recognise become `AsvoJobType::Unknown`
///   (existing forward-compat behaviour, never fails).
/// - `job_state` is parsed via the existing `AsvoJobState::FromStr`, with
///   "completed" and "error" special-cased (see comment at the match
///   below) - confirmed "staging"/"staged" round-trip correctly via real
///   responses, but the full vocabulary isn't confirmed.
/// - `obsid` is looked for at `job_params["obs_id"]` (an untyped JSON
///   map). CONFIRMED against a real response: the key name is right, but
///   the value is a JSON string, not a number - handled below.
/// - `files` is always `None` for now - `product`'s shape isn't
///   confirmed, so File Size/Delivery will show blank until we have a
///   real sample response to design against.
fn job_detail_to_asvo_job(detail: JobDetailResponse) -> Option<AsvoJob> {
    let jobid = match AsvoJobID::try_from(detail.id) {
        Ok(id) => id,
        Err(_) => {
            warn!(
                "Skipping MWA ASVO job: ID {} doesn't fit in the expected range",
                detail.id
            );
            return None;
        }
    };

    // ASSUMPTION resolved by a real sample response: the key is `obs_id`
    // as guessed, but its value is a JSON string (e.g. "1455950264"), not
    // a number - handle both, in case that's not consistent across jobs.
    let obs_id_value = detail.job_params.get("obs_id").and_then(|v| {
        v.as_u64()
            .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
    });

    let obsid = match obs_id_value {
        Some(o) => match Obsid::validate(o) {
            Ok(obsid) => obsid,
            Err(e) => {
                warn!(
                    "Skipping MWA ASVO job {}: invalid obs_id in job_params: {}",
                    jobid, e
                );
                return None;
            }
        },
        None => {
            warn!(
                "Skipping MWA ASVO job {}: couldn't find a usable obs_id in job_params",
                jobid
            );
            return None;
        }
    };

    let jtype = match *detail.job_type {
        0 => AsvoJobType::Conversion,
        1 => AsvoJobType::DownloadVisibilities,
        2 => AsvoJobType::DownloadMetadata,
        3 => AsvoJobType::DownloadVoltage,
        4 => AsvoJobType::CancelJob,
        5 => AsvoJobType::DownloadBeamformer,
        6 => AsvoJobType::Imaging,
        _ => AsvoJobType::Unknown,
    };

    // The API's job_state vocabulary (JobsByUserRequestJobState) uses
    // "completed" and has no "ready"/"expired" at all, whereas
    // AsvoJobState::from_str expects "ready" for the same concept.
    // Translated locally rather than changing AsvoJobState itself (which
    // CLI argument parsing also uses). Also: since JobDetailResponse
    // separately carries `error_text`, use it to populate
    // AsvoJobState::Error's message instead of discarding it.
    let state = match detail.job_state.as_str() {
        "completed" => AsvoJobState::Ready,
        "error" => AsvoJobState::Error(detail.error_text.unwrap_or_default()),
        other => match AsvoJobState::from_str(other) {
            Ok(state) => state,
            Err(_) => {
                warn!(
                    "Skipping MWA ASVO job {}: unrecognised job_state {:?}",
                    jobid, other
                );
                return None;
            }
        },
    };

    Some(AsvoJob {
        obsid,
        jobid,
        jtype,
        state,
        files: None,
        completed: detail.completed,
    })
}
