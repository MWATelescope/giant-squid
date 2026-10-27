// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A client for the MWA ASVO v2 API.
//!
//! Every request and reply is a type generated from the MWA ASVO OpenAPI
//! schema (see `super::openapi`). The client logs in with the API key
//! (`login`), refreshes the access token before it expires (`refresh`,
//! `get_valid_tokens`; the JWT `exp` claim says when), and keeps the session
//! on disk through `crate::mwa_asvo::token_store`, so that the next process
//! does not log in again.
//!
//! The client reads no environment variables: the caller supplies the host,
//! API key, timeout and token cache path in an [`AsvoClientConfig`].

use std::num::NonZeroU64;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use base64::Engine;
use jiff::Timestamp;
use log::{debug, trace, warn};
use reqwest::blocking::{Client, ClientBuilder};
use reqwest::header::{HeaderMap, HeaderValue};

use crate::mwa_asvo::download::{download_by_job_id, download_by_obs_id};
use crate::mwa_asvo::redact::{redact_body, redact_parsed_url};
use crate::mwa_asvo::retry::{sleep_unless_stopped, Backoff};
use crate::mwa_asvo::token_store::{self, StoredTokens};
use crate::mwa_asvo::{AsvoError, AsvoJob, AsvoJobId, AsvoJobVec, DownloadOptions};
use crate::obs_id::ObsId;

use super::error::AsvoApiError;
use super::job_params::PreprocessJobParams;
use super::openapi::{
    ApiLoginRequest, ApiLoginResponse, DownloadJobParams, ErrorResponse, JobCancelledResponse,
    JobDetailResponse, JobState, JobSubmittedResponse, JobType, JobsByUserRequest, Login, SortBy,
    TokenResponse, UserResponse,
};
use super::validate::{self, validate_download_params};

/// The production MWA ASVO host. Callers that do not need a different
/// server (for example a test or development instance) use this as
/// [`AsvoClientConfig::host`].
pub const DEFAULT_ASVO_HOST: &str = "https://asvo.mwatelescope.org:443";

/// The default timeout for a single MWA ASVO API request.
pub const DEFAULT_API_TIMEOUT: Duration = Duration::from_secs(60);

/// The URL scheme that permits plain HTTP. See [`require_tls`].
const PLAIN_HTTP_SCHEME: &str = "http://";

/// User-agent string sent on every request to the MWA ASVO.
const APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

/// The client version that the login sends: the MWA ASVO requires it.
const CLIENT_VERSION: &str = concat!("giant-squidv", env!("CARGO_PKG_VERSION"));

/// Error codes the MWA ASVO returns when our access token is missing or
/// rejected. These are recoverable: logging in again gets us a new token.
/// In particular this covers a cached token that is still unexpired locally
/// but was minted by a different environment (e.g. dev vs test, which sign
/// JWTs with different secrets), so the other environment rejects it.
const AUTH_ERROR_CODES: [&str; 2] = ["AUTH_INVALID_TOKEN", "AUTH_REQUIRED"];

/// A path of the schema under [`API_PREFIX`]: the prefix is written only
/// here.
macro_rules! api_path {
    ($path:literal) => {
        concat!("/api", $path)
    };
}

/// Where the MWA ASVO serves its API: the schema's paths (for example
/// `/v2/jobs/list`) are under this prefix, which the schema does not give
/// (the API is mounted there behind the web server).
pub const API_PREFIX: &str = api_path!("");

// The MWA ASVO API endpoints this client uses: the schema's paths, under
// `API_PREFIX`. The login, refresh and job list endpoints are used only
// here.
const ENDPOINT_API_LOGIN: &str = api_path!("/v2/auth/api_login");
const ENDPOINT_REFRESH: &str = api_path!("/v2/auth/refresh");
const ENDPOINT_LIST_JOBS: &str = api_path!("/v2/jobs/list");
// The job and submission endpoints are public, so that the CLI can name the
// endpoint that a `--dry-run` submission or cancellation would go to,
// without a copy of the path.
pub const ENDPOINT_JOBS: &str = api_path!("/v2/jobs");
// The two endpoints that create a job: one for the download jobs
// (visibility, metadata, voltage and beamformer) and one for the
// preprocessing jobs (conversion, imaging, imaging from a job).
pub const ENDPOINT_DOWNLOAD_JOB: &str = api_path!("/v2/jobs/download");
pub const ENDPOINT_PREPROCESS_JOB: &str = api_path!("/v2/jobs/preprocess");

/// Everything an [`AsvoClient`] needs to connect to the MWA ASVO.
///
/// The library reads no environment variables. A caller (such as the
/// giant-squid CLI) finds these values and builds the config.
#[derive(Debug, Clone)]
pub struct AsvoClientConfig {
    /// The MWA ASVO base URL, for example [`DEFAULT_ASVO_HOST`]. An
    /// `http://` host is permitted (a local mock server or a plain-HTTP
    /// development instance); any other host must use TLS.
    pub host: String,
    /// The user's MWA ASVO API key.
    pub api_key: String,
    /// The timeout for a single API request. It must be more than zero:
    /// [`AsvoClient::new`] refuses zero.
    pub api_timeout: Duration,
    /// Where to cache the session tokens between runs. `None` keeps the
    /// session in memory only, so every new client logs in again. A cached
    /// session is used only by a client with the same `host` and `api_key`
    /// that it was made for; any other session in the file is replaced by
    /// a fresh login. Use
    /// [`default_token_cache_path`](crate::default_token_cache_path) for
    /// the same place that the `giant-squid` command uses.
    pub token_cache_path: Option<PathBuf>,
}

