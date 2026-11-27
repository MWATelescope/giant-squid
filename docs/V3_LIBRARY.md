# Rust library and Python API changes in 3.0.0

This page lists the changes in 3.0.0 to the `mwa_giant_squid` Rust library and to the new Python module
(`mwa-giant-squid`). For the changes to the `giant-squid` command, see [V3_MIGRATION.md](V3_MIGRATION.md).

More detail:

- The Rust library: the [README](../README.md#using-giant-squid-as-a-rust-library) and the API documentation of the
  crate.
- The Python module: [PYTHON.md](PYTHON.md) and the API reference, <https://mwatelescope.github.io/giant-squid/>.

The library uses version 2 of the MWA ASVO API (2.8.3, OpenAPI 3.1.0). Its methods, types, names, defaults and limits
come from the API's OpenAPI schema.

## Rust library: changed

### Environment and output

The library reads no environment variable and prints nothing. The `giant-squid` command reads its variables with
`client_config_from_env` and `DownloadSettings::from_env`. A failed download is reported once, by the caller: the
download no longer also logs it.

### Names and modules

- The public API is reorganised. The module `asvo` is `mwa_asvo`, and most items are also at the crate root (for
  example `mwa_giant_squid::MwaAsvoClient`).
- The names say "MWA ASVO", not "ASVO" (ASVO is the five data portals; the MWA ASVO is one of them):

  | 2.x | 3.0.0 |
  |---|---|
  | `AsvoClient` | `MwaAsvoClient` |
  | `AsvoJob` | `MwaAsvoJob` |
  | `AsvoJobVec` | `MwaAsvoJobVec` |
  | `AsvoError` | `MwaAsvoError` |
  | `AsvoJobID` | `MwaAsvoJobId` (a `NonZeroU64`) |
  | `Obsid`, `ObsidError` | `ObsId`, `ObsIdError` |

- New types: `MwaAsvoClientConfig`, `MwaAsvoJobId` and `MwaAsvoApiError`. The constant of the default host is
  `DEFAULT_MWA_ASVO_HOST`.
- `ObsId::MIN` and `ObsId::MAX` give the range of a valid Obs ID. `ObsId::MIN` is the API's minimum, 1056643216. A
  number with 10 digits is still read as an Obs ID, so a 10-digit number below the minimum is
  `ParseError::InvalidObsId`, not a Job ID.

### The client

- `MwaAsvoClient::new` takes an `MwaAsvoClientConfig`.
- `download_jobid` and `download_obsid` are `download_job` and `download_obs`. They return the `MwaAsvoJob` that was
  downloaded (they returned `()`). `download_job_from` and `download_obs_from` use a job list that the caller already
  has.
- `get_jobs` takes a `JobsFilter`.
- There are three submit methods, one for each submit endpoint of the API:
  - `submit_download_job` takes a `DownloadJobParams`: one body for a visibility, metadata, voltage or beamformer job
    (`/api/v2/jobs/download`).
  - `submit_preprocess_job` takes a `PreprocessJobParams`: a `ConversionJobParams` or an `ImagingJobParams`
    (`/api/v2/jobs/preprocess`).
  - `submit_image_from_job` takes an `ImageFromJobParams`, which has no Obs ID (`/api/v2/jobs/image_from_job`).
- The request bodies are made at the schema's defaults by `mwa_asvo::api::job_params` (`download_params`,
  `conversion_params`, `imaging_params` and `image_from_job_params`), and checked by `mwa_asvo::api::validate`.
- `MwaAsvoClient` authenticates against the MWA ASVO's v2 (JWT-based) login API, not HTTP Basic Auth. The session is cached at `$HOME/.mwa-asvo/tokens.json` (`default_token_cache_path`) and refreshed when it
  expires. A session that mwa-cli wrote is not used.

### Types from the OpenAPI schema

The types of the OpenAPI schema replace the library's own:

| 2.x | 3.0.0 |
|---|---|
| `AsvoJobState` | `JobState`. Its `Error` member has no message: use `MwaAsvoJob::error_text`. |
| `AsvoJobType` | `JobType`, the schema's string enum, with `names()` and `parse_name()` |
| `AsvoFilesArray`, `AsvoJobProduct` | `JobFile`, `JobProduct`. `JobFile.type` is a `DeliveryDestination`. |
| `Delivery` | `DeliveryDestination`, the schema's |

