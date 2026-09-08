// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A client for the MWA ASVO v2 API.
//!
//! The authentication flow here (`new`, `get_valid_tokens`, `login`,
//! `refresh`, JWT `exp` decoding, and the on-disk token cache shared with
//! mwa-cli) is adapted from [`crate::asvo::AsvoClient`], the v1 client -
//! updated to use the request/response types generated from the MWA ASVO
//! OpenAPI schema (see `super::openapi`) instead of hand-rolled structs.
//! `token_store` and `get_asvo_server_address` are genuinely shared
//! infrastructure (not v1-specific) and are used directly from
//! `crate::asvo` rather than being duplicated here.

use std::env::var;
use std::num::NonZeroU64;
use std::str::FromStr;
use std::time::Duration;

use base64::Engine;
use chrono::{DateTime, Utc};
use log::{debug, warn};
use reqwest::blocking::{Client, ClientBuilder};
use reqwest::header::{HeaderMap, HeaderValue};

use crate::asvo::token_store::{self, StoredTokens};
use crate::asvo::{
    get_asvo_server_address, get_asvo_server_address_env, AsvoJob, AsvoJobID, AsvoJobState,
    AsvoJobType, AsvoJobVec,
};
use crate::built_info;
use crate::obsid::Obsid;

use super::error::Apiv2Error;
use super::openapi::{
    ApiLoginRequest, ApiLoginResponse, ErrorResponse, JobDetailResponse, JobsByUserRequest,
    JobsByUserResponse, Login, TokenResponse, UserResponse,
};

const CONST_ENV_MWA_ASVO_API_KEY: &str = "MWA_ASVO_API_KEY";
const CONST_ENV_MWA_ASVO_API_TIMEOUT: &str = "MWA_ASVO_API_TIMEOUT";
const CONST_DEFAULT_MWA_ASVO_API_TIMEOUT: u64 = 60;

#[derive(Debug)]
pub struct AsvoClientv2 {
    /// The `reqwest` [Client] used to interface with the MWA ASVO v2 API.
    client: Client,
}

/// Decode a JWT's payload (without verifying its signature - we're only
/// reading the `exp` claim to know when a token we've already been handed
/// by the server will expire, not authenticating anything with it) and
/// return its expiry as a UTC timestamp.
fn decode_jwt_exp(token: &str) -> Result<DateTime<Utc>, Apiv2Error> {
    #[derive(serde::Deserialize)]
    struct JwtExpClaim {
        exp: i64,
    }

    let payload_b64 = token
        .split('.')
        .nth(1)
        .ok_or_else(|| Apiv2Error::AuthenticationFailed {
            message: "Malformed JWT returned by MWA ASVO: no payload segment".to_string(),
        })?;

    let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64)
        .map_err(|e| Apiv2Error::AuthenticationFailed {
            message: format!("Could not base64-decode JWT payload from MWA ASVO: {}", e),
        })?;

    let claim: JwtExpClaim =
        serde_json::from_slice(&payload_bytes).map_err(|e| Apiv2Error::AuthenticationFailed {
            message: format!("Could not parse JWT payload JSON from MWA ASVO: {}", e),
        })?;

    DateTime::<Utc>::from_timestamp(claim.exp, 0).ok_or_else(|| Apiv2Error::AuthenticationFailed {
        message: "JWT `exp` claim from MWA ASVO was out of range".to_string(),
    })
}

impl AsvoClientv2 {
    /// Get a new reqwest [Client] which has authenticated with the MWA ASVO
    /// v2 API. Uses the `MWA_ASVO_API_KEY` environment variable for login.
    ///
    /// A cached session (shared with mwa-cli, at
    /// `$HOME/.mwa-asvo/tokens.json`) is reused if it's still valid,
    /// refreshed if only the access token has expired, or a fresh login is
    /// performed otherwise. This is all best-effort and silent: if caching
    /// isn't available or fails for any reason, giant-squid just falls back
    /// to a fresh login.
    pub fn new() -> Result<AsvoClientv2, Apiv2Error> {
        static APP_USER_AGENT: &str =
            concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

        let api_key = var(CONST_ENV_MWA_ASVO_API_KEY).map_err(|_| Apiv2Error::MissingAuthKey)?;

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
        // default header on every request. We do this explicitly (rather
        // than relying on the cookie jar alone) so that a session loaded
        // from the cache behaves identically to one from a fresh login.
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::COOKIE,
            HeaderValue::from_str(&format!("mwa_access_token={}", tokens.access_token)).map_err(
                |e| Apiv2Error::AuthenticationFailed {
                    message: format!("MWA ASVO returned an invalid access token: {}", e),
                },
            )?,
        );