impl AsvoClientConfig {
    /// A config for `host` and `api_key`, with the default timeout
    /// ([`DEFAULT_API_TIMEOUT`]) and no token cache.
    pub fn new(host: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            api_key: api_key.into(),
            api_timeout: DEFAULT_API_TIMEOUT,
            token_cache_path: None,
        }
    }
}

/// The authenticated HTTP client, and a count of how many times it has been
/// replaced after a rejected token.
///
/// A thread records the generation of the client it sends a request with.
/// If the server rejects that token, [`AsvoClient::reauthenticate`] compares
/// that generation with the current one. If they differ, another thread
/// has already logged in again, so this thread reuses the new client.
#[derive(Debug)]
struct Session {
    client: Client,
    generation: u64,
}

#[derive(Debug)]
pub struct AsvoClient {
    /// The `reqwest` [Client] used to interface with the MWA ASVO v2 API.
    ///
    /// Wrapped in a [`Mutex`] so that, if the server rejects our access
    /// token (`AUTH_INVALID_TOKEN` / `AUTH_REQUIRED`), `send_authed` can
    /// re-authenticate and swap in a fresh client mid-flight, and so that
    /// an `AsvoClient` is `Sync` and can be shared between threads (the
    /// Python bindings need this). The lock is held only to clone or swap
    /// the client (see [`Self::current_session`]), never during a request,
    /// so requests from different threads run at the same time.
    session: Mutex<Session>,
    /// Held for the whole of a re-login, so that when several threads have
    /// the same token rejected at the same time, only the first one logs
    /// in again. The others wait, then reuse its token. This matters
    /// because the server permits only a few logins a minute.
    login_lock: Mutex<()>,
    /// The connection details. Kept so that a fresh login can be done on
    /// demand (i.e. when the current token is rejected), and so that every
    /// request uses the same host.
    config: AsvoClientConfig,
    /// The HTTP client for the files of a job. It sends no access token:
    /// the files are on another host (Acacia), and their URLs are signed.
    download_client: Client,
}

/// A minimal view of an HTTP response - the pieces callers need once the
/// body has been read and logged centrally by [`execute_logged`].
struct HttpResponse {
    status: reqwest::StatusCode,
    body: String,
}

/// How [`execute_logged`] logs the bodies of a request and its response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BodyLogging {
    /// Log both bodies at trace level, with the URLs in them redacted (a
    /// signed URL of a file of a job is a credential).
    UrlsRedacted,
    /// Log no body, because a body holds a credential: the login request
    /// has the API key, and the login and refresh responses have the access
    /// and refresh tokens.
    Hidden,
}

/// The text that a trace log has in place of a body that is not logged.
const BODY_HIDDEN: &str = "<not logged: it holds credentials>";

/// Execute `builder` on `client`, logging one concise line for the request
/// and one for the response at debug level (i.e. `-v`): the HTTP method,
/// URL, and (for the response) the status. The bodies are logged at trace
/// level (`-vv`) only, as `body_logging` says: a body with a credential in
/// it is never logged. Reading the body is centralised here so every caller
/// gets consistent logging in one place.
fn execute_logged(
    client: &Client,
    builder: reqwest::blocking::RequestBuilder,
    body_logging: BodyLogging,
) -> reqwest::Result<HttpResponse> {
    let request = builder.build()?;
    let method = request.method().clone();
    let url = request.url().clone();
    debug!("HTTP request:  {method} {}", redact_parsed_url(&url));

    if let Some(body) = request.body().and_then(|b| b.as_bytes()) {
        match body_logging {
            BodyLogging::UrlsRedacted => {
                trace!(
                    "HTTP request body: {}",
                    redact_body(&String::from_utf8_lossy(body))
                );
            }
            BodyLogging::Hidden => trace!("HTTP request body: {BODY_HIDDEN}"),
        }
    }

    let response = client.execute(request)?;
    let status = response.status();
    debug!(
        "HTTP response: {method} {} -> {status}",
        redact_parsed_url(&url)
    );

    let body = response.text()?;
    match body_logging {
        BodyLogging::UrlsRedacted => trace!("HTTP response body: {}", redact_body(&body)),
        BodyLogging::Hidden => trace!("HTTP response body: {BODY_HIDDEN}"),
    }

    Ok(HttpResponse { status, body })
}