`MwaAsvoJob` wraps `JobDetailResponse` (through `Deref`). `obs_id()` and `job_id()` are methods.

### Validation

The library checks a job body only against the limits of the schema (numbers, image sizes). The rules that are not in
the schema are the MWA ASVO's, and it refuses a body that breaks one with a validation error: `delivery_format`
`files` with `delivery` `acacia`, the phase centre (`custom_centre_ra` and `custom_centre_dec` with `centre` `custom`
only), and `only_metadata` (true) with a voltage argument. `Bounds::range_text` gives the text of a limit, for example
`range: 0 to 7200`, which the help of the command shows.

### Errors

- A request to the MWA ASVO API fails with `MwaAsvoApiError`, which holds the API's `ErrorResponse`. Inside a check or
  download it is `MwaAsvoError::Api`.
- `AsvoError::Reqwest` is gone. A failed HTTP request is `MwaAsvoApiError::Http`, and an error reply of the MWA ASVO
  is `MwaAsvoApiError::Server`. Its `error_code` is one of the schema's `ErrorCode` values. A reply with another code
  is an HTTP error (`MwaAsvoApiError::BadStatus`, `HTTP_<status>` in `--json`) with the reply as its message.
- `AsvoError::HttpError` and `Http404Error` are `MwaAsvoError::HttpStatus` and `MwaAsvoError::HttpNotFound`, and have
  the `job_id` of the download.
- A file or directory that cannot be created is `MwaAsvoError::CannotCreate`, not `AsvoError::IO`.
- `MwaAsvoError::HashMismatch` names the file by its path in the download directory, not by its URL, which is a signed
  URL.
- `error_response` on `MwaAsvoApiError`, `MwaAsvoError` and `ParseError` gives an error as a `ClientErrorResponse`.
  It has the JSON shape of the schema's `ErrorResponse` with the code as text, so it can also hold a giant-squid code.
  The `--json` error lines of the command use it.
- New download errors:

  | Error | `--json` code | When |
  |---|---|---|
  | `MwaAsvoError::JobExpired` | `JOB_EXPIRED` | The links of the job have expired, or the file server answers `AccessDenied` (HTTP 403). |
  | `MwaAsvoError::DestinationExists` | `DESTINATION_EXISTS` | Something is already where a Scratch job would be moved to. |
  | `MwaAsvoError::SymlinkInDownloadDir` | `SYMLINK_IN_DOWNLOAD_DIR` | A download would write through a symbolic link. |
  | `MwaAsvoError::FilesNotReachable` | `FILES_NOT_REACHABLE` | A DUG job, or a Scratch job whose path this host cannot reach. |
  | `MwaAsvoError::InvalidJob` | `INVALID_JOB` | A Scratch path that is not a folder named for its Job ID. |
  | `MwaAsvoError::NoHash`, `NoUrl`, `NoPath` | `NO_HASH`, `NO_URL`, `NO_PATH` | A job file with no SHA-1 hash, or a URL or Scratch path that names no file. Before, the download panicked. |

  `MwaAsvoError::NoMwaAsvoJob` is a Job ID that the MWA ASVO does not have.

### Downloads

- `DownloadOptions::download_dir` and `DownloadManyOptions::download_dir` are a `&Path`, not a `&str`, so a directory
  whose name is not UTF-8 can be used.
- `DownloadOptions::job_subdirectory` (`true` in `DownloadOptions::new`) puts the files of each job into a directory
  of its own below the download directory, named for the Job ID.