        let client = ClientBuilder::new()
            .cookie_store(true)
            .connection_verbose(true)
            .user_agent(APP_USER_AGENT)
            .https_only(true)
            .default_headers(headers)
            .timeout(Duration::from_secs(
                api_timeout_seconds.unwrap_or(CONST_DEFAULT_MWA_ASVO_API_TIMEOUT),
            ))
            .build()?;

        Ok(AsvoClientv2 { client })
    }

    /// Returns a valid, ready-to-use `StoredTokens`, preferring (in order):
    /// a still-valid cached session, a refreshed cached session, or a fresh
    /// login. Successful refreshes and logins are cached to disk for next
    /// time (best-effort; failure to cache is not fatal).
    fn get_valid_tokens(
        client_version: &str,
        api_key: &str,
        api_timeout_seconds: Option<u64>,
    ) -> Result<StoredTokens, Apiv2Error> {
        // A short-lived client, used only to perform the login/refresh call
        // itself (it doesn't need the cookie jar or auth headers).
        let auth_client = ClientBuilder::new()
            .connection_verbose(true)
            .user_agent(concat!(
                env!("CARGO_PKG_NAME"),
                "/",
                env!("CARGO_PKG_VERSION")
            ))
            .https_only(true)
            .timeout(Duration::from_secs(
                api_timeout_seconds.unwrap_or(CONST_DEFAULT_MWA_ASVO_API_TIMEOUT),
            ))
            .build()?;

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
    ) -> Result<StoredTokens, Apiv2Error> {
        let login: Login = client_version.try_into()?;

        let response = auth_client
            .post(format!("{}/api/v2/api_login", get_asvo_server_address()))
            .json(&ApiLoginRequest {
                login,
                password: api_key.to_string(),
            })
            .send()?;

        if !response.status().is_success() {
            return Err(Apiv2Error::AuthenticationFailed {
                message: response.text().unwrap_or_default(),
            });
        }

        let body = response.text()?;
        debug!("MWA ASVO v2 login response body: {}", body);
        let auth: ApiLoginResponse = serde_json::from_str(&body)?;

        Self::stored_tokens_from_auth(auth.access_token, auth.refresh_token, auth.user)
    }

    /// Refresh an existing session against the MWA ASVO v2 API. The refresh
    /// endpoint's response (`TokenResponse`) doesn't include user info, so
    /// we carry it over from the session being refreshed - it's still the
    /// same account.
    fn refresh(auth_client: &Client, previous: &StoredTokens) -> Result<StoredTokens, Apiv2Error> {
        let response = auth_client
            .post(format!("{}/api/v2/refresh", get_asvo_server_address()))
            .header(
                reqwest::header::COOKIE,
                format!("mwa_refresh_token={}", previous.refresh_token),
            )
            .send()?;

        if !response.status().is_success() {
            return Err(Apiv2Error::AuthenticationFailed {
                message: response.text().unwrap_or_default(),
            });
        }

        let body = response.text()?;
        debug!("MWA ASVO v2 refresh response body: {}", body);
        let token: TokenResponse = serde_json::from_str(&body)?;

        Self::stored_tokens_from_parts(
            token.access_token,
            token.refresh_token,
            previous.user_id,
            previous.user_login.clone(),
            previous.user_email.clone(),
        )
    }

    /// Build a `StoredTokens` from a login response's pieces, decoding each
    /// JWT's `exp` claim to determine its expiry (the API response doesn't
    /// provide expiry timestamps directly).
    fn stored_tokens_from_auth(
        access_token: String,
        refresh_token: String,
        user: UserResponse,
    ) -> Result<StoredTokens, Apiv2Error> {
        // `StoredTokens.user_id` is a u64 (it's shared with mwa-cli's token
        // cache format), but the generated `UserResponse.id` is an i64.
        // A negative user ID from the server would be unexpected, but
        // let's not panic or silently wrap if it ever happened.
        let user_id = u64::try_from(user.id).map_err(|_| Apiv2Error::AuthenticationFailed {
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
    ) -> Result<StoredTokens, Apiv2Error> {
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

    /// Fetch the caller's MWA ASVO jobs via the v2 API, returning the same
    /// `AsvoJobVec` the v1 client's `get_jobs` does, so the rest of
    /// giant-squid (filtering, `--json`, table rendering) doesn't need to
    /// change.
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
    pub fn get_jobs(&self, days: Option<i64>) -> Result<AsvoJobVec, Apiv2Error> {
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

            // ASSUMPTION: guessing POST to /api/v2/job_history (matching
            // the other v2 endpoints' POST+JSON convention, and the
            // schema's doc comment naming this endpoint) since we don't
            // have the real `paths` section to confirm the method or path.
            let response = self
                .client
                .post(format!("{}/api/v2/get_jobs", get_asvo_server_address()))
                .json(&request)
                .send()?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().unwrap_or_default();
                return Err(match serde_json::from_str::<ErrorResponse>(&body) {
                    Ok(err) => Apiv2Error::ApiError {
                        error_code: err.error_code,
                        message: err.message,
                        detail: err.detail,
                        suggestion: err.suggestion,
                    },
                    Err(_) => Apiv2Error::BadStatus {
                        code: status,
                        message: body,
                    },
                });
            }

            let body = response.text()?;
            debug!("MWA ASVO v2 get_jobs response body: {}", body);
            let page: JobsByUserResponse = serde_json::from_str(&body)?;

            let page_len = page.jobs.len() as u64;
            for job_value in page.jobs {
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
}

/// Best-effort conversion from the v2 API's `JobDetailResponse` into the
/// existing `AsvoJob` domain type. Returns `None` (after logging a warning)
/// if a job can't be reliably converted; callers should skip that job and
/// continue rather than fail the whole listing.
///
/// - `job_type` codes we don't recognise become `AsvoJobType::Unknown`
///   (existing forward-compat behaviour, never fails).
/// - `job_state` is parsed via the existing `AsvoJobState::FromStr`.
///   ASSUMPTION: `JobDetailResponse.job_state`'s doc comment describes it
///   as a numeric code, but the field is typed `String` - I'm guessing
///   it's actually the same string vocabulary as
///   `JobsByUserRequestJobState` ("queued", "completed", etc), which is
///   what `AsvoJobState::from_str` already parses. If that's wrong, jobs
///   will be skipped with a warning naming the unrecognised value, rather
///   than silently misrepresenting the job's state.
/// - `obsid` is looked for at `job_params["obs_id"]` (an untyped JSON
///   map). ASSUMPTION: guessing the key name and that it's a JSON number,
///   by analogy with every other `obs_id` field in this schema - not
///   confirmed against a real response.
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

    let obsid = match detail.job_params.get("obs_id").and_then(|v| v.as_u64()) {
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

    let jtype = match detail.job_type {
        0 => AsvoJobType::Conversion,
        1 => AsvoJobType::DownloadVisibilities,
        2 => AsvoJobType::DownloadMetadata,
        3 => AsvoJobType::DownloadVoltage,
        4 => AsvoJobType::CancelJob,
        5 => AsvoJobType::DownloadBeamformer,
        6 => AsvoJobType::Imaging,
        _ => AsvoJobType::Unknown,
    };

    let state = match AsvoJobState::from_str(&detail.job_state) {
        Ok(state) => state,
        Err(_) => {
            warn!(
                "Skipping MWA ASVO job {}: unrecognised job_state {:?}",
                jobid, detail.job_state
            );
            return None;
        }
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