/// Decode a JWT's payload (without verifying its signature - we're only
/// reading the `exp` claim to know when a token we've already been handed
/// by the server will expire, not authenticating anything with it) and
/// return its expiry as a UTC timestamp.
fn decode_jwt_exp(token: &str) -> Result<Timestamp, AsvoApiError> {
    #[derive(serde::Deserialize)]
    struct JwtExpClaim {
        exp: i64,
    }

    let payload_b64 = token.split('.').nth(1).ok_or_else(|| {
        AsvoApiError::authentication_failed(
            "Malformed JWT returned by MWA ASVO: no payload segment",
        )
    })?;

    let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64)
        .map_err(|e| {
            AsvoApiError::authentication_failed(format!(
                "Could not base64-decode JWT payload from MWA ASVO: {e}"
            ))
        })?;

    let claim: JwtExpClaim = serde_json::from_slice(&payload_bytes).map_err(|e| {
        AsvoApiError::authentication_failed(format!(
            "Could not parse JWT payload JSON from MWA ASVO: {e}"
        ))
    })?;

    Timestamp::from_second(claim.exp).map_err(|_| {
        AsvoApiError::authentication_failed("JWT `exp` claim from MWA ASVO was out of range")
    })
}

/// Whether HTTP requests must use TLS: always, except for an explicit
/// `http://` host (a local mock server, or a plain-HTTP dev instance).
fn require_tls(host: &str) -> bool {
    !host.starts_with(PLAIN_HTTP_SCHEME)
}

impl AsvoClient {
    /// Get a new client which has authenticated with the MWA ASVO v2 API,
    /// using the host, API key and timeout in `config`.
    ///
    /// If `config.token_cache_path` is set, a cached session there (the
    /// format is shared with mwa-cli) is reused if it's still valid,
    /// refreshed if only the access token has expired, or a fresh login is
    /// performed otherwise. This is all best-effort and silent: if the
    /// cache can't be read or written for any reason, the client just falls
    /// back to a fresh login. With no cache path, the client always logs in.
    ///
    /// # Errors
    ///
    /// [`AsvoError::InvalidOption`] if `config.api_timeout` is zero (every
    /// request would time out). [`AsvoError::AsvoApi`] with
    /// [`AsvoApiError::MissingAuthKey`] if `config.api_key` is empty, or
    /// with the error of the login.
    pub fn new(config: AsvoClientConfig) -> Result<AsvoClient, AsvoError> {
        if config.api_timeout.is_zero() {
            return Err(AsvoError::InvalidOption {
                name: "api_timeout",
                message: "must be more than 0 seconds".to_string(),
            });
        }
        if config.api_key.is_empty() {
            return Err(AsvoApiError::MissingAuthKey { variable: None }.into());
        }

        // If we are using a non-default MWA ASVO host, then upgrade this
        // debug message to a warn message.
        let host = &config.host;
        if config.host == DEFAULT_ASVO_HOST {
            debug!("Connecting to MWA ASVO... {host}");
        } else {
            warn!("Connecting to MWA ASVO non-default host: {host}...");
        }

        debug!("User Agent string: {APP_USER_AGENT}");

        // Figure out which access token we're going to use: a cached one
        // (as-is, or refreshed), or a fresh login. Whichever path we take,
        // we end up with a valid `StoredTokens` to authenticate with.
        let tokens = Self::get_valid_tokens(&config)?;

        // Build the "real" client, with the access token attached as a
        // default header on every request. If the server later rejects this
        // token, `send_authed` re-logs-in and swaps in a new client, which
        // is why we hold on to the config below.
        let client = Self::build_authed_client(&config, &tokens.access_token)?;
        let download_client = Self::build_plain_client(&config)?;

        Ok(AsvoClient {
            session: Mutex::new(Session {
                client,
                generation: 0,
            }),
            login_lock: Mutex::new(()),
            config,
            download_client,
        })
    }