- `DownloadOptions::retry_duration` (the command's `GIANT_SQUID_DOWNLOAD_RETRY_SECS`) sets how long a transient
  failure is retried, from the first failure.
- Expired download links: `MwaAsvoJob::expires`, `is_expired` and `display_state`. The links of a job work for 7
  days (`SIGNED_URL_LIFETIME_DAYS`), and the end of that time is read from the `Expires` of the signed URL.
  `MwaAsvoJobVec::all_ready` fails for a completed job whose links have expired, as for a cancelled job.

### Version

`mwa_asvo::api::openapi::MWA_ASVO_API_VERSION` and `OPENAPI_VERSION` are the versions of the MWA ASVO API and of the
OpenAPI specification that the library was generated from. `giant-squid --version` prints them.

## Rust library: added

- `DownloadOptions::new(download_dir)` and `DownloadManyOptions::new(download_dir)` give the defaults of the
  `giant-squid` command: `keep_tar` and `no_resume` `false`, `hash` and `job_subdirectory` `true`,
  `DEFAULT_DOWNLOAD_BUFFER_SIZE`, `DEFAULT_DOWNLOAD_RETRY_DURATION`, one download, and for several downloads
  `DEFAULT_CONCURRENT_DOWNLOADS`. The command and the Python module use them.
- `MwaAsvoClient::get_job` gets one job from the schema's `GET /api/v2/jobs/{job_id}`, so it finds a job of any age.
  A Job ID that the MWA ASVO does not have is `MwaAsvoError::NoMwaAsvoJob`. `download_job` uses it, so it downloads a
  job that is older than the window of the job list (30 days by default).
- `MwaAsvoClient::wait_for_jobs` is the wait of `giant-squid wait` and `--wait`, which now call it. `WaitOptions` has
  its times (`DEFAULT_WAIT_INITIAL_DELAY`, `DEFAULT_WAIT_POLL_INTERVAL`, `DEFAULT_WAIT_RETRY_DURATION`) and a
  `should_stop`.
- `MwaAsvoClient::download_many` is the downloads of `giant-squid download`, which now calls it. It takes
  `DownloadTarget`s and `DownloadManyOptions`. It gets the job list once, downloads a job that is given twice once,
  starts the downloads in their order, and gives a result for each. The command no longer depends on `rayon`.
- `MwaAsvoClient::submit_many` is the submissions of the `giant-squid submit-*` commands, which now call it. It
  submits one job for each Obs ID, in their order, with a body that a function makes for each Obs ID. It does not
  stop at a failure, and gives a result for each Obs ID (`SubmitManyResult`). The trait `ObsIdJobParams` is the
  bodies that it takes (`DownloadJobParams` and `PreprocessJobParams`), with `with_obs_id`.
- `MwaAsvoJob::total_size` is the sum of the sizes of the files of a job, which `list` and `download --json` show.
  `MwaAsvoJob::download_destination` is the directory that a download puts the files of a job in, which
  `download --json` shows.
- `parse_ids_from_text` reads the IDs that `download -`, `cancel -` and `wait -` read on standard input (the output
  of `list --ids` or `list --json`, or IDs that spaces or lines separate) and gives each ID once (`JobIdsAndObsIds`).
  The commands now call it.
- `job_field_names`, `parse_job_field` and `job_field_value`, for `list --fields`. A listed job's `job_params` are
  read as the schema type of the job's type, so a key that the server did not send gives the schema default.
- `validate::RECEIVER_CHANNEL`: the schema's limits of `from_channel` and `to_channel` (0 to 255), which the help of
  the command shows.
- manta-ray-client CSV files: `MwaAsvoClient::submit_csv_file`, which reads a file and returns the
  `JobSubmittedResponse` of each job, `MwaAsvoClient::submit_csv_jobs`, `read_manta_ray_csv`, `parse_manta_ray_csv`
  and `CsvDefaults`. A file with no job is `CsvFileError::NoJobs`. See the
  [README](../README.md#submitting-the-jobs-of-a-manta-ray-client-csv-file).

## Rust library: removed

- The version 1 API client.
- `AsvoError::Expired` and the `expired` job state. The MWA ASVO API has no expired state. A job whose download links
  have expired is found from the links: see `MwaAsvoJob::is_expired`.
- The `Unknown` job type. A job type that the schema does not list fails the listing. Every job has a `job_type`.
- `Obsid::from_string`. Parse each Obs ID with `str::parse::<ObsId>()`.
- The `built_info` module (build-time information from the `built` crate), which nothing used.
- `AsvoJobMap` and `AsvoJobVec::into_map`. `MwaAsvoJobVec::json` still prints the jobs as an object keyed by Job ID.

## Security

- `Display` of `MwaAsvoJob` gives the number of its files, not their signed URLs. `Debug` of `MwaAsvoClientConfig`,
  `MwaAsvoClient` and `StoredTokens` does not show the API key or the tokens.
- The library logs no credential: see
  [V3_MIGRATION.md](V3_MIGRATION.md#authentication-and-environment-variables). It no longer asks `reqwest` for a log
  of every byte on the wire, which had the cookies, the API key and the signed URLs in it.
- An error reply that is not an MWA ASVO `ErrorResponse` is kept in the error (`BadStatus`, `AuthenticationFailed`,
  `HttpStatus`) with its URLs redacted, and only its first 2000 characters.
- The login, refresh and API requests follow a redirect only to the same host and port. The download of a file still
  follows every redirect. A client for an `http://` host warns that the API key and the session are sent in clear
  text.
- The download safety rules (tar entry paths, links in the archive, symbolic links in the download directory, the
  Scratch folder name) are the library's: see [V3_MIGRATION.md](V3_MIGRATION.md#safety-of-the-download-directory).

## Python module

### The package

3.0.0 adds a Python package, `mwa-giant-squid` (`import mwa_giant_squid`), built from the same code, with type stubs.
The wheels are attached to each GitHub release. The API reference is generated with pdoc and published to GitHub
Pages, <https://mwatelescope.github.io/giant-squid/>. See [PYTHON.md](PYTHON.md).

The package installs the `giant-squid` command. It is the Rust program, run inside the module, so it is the same
program as the Rust one: the same commands, options, help, messages and exit codes.

The module has the items of the Rust library, with the same names. The Python classes have the same names as the Rust
types (`MwaAsvoClient`, `MwaAsvoJob` and so on). `tests/python/parity_manifest.py` lists what is left.

### What the module has

- The constants `DEFAULT_MWA_ASVO_HOST`, `DEFAULT_API_TIMEOUT` and `DEFAULT_DOWNLOAD_RETRY_DURATION` (in seconds),
  `BYTES_PER_MIB`, `MIN_DOWNLOAD_BUFFER_SIZE`, `DEFAULT_DOWNLOAD_BUFFER_SIZE`, `SIGNED_URL_LIFETIME_DAYS`,
  `OBS_ID_HINT` and the `ENV_*` names of the environment variables. The functions `default_token_cache_path` and
  `check_file_sha1_hash`.
- `MwaAsvoClient(config)`, which takes an `MwaAsvoClientConfig` (its `repr` does not show the API key), and
  `client_config_from_env()`. `JobsFilter` (`get_jobs(filter=None)`).
- `MwaAsvoClient.get_job`, as in the library.
- `MwaAsvoClient.wait_for_jobs`. It takes the times of the wait as keyword arguments, and Ctrl-C stops the wait.
- Downloads: `DownloadOptions` and `DownloadSettings` (with `DownloadSettings.from_env()`), for
  `download_job(job_id, options)`, `download_obs(obs_id, options)`, `download_job_from(jobs, job_id, options)` and
  `download_obs_from(jobs, obs_id, options)`. `DownloadOptions` has the defaults of `DownloadOptions::new`, and the
  field `job_subdirectory` (`True` by default). The module gives its `download_dir` to the library as it is, so a
  `download_dir` that is not valid UTF-8 works.
- `MwaAsvoClient.download_many`. It takes `job_ids` and `obs_ids`, a `DownloadOptions` and `concurrency` (default
  `DEFAULT_CONCURRENT_DOWNLOADS`), and gives a `DownloadResult` for each download.
- `MwaAsvoClient.submit_many`. It takes one body, gives it the Obs ID of each job, and gives a `SubmitResult` for each
  Obs ID (with `obs_id`, `response` and `error`).
- `MwaAsvoJob.total_size`, `MwaAsvoJob.download_destination`, `MwaAsvoJob.expires`, `is_expired` and
  `display_state`.
- `parse_ids_from_text`, which gives a tuple of the Job IDs and the Obs IDs.
- `ObsId` (`ObsId.MIN`, `MAX`, `validate`, `from_str`, `get`). Every Obs ID argument takes an `int` or an `ObsId`.
  `MwaAsvoJob.obs_id`, `CsvJobFailure.obs_id`, the `obs_id` of an error and the `obs_id` of a request body are
  `ObsId`. An `ObsId` is equal to the `int` of the same value, but `json.dumps` needs `int(obs_id)`.
- The functions `parse_obs_ids_only`, `parse_job_ids_only`, `parse_many_job_ids_or_obs_ids`,
  `parse_job_ids_and_obs_ids_from_file` and `parse_utc_time`.
- `JobDetailResponse` and `MwaAsvoJob.detail()` (the job as the API gives it), `JobFile.size_bytes`,
  `MwaAsvoJobVec.json()` (as `giant-squid list --json`), and `names()` and `parse_name()` on `JobType` and
  `JobState`. The `repr` of a `JobFile` redacts its URL.
- The request bodies `DownloadJobParams`, `ConversionJobParams`, `ImagingJobParams` and `ImageFromJobParams` (which
  has no `obs_id`, as the schema's type does not), with `validate()`, and the functions that make them
  (`download_params`, `conversion_params`, `imaging_params` and `image_from_job_params`). Each optional argument has
  the schema's default as its default value (for example `avg_freq_res=40.0`). `None` is sent as `null` for the
  arguments that the schema allows to be `null`. The `obs_id` and `source_job_id` of a body are checked when the class
  is made or the attribute is set. The `Args:` of the classes have the help texts of the schema (`x-cmd-help`), as
  the command's help does.
- `submit_download_job` takes a `DownloadJobParams`, `submit_preprocess_job` a `PreprocessJobParams`
  (`PreprocessJobParams.Conversion(params)` or `PreprocessJobParams.Imaging(params)`) or one of the two classes alone,
  and `submit_image_from_job` an `ImageFromJobParams`. The enum `OutputFormat`.
- manta-ray-client CSV files: `CsvDefaults` (with `from_env()`), `CsvJob`, `CsvJobParams` (`Conversion`, `Imaging`,
  `Download`, `Voltage` and `Beamformer` variants, with `description()` and `validate()`), `CsvRowError`,
  `parse_manta_ray_csv`, `read_manta_ray_csv`, `MwaAsvoClient.submit_csv_jobs(jobs, on_result=None)`,
  `MwaAsvoClient.submit_csv_job(params)` and `MwaAsvoClient.submit_csv_file(path, defaults=None)`, which returns a
  list of `JobSubmittedResponse`.

### Exceptions

See [PYTHON.md](PYTHON.md#errors) for the table of exceptions.

- Every variant of `MwaAsvoApiError`, `MwaAsvoError`, `ObsIdError`, `ParseError`, `CsvError`, `CsvJobsFailed`,
  `CsvFileError` and `CsvSubmitError` has a subclass of that exception, named by the two
  (`MwaAsvoApiErrorInvalidParameter`, `MwaAsvoErrorNoMwaAsvoJob`, `CsvSubmitErrorNoJobs`), with the fields of the
  variant.
- `kind` is the name of the Rust variant (for example `JobExpired` for `MwaAsvoError::JobExpired`). The stubs type it
  as a `typing.Literal` of the kinds, and each subclass narrows it to its one kind.
- An argument outside the MWA ASVO limits raises `MwaAsvoApiErrorInvalidParameter` (with `name` and `message`), a
  download option that cannot be used `MwaAsvoErrorInvalidOption`, and an Obs ID that is not valid `ObsIdError`. A
  10-digit number below `ObsId.MIN` raises `ParseErrorInvalidObsId`.
- An API error during `MwaAsvoClient(config)`, `client_config_from_env()` or a download is `MwaAsvoErrorApi`, as in
  the library, with the `MwaAsvoApiError` as its `source` and `__cause__`.
- A CSV file with a problem or with no job raises `CsvSubmitErrorInvalid` or `CsvSubmitErrorNoJobs`
  (`read_manta_ray_csv`: `CsvFileErrorNoJobs`), and one with jobs that failed `CsvSubmitError`.
- A Job ID of 0, in any argument or attribute (also an attribute of a request body or a `JobsFilter`), raises
  `ParseErrorZeroJobId`: the library's `ParseError::ZeroJobId`, with its message `0 is not a Job ID or an Obs ID`.
- A negative duration and other values that are not valid as Python values raise `ValueError`.