    /// Build a `reqwest` [Client] that sends `access_token` as the
    /// `mwa_access_token` cookie on every request. Factored out of `new` so
    /// `reauthenticate` can rebuild the client with a freshly-minted token.
    ///
    /// The client keeps no cookie jar: the token is attached explicitly, so
    /// a session loaded from the cache behaves the same as one from a fresh
    /// login.
    fn build_authed_client(
        config: &AsvoClientConfig,
        access_token: &str,
    ) -> Result<Client, AsvoApiError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::COOKIE,
            HeaderValue::from_str(&format!("mwa_access_token={access_token}")).map_err(|e| {
                AsvoApiError::authentication_failed(format!(
                    "MWA ASVO returned an invalid access token: {e}"
                ))
            })?,
        );

        // Not `connection_verbose`: it logs every byte on the wire at trace
        // level, with the cookies, the API key and the signed URLs in them.
        // `execute_logged` is the log of the requests.
        Ok(ClientBuilder::new()
            .user_agent(APP_USER_AGENT)
            .https_only(require_tls(&config.host))
            .default_headers(headers)
            .timeout(config.api_timeout)
            .build()?)
    }

    /// Build a [Client] that sends no access token: for the login and
    /// refresh calls, and for the files of a job. The access token must
    /// not go to the host of a file (Acacia).
    fn build_plain_client(config: &AsvoClientConfig) -> Result<Client, AsvoApiError> {
        Ok(ClientBuilder::new()
            .user_agent(APP_USER_AGENT)
            .https_only(require_tls(&config.host))
            .timeout(config.api_timeout)
            .build()?)
    }

    /// Lock the session. A poisoned lock is not an error here: the lock
    /// only guards the swap of one [`Session`] value for another, so a
    /// panic in another thread cannot leave it half-changed.
    fn lock_session(&self) -> MutexGuard<'_, Session> {
        self.session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// A clone of the current authenticated HTTP client, and its
    /// generation. Cloning a reqwest client is cheap (it's reference-counted
    /// internally), and a clone lets the caller send requests without
    /// holding the lock.
    fn current_session(&self) -> (Client, u64) {
        let session = self.lock_session();
        (session.client.clone(), session.generation)
    }

    /// Download the MWA ASVO job with the given Job ID.
    /// Fetches the current job list, locates the job, and downloads its
    /// files according to the supplied options. Returns the job that was
    /// downloaded. To download several jobs, get the job list once and use
    /// [`Self::download_job_from`].
    pub fn download_job(
        &self,
        job_id: AsvoJobId,
        opts: &DownloadOptions,
    ) -> Result<AsvoJob, AsvoError> {
        opts.check()?; // before the job list request
        let jobs = self.get_jobs(&JobsFilter::default())?;
        self.download_job_from(&jobs, job_id, opts)
    }

    /// As [`Self::download_job`], with the job looked up in `jobs` (a job
    /// list from [`Self::get_jobs`]) instead of a new
    /// job list request.
    pub fn download_job_from(
        &self,
        jobs: &AsvoJobVec,
        job_id: AsvoJobId,
        opts: &DownloadOptions,
    ) -> Result<AsvoJob, AsvoError> {
        download_by_job_id(&self.download_client, jobs, job_id, opts)
    }

    /// Download the MWA ASVO job associated with the given Obs ID.
    /// Fetches the current job list, locates the single ready job for
    /// the Obs ID, and downloads its files according to the supplied options.
    /// Returns the job that was downloaded (so the caller learns its Job
    /// ID). To download several jobs, get the job list once and use
    /// [`Self::download_obs_from`].
    pub fn download_obs(
        &self,
        obs_id: ObsId,
        opts: &DownloadOptions,
    ) -> Result<AsvoJob, AsvoError> {
        opts.check()?; // before the job list request
        let jobs = self.get_jobs(&JobsFilter::default())?;
        self.download_obs_from(&jobs, obs_id, opts)
    }

    /// As [`Self::download_obs`], with the job looked up in `jobs` instead
    /// of a new job list request.
    pub fn download_obs_from(
        &self,
        jobs: &AsvoJobVec,
        obs_id: ObsId,
        opts: &DownloadOptions,
    ) -> Result<AsvoJob, AsvoError> {
        download_by_obs_id(&self.download_client, jobs, obs_id, opts)
    }

    /// Returns a valid, ready-to-use `StoredTokens`, preferring (in order):
    /// a still-valid cached session, a refreshed cached session, or a fresh
    /// login. With a token cache path in `config`, successful refreshes and
    /// logins are cached to disk for next time (best-effort; failure to
    /// cache is not fatal).
    fn get_valid_tokens(config: &AsvoClientConfig) -> Result<StoredTokens, AsvoApiError> {
        // A short-lived client, used only to perform the login/refresh call
        // itself (it doesn't need the auth headers).
        let auth_client = Self::build_plain_client(config)?;

        let cached = config
            .token_cache_path
            .as_deref()
            .and_then(token_store::load)
            .filter(|cached| {
                let belongs = cached.belongs_to(&config.host, &config.api_key);
                if !belongs {
                    debug!(
                        "Ignoring the cached MWA ASVO session: it is for another host or API key, or in the old format"
                    );
                }
                belongs
            });
        if let Some(cached) = cached {
            if cached.is_access_valid() {
                debug!("Reusing cached MWA ASVO session (shared with mwa-cli)");
                return Ok(cached);
            }

            if cached.is_refresh_valid() {
                debug!("Cached MWA ASVO access token expired; refreshing session");
                match Self::refresh(&auth_client, &config.host, &cached) {
                    Ok(refreshed) => {
                        Self::cache_tokens(config, &refreshed);
                        return Ok(refreshed);
                    }
                    Err(e) => {
                        // Refresh can fail legitimately (e.g. the refresh
                        // token was already rotated by another process, or
                        // by mwa-cli). Fall through to a fresh login rather
                        // than treating this as fatal.
                        debug!("MWA ASVO session refresh failed, falling back to fresh login: {e}");
                    }
                }
            }
        }

        debug!("Performing fresh MWA ASVO login");
        let fresh = Self::login(&auth_client, config)?;
        Self::cache_tokens(config, &fresh);
        Ok(fresh)
    }

    /// Save `tokens` to the token cache in `config`, if it has one.
    fn cache_tokens(config: &AsvoClientConfig, tokens: &StoredTokens) {
        match &config.token_cache_path {
            Some(path) => token_store::save(path, tokens),
            None => {
                debug!("No token cache configured; keeping the MWA ASVO session in memory only")
            }
        }
    }

    /// Perform a fresh login against the MWA ASVO v2 API using the API key.
    fn login(
        auth_client: &Client,
        config: &AsvoClientConfig,
    ) -> Result<StoredTokens, AsvoApiError> {
        let login: Login = CLIENT_VERSION.try_into()?;

        let host = &config.host;
        let response = execute_logged(
            auth_client,
            auth_client
                .post(format!("{host}{ENDPOINT_API_LOGIN}"))
                .json(&ApiLoginRequest {
                    login,
                    password: config.api_key.clone(),
                }),
            BodyLogging::Hidden,
        )?;

        if !response.status.is_success() {
            return Err(Self::auth_error_from_body(response.body));
        }

        let auth: ApiLoginResponse = serde_json::from_str(&response.body)?;

        let tokens =
            Self::stored_tokens_from_auth(auth.access_token, auth.refresh_token, auth.user)?;
        Ok(StoredTokens {
            host: config.host.clone(),
            api_key_sha256: token_store::api_key_sha256(&config.api_key),
            ..tokens
        })
    }

    /// Refresh an existing session against the MWA ASVO v2 API. The refresh
    /// endpoint's response (`TokenResponse`) doesn't include user info, so
    /// we carry it over from the session being refreshed - it's still the
    /// same account.
    fn refresh(
        auth_client: &Client,
        host: &str,
        previous: &StoredTokens,
    ) -> Result<StoredTokens, AsvoApiError> {
        let refresh_token = &previous.refresh_token;
        let response = execute_logged(
            auth_client,
            auth_client
                .post(format!("{host}{ENDPOINT_REFRESH}"))
                .header(
                    reqwest::header::COOKIE,
                    format!("mwa_refresh_token={refresh_token}"),
                ),
            BodyLogging::Hidden,
        )?;

        if !response.status.is_success() {
            return Err(Self::auth_error_from_body(response.body));
        }

        let token: TokenResponse = serde_json::from_str(&response.body)?;

        let tokens = Self::stored_tokens_from_parts(
            token.access_token,
            token.refresh_token,
            previous.user_id,
            previous.user_login.clone(),
            previous.user_email.clone(),
        )?;
        Ok(StoredTokens {
            host: previous.host.clone(),
            api_key_sha256: previous.api_key_sha256.clone(),
            ..tokens
        })
    }

    /// Force a fresh login (ignoring any cached session), persist the new
    /// tokens, and swap the freshly-authenticated client in as our active
    /// one. Called by `send_authed` when the server rejects the token of
    /// the client with generation `rejected_generation`.
    ///
    /// Only one thread logs in at a time (see `login_lock`). If the
    /// generation has changed by the time this thread holds the lock,
    /// another thread has already logged in again, so this thread does not
    /// log in and the caller uses the new client.
    fn reauthenticate(&self, rejected_generation: u64) -> Result<(), AsvoApiError> {
        let _login = self
            .login_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        if self.lock_session().generation != rejected_generation {
            debug!("Another thread has already logged in to MWA ASVO again; reusing its token");
            return Ok(());
        }

        debug!("Re-authenticating with MWA ASVO after a rejected access token");
        let auth_client = Self::build_plain_client(&self.config)?;
        let fresh = Self::login(&auth_client, &self.config)?;
        Self::cache_tokens(&self.config, &fresh);
        let new_client = Self::build_authed_client(&self.config, &fresh.access_token)?;

        let mut session = self.lock_session();
        session.client = new_client;
        session.generation = session.generation.wrapping_add(1);
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
        let user_id = u64::try_from(user.id).map_err(|_| {
            let negative_id = user.id;
            AsvoApiError::authentication_failed(format!(
                "MWA ASVO returned an invalid (negative) user ID: {negative_id}"
            ))
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
            host: String::new(),
            api_key_sha256: String::new(),
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
        // Clone the client out of the Mutex rather than holding the lock
        // across the (blocking) send, so `reauthenticate` is free to take
        // the lock on the retry path, and other threads are not blocked.
        let (client, generation) = self.current_session();
        let response = execute_logged(&client, build(&client), BodyLogging::UrlsRedacted)?;
        if response.status.is_success() {
            return Ok(response.body);
        }

        let err = Self::error_from_body(response.status, response.body);
        if !Self::is_auth_error(&err) {
            return Err(err);
        }

        // Token rejected: re-login once (or reuse another thread's re-login)
        // and retry the request against the freshly-swapped client.
        self.reauthenticate(generation)?;
        let (client, _) = self.current_session();
        let response = execute_logged(&client, build(&client), BodyLogging::UrlsRedacted)?;
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
            Ok(err) => AsvoApiError::api_error(status.as_u16(), err),
            Err(_) => AsvoApiError::BadStatus {
                status: status.as_u16(),
                message: body,
            },
        }
    }

    /// Map the body of a failed login or refresh to an
    /// [`AsvoApiError::AuthenticationFailed`]: the fields of a structured
    /// `ErrorResponse`, verbatim, or else the body as the message.
    fn auth_error_from_body(body: String) -> AsvoApiError {
        match serde_json::from_str::<ErrorResponse>(&body) {
            Ok(err) => AsvoApiError::authentication_failed_with(err),
            Err(_) => AsvoApiError::authentication_failed(body),
        }
    }

    /// Whether `err` is an authentication failure that a fresh login might
    /// fix (see [`AUTH_ERROR_CODES`]).
    fn is_auth_error(err: &AsvoApiError) -> bool {
        matches!(
            err,
            AsvoApiError::ApiError { response, .. }
                if AUTH_ERROR_CODES.contains(&response.error_code.as_str())
        )
    }

    /// Fetch the caller's MWA ASVO jobs, all pages of them, as `giant-squid
    /// list` does. `filter` selects the jobs, and how, as [`JobsFilter`]
    /// says; [`JobsFilter::default()`] gets the jobs that the API returns by
    /// default. Where the filter does not say otherwise, `days` is the
    /// schema's default (30 in the current schema): the library does not ask
    /// for "all" jobs. With a date, the request has `"days": null`.
    ///
    /// A job without a usable obs_id in its untyped `job_params` is skipped
    /// with a warning, rather than failing the whole listing (see
    /// `AsvoJob::try_from`). A job state or file type that the schema
    /// does not list fails the whole listing with a decode error.
    ///
    /// # Errors
    ///
    /// [`AsvoApiError::InvalidParameter`] before any request, if the filter
    /// is refused ([`JobsFilter::validate`]); otherwise the error from the
    /// request.
    pub fn get_jobs(&self, filter: &JobsFilter) -> Result<AsvoJobVec, AsvoApiError> {
        filter.validate()?;
        let days = filter.days;
        let dates = filter.date_from.is_some() || filter.date_to.is_some();
        // The server takes one state and one type.
        let job_state = single(&filter.job_states);
        let job_type = single(&filter.job_types);

        let mut all_jobs = Vec::new();
        let mut offset: u64 = 0;

        loop {
            // The builder starts from the schema defaults (the page size,
            // `limit`, is the schema's). `days` keeps its default unless the
            // filter has `days`, or has a date: then `days` is `null`, so
            // that it does not limit the dates.
            let mut builder = JobsByUserRequest::builder()
                .job_state(job_state)
                .job_type(job_type)
                .date_from(filter.date_from)
                .date_to(filter.date_to)
                .offset(offset);
            if dates {
                builder = builder.days(None::<NonZeroU64>);
            } else if let Some(days) = days {
                builder = builder.days(days);
            }
            if let Some(sort_by) = &filter.sort_by {
                builder = builder.sort_by(*sort_by);
            }
            let request: JobsByUserRequest = builder.try_into()?;

            // The job list is a POST to /api/v2/jobs/list.
            let host = &self.config.host;
            let body = self.send_authed(|client| {
                client
                    .post(format!("{host}{ENDPOINT_LIST_JOBS}"))
                    .json(&request)
            })?;
            // Not parsed as `JobsByUserResponse` at once: since schema
            // v1.11 its `jobs` are typed, and `normalize_job_value` must
            // patch two server quirks into each job first. The page shape
            // is still the schema's.
            let page: RawJobsPage = serde_json::from_str(&body)?;

            let page_len = page.jobs.len() as u64;
            for mut job_value in page.jobs {
                normalize_job_value(&mut job_value);

                // Hard error: this is a basic structural mismatch against
                // the schema (an item that isn't even a JobDetailResponse
                // shape), not a content-level problem like the ones that
                // `AsvoJob::try_from` finds, which skip only that job.
                let detail: JobDetailResponse =
                    serde_json::from_value(serde_json::Value::Object(job_value))?;
                // A job that cannot be used (see `AsvoJob::try_from`) is
                // skipped, so that one bad job does not hide the others.
                match AsvoJob::try_from(detail) {
                    Ok(job) => all_jobs.push(job),
                    Err(e) => warn!("Skipping a job: {e}"),
                }
            }

            offset += page_len;
            let total_count = u64::try_from(page.total_count).unwrap_or(0);
            if page_len == 0 || offset >= total_count {
                break;
            }
        }

        Ok(AsvoJobVec(all_jobs).filter(
            &filter.job_ids,
            &filter.obs_ids,
            &filter.job_types,
            &filter.job_states,
        ))
    }

    /// As [`Self::get_jobs`], and when the request fails in a way that may
    /// pass (see [`AsvoApiError::is_transient`]), try again after a wait
    /// that grows (see the retry policy of the downloads). It gives up with
    /// the last error when the failures have gone on for `retry_duration`,
    /// and at once for any other error. A zero `retry_duration` never
    /// retries. It is for a long task that polls the job list, which is the
    /// `wait` command: one failure should not end the task. It is not public,
    /// so [`Self::get_jobs`] is the one public way to get the job list. A
    /// caller of the library that wants retries makes its own loop around
    /// [`Self::get_jobs`], with [`AsvoApiError::is_transient`].
    ///
    /// `should_stop` is asked during each wait. When it returns `true`,
    /// the call ends with [`AsvoError::Interrupted`]. `None` never stops.
    ///
    /// # Errors
    ///
    /// [`AsvoError::AsvoApi`] with the error from the request, as for
    /// [`Self::get_jobs`], and [`AsvoError::Interrupted`].
    // Only the CLI (the "bin" feature) calls it, so a build without that
    // feature does not use it.
    #[cfg_attr(not(feature = "bin"), allow(dead_code))]
    pub(crate) fn get_jobs_retrying(
        &self,
        filter: &JobsFilter,
        retry_duration: Duration,
        should_stop: Option<&dyn Fn() -> bool>,
    ) -> Result<AsvoJobVec, AsvoError> {
        let mut backoff = Backoff::new(retry_duration);
        loop {
            let err = match self.get_jobs(filter) {
                Ok(jobs) => return Ok(jobs),
                Err(err) => err,
            };
            if !err.is_transient() {
                return Err(err.into());
            }
            let Some(wait) = backoff.next_backoff() else {
                return Err(err.into());
            };
            warn!("Could not get the job list; trying again in {wait:?}: {err}");
            sleep_unless_stopped(wait, should_stop)?;
        }
    }

    /// Submit a download job: a visibility, metadata, voltage or beamformer
    /// job. The MWA ASVO finds the kind of download from the observation
    /// (and `only_get_metadata`). Like the other submit endpoint, a success
    /// returns a `JobSubmittedResponse` carrying the new job's ID.
    ///
    /// The delivery and its format, and the `offset` limits, are checked
    /// first ([`validate_download_params`]); nothing is sent if one is
    /// wrong, and the error is [`AsvoApiError::InvalidParameter`]. The
    /// request body does not say that it is a voltage job, so the extra
    /// rules of a voltage job are not checked here: call
    /// [`validate_voltage_params`](super::validate::validate_voltage_params)
    /// first for a voltage job.
    pub fn submit_download_job(
        &self,
        params: &DownloadJobParams,
    ) -> Result<JobSubmittedResponse, AsvoApiError> {
        validate_download_params(params)?;
        debug!("Submitting a download job to MWA ASVO");

        self.submit(ENDPOINT_DOWNLOAD_JOB, params)
    }

    /// Submit a preprocessing job: a conversion job, an imaging job from an
    /// Obs ID (flow 1) or an imaging job from a conversion job (flow 2).
    ///
    /// The body is checked against the schema's limits first
    /// ([`PreprocessJobParams::validate`]); nothing is sent if a value is
    /// wrong, and the error is [`AsvoApiError::InvalidParameter`].
    pub fn submit_preprocess_job(
        &self,
        params: &PreprocessJobParams,
    ) -> Result<JobSubmittedResponse, AsvoApiError> {
        params.validate()?;
        debug!("Submitting a preprocess job to MWA ASVO");

        self.submit(ENDPOINT_PREPROCESS_JOB, params)
    }

    /// POST the job submission `params` to `endpoint`, and parse the reply.
    /// Every submit endpoint takes a JSON body and replies with a
    /// `JobSubmittedResponse`.
    fn submit(
        &self,
        endpoint: &str,
        params: &impl serde::Serialize,
    ) -> Result<JobSubmittedResponse, AsvoApiError> {
        let host = &self.config.host;
        let body =
            self.send_authed(|client| client.post(format!("{host}{endpoint}")).json(params))?;
        Ok(serde_json::from_str(&body)?)
    }

    /// Cancel a job. The reply is the schema's `JobCancelledResponse`.
    ///
    /// The reply's `status` and `message` describe what happened, and the
    /// `status` is not to be used to decide whether the call worked: the
    /// HTTP status decides that (the API developer's rule). The MWA ASVO
    /// answers the cancellation of a job that is already cancelled with a
    /// normal (HTTP 200) reply whose `status` is "failed" and whose message
    /// says so, so that case is `Ok` here. Read the `message`. Any other
    /// refusal is an HTTP 4xx error, which is an `Err` here.
    ///
    /// # Errors
    ///
    /// The error from the request, for example `JOB_NOT_FOUND` for a job
    /// that does not exist.
    pub fn cancel_job(&self, job_id: AsvoJobId) -> Result<JobCancelledResponse, AsvoApiError> {
        debug!("Cancelling MWA ASVO Job ID {job_id}");

        let host = &self.config.host;
        let body =
            self.send_authed(|client| client.delete(format!("{host}{ENDPOINT_JOBS}/{job_id}")))?;

        let resp: JobCancelledResponse = serde_json::from_str(&body)?;
        Ok(resp)
    }
}

/// Patches the quirks of the real server into a raw job JSON object before it
/// is read as a `JobDetailResponse`. The generated type cannot be fixed
/// (regeneration overwrites it), and the API developer has been told of the
/// first two (docs/PYTHON_BINDINGS.md): remove them when the API sends `null`
/// and RFC 3339 times.
///
/// 1. `completed`, `started` and `modified` are sometimes absent, not `null`.
///    The generated type has no `#[serde(default)]` for them, so an absent key
///    is an error. Each missing one is set to `null`.
/// 2. Times (those three and `created`) have no time zone, for example
///    `2026-09-08T05:41:54.757232`, and `jiff::Timestamp` needs one. `Z` is
///    appended (the times are UTC, as the schema's `date-time` says).
/// 3. Defensive, not seen from the real server: a `product` without `files`
///    (for example `{}`) would fail the whole listing, so it is replaced with
///    `null`, and the download reports `NoFiles` for that one job.
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

    if let Some(serde_json::Value::Object(product)) = job_value.get("product") {
        if !product.contains_key("files") {
            debug!("A job's product has no files; treating it as no product");
            job_value.insert("product".to_string(), serde_json::Value::Null);
        }
    }
}

/// The characters that name a time zone in an RFC 3339 time: `Z` or `z` for
/// UTC, and the sign of an offset.
const TIME_ZONE_MARKS: [char; 4] = ['Z', 'z', '+', '-'];

/// Does `s` look like an ISO8601 timestamp with no timezone designator?
/// Deliberately simple: skip the `YYYY-MM-DD` date portion (which has its
/// own `-` characters that would otherwise look like a negative UTC
/// offset), then check whether what's left names a zone at all.
fn looks_like_naive_timestamp(s: &str) -> bool {
    match s.get(10..) {
        Some(rest) => !rest.is_empty() && !rest.contains(TIME_ZONE_MARKS),
        None => false,
    }
}

/// Which jobs [`AsvoClient::get_jobs`] gets, as `giant-squid list` selects
/// them. An empty list, or a field that is `None`, does not filter, except
/// `days` and `sort_by`, which use the API's defaults (the past 30 days, and
/// the order `id`, in the current schema).
///
/// `days`, the dates, `sort_by` and, when a list has exactly one entry, that
/// state or type are filters of the MWA ASVO API (`JobsByUserRequest`) and are
/// sent to the server. The lists are also applied to the jobs that come back
/// ([`AsvoJobVec::filter`]), because the server takes one state and one type
/// at most. `days` and the dates are exclusive: the jobs are selected by one
/// or the other, and a filter with `days` and a date is refused
/// ([`JobsFilter::validate`]). With a date and no `days`, the request has
/// `"days": null`.
///
/// The states and types are the schema's `JobState` and `JobType`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobsFilter {
    /// Only the jobs from the past `days` days, from 1 to 30
    /// ([`validate::DAYS`]). `None` is the API's default.
    pub days: Option<NonZeroU64>,
    /// Only the jobs created at or after this time.
    pub date_from: Option<Timestamp>,
    /// Only the jobs created at or before this time.
    pub date_to: Option<Timestamp>,
    /// The column to sort the jobs by, for example `id`.
    pub sort_by: Option<SortBy>,
    /// Only these jobs. Cannot be combined with `obs_ids`.
    pub job_ids: Vec<AsvoJobId>,
    /// Only the jobs for these Obs IDs. Cannot be combined with `job_ids`.
    pub obs_ids: Vec<ObsId>,
    /// Only the jobs of these types.
    pub job_types: Vec<JobType>,
    /// Only the jobs in these states.
    pub job_states: Vec<JobState>,
}

impl JobsFilter {
    /// Check the filter before any request.
    ///
    /// # Errors
    ///
    /// [`AsvoApiError::InvalidParameter`] if both `job_ids` and `obs_ids` are
    /// given, if `days` is outside 1 to 30, or if `days` is given with
    /// `date_from` or `date_to`: the jobs are selected by `days` or by the
    /// dates, not by both.
    pub fn validate(&self) -> Result<(), AsvoApiError> {
        if !self.job_ids.is_empty() && !self.obs_ids.is_empty() {
            return Err(AsvoApiError::InvalidParameter {
                name: "job_ids",
                message: "can't specify both Job IDs and Obs IDs; use one or the other".to_string(),
            });
        }
        if let Some(days) = self.days {
            validate::check_days(days)?;
            if self.date_from.is_some() || self.date_to.is_some() {
                return Err(AsvoApiError::InvalidParameter {
                    name: "days",
                    message: "can't be used with date_from or date_to; use days or the dates"
                        .to_string(),
                });
            }
        }
        Ok(())
    }
}

/// The only entry of `items`: the filter that the server takes. With none or
/// several entries, the server does not filter.
fn single<T: Copy>(items: &[T]) -> Option<T> {
    match items {
        [one] => Some(*one),
        _ => None,
    }
}

/// One page of `get_jobs`, with each job left untyped so that
/// [`normalize_job_value`] can patch it before it is parsed as a
/// `JobDetailResponse`. The fields are those of `JobsByUserResponse`.
#[derive(serde::Deserialize)]
struct RawJobsPage {
    jobs: Vec<serde_json::Map<String, serde_json::Value>>,
    total_count: i64,
}

#[cfg(test)]
mod tests;
