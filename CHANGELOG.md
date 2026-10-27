# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 3.0.0 - 2026-10-??

giant-squid 3.0.0 uses version 2 of the MWA ASVO API. The job options, their names, their defaults and their
limits now come from the API's OpenAPI schema, so giant-squid, the API and its documentation use the same names.
Several defaults, option names and output formats changed: **read [docs/V3_MIGRATION.md](docs/V3_MIGRATION.md)
before you upgrade scripts.**

### Changed in 3.0.0

* The download links of a job work for 7 days, and the job stays `completed` in the API. `giant-squid` reads the end
  of that time from the `Expires` of the signed URL. `list` and `wait` show a completed job as `completed (ready)`
  or `completed (expired)` (the JSON and `--job-states` keep the API's `completed`). `download` of a job whose links
  have expired fails at once, with no request, with the new error `AsvoError::JobExpired` (`JOB_EXPIRED` in
  `--json`; kind `JobExpired` in Python): resubmit the job. The same error is given for the file server's
  `AccessDenied` answer (HTTP 403), which was an `HttpError` 403 before. `download` of an Obs ID skips a completed
  job whose links have expired when another one works, and `wait` (and `AsvoJobVec::all_ready`) fails for a completed
  job whose links have expired, as it does for a cancelled job. The library has `AsvoJob::expires`, `is_expired` and
  `display_state` (Python: `AsvoJob.expires`, `is_expired`, `display_state`). See the
  [README](README.md#expired-download-links).
* `download` puts the files of each job into a directory of its own below the download directory (`-d`,
  `--download-dir`), named for the Job ID: `-d /data` gives `/data/12345`. The files of two jobs of the same
  observation no longer meet in one directory. Use `--no-job-subdir` for the layout of 2.x, with the files in the
  download directory itself. A download resumes from the directory of its job, so a partial download that an earlier
  version left in the download directory is not found there: use `--no-job-subdir` to carry on with it. A Scratch job
  is not affected (it is moved into the download directory in its own folder, which the MWA ASVO has named for the
  job). The library has the option `DownloadOptions::job_subdirectory` (it has no default: set it), and the Python module
  has the argument `job_subdirectory` of `download_job` and `download_obs`, which is `True` by default, as for the
  command. See the [README](README.md#a-directory-for-each-job).
* `download` of a Scratch job that this host can reach moves its files into the download directory (`-d`,
  `--download-dir`), as for an Acacia job. Before, they went to the current directory. The move does not replace
  anything: if something is already at the target, the download fails (`AsvoError::DestinationExists`,
  `DESTINATION_EXISTS`; `SymlinkInDownloadDir` for a symbolic link) and nothing changes. A directory at the target,
  when the Scratch folder is gone, is the result of an earlier download, and a second download succeeds. If the download directory
  is where the files already are, nothing is moved.
* `download` of a DUG job, or of a Scratch job whose path this host cannot reach, fails
  (`AsvoError::FilesNotReachable`, `FILES_NOT_REACHABLE`) with a non-zero exit code. Before, it logged an error and
  reported success.
* An error that the MWA ASVO sends is shown as it sent it: its error code, message, `detail`, `suggestion`, field
  errors and request ID (also for a failed login). A failed job reports the MWA ASVO's `error_code` as well as its
  error text.
* The job table of `list` and `wait` is drawn with `comfy-table` (was `prettytable-rs`, whose `term` dependency is
  unmaintained). The layout and the colours are the same.
* A value of `MWA_ASVO_API_TIMEOUT` or `GIANT_SQUID_DOWNLOAD_RETRY_SECS` that is not a whole number is an error
  (`INVALID_ENVIRONMENT` in `--json`), as it already was for `GIANT_SQUID_BUF_SIZE`. In 2.5.1, giant-squid warned
  about `MWA_ASVO_API_TIMEOUT` and used the default.
* Every option that takes a named value (`--delivery`, `--centre`, `--pol`, `--job-states`, `--job-types` and so on)
  parses it in the same way: the case, spaces, hyphens and underscores do not matter (for example `--delivery ACACIA`,
  `--job-states WAIT-CAL`). The help (`[possible values: ...]`) and the message for a bad value list the values in the
  schema's order.
* Library (breaking): the public API is reorganised, and it reads no environment variable and prints nothing. The
  `giant-squid` command's variables are read by `client_config_from_env` and `DownloadSettings::from_env`.
  * The module `asvo` is `mwa_asvo`, and most items are also at the crate root (for example
    `mwa_giant_squid::AsvoClient`). `Obsid` and `ObsidError` are `ObsId` and `ObsIdError`; `AsvoJobID` is `AsvoJobId`
    (a `NonZeroU64`). `ObsId::MIN` and `ObsId::MAX` give the range of a valid Obs ID.
  * `AsvoClient::new` takes an `AsvoClientConfig`. `download_jobid` and `download_obsid` are `download_job` and
    `download_obs`, and return the `AsvoJob` that was downloaded (they returned `()`); `download_job_from` and
    `download_obs_from` use a job list that the caller already has. `get_jobs` takes a `JobsFilter`. There are two
    submit methods, one for each submit endpoint of the API: `submit_download_job` (a `DownloadJobParams`: a
    visibility, metadata, voltage or beamformer job) and `submit_preprocess_job` (a `PreprocessJobParams`: a
    `ConversionJobParams`, an `ImagingJobFlow1Params` or an `ImagingJobFlow2Params`, which takes no Obs ID). Their
    request bodies are made at the schema's defaults by `mwa_asvo::api::job_params`. A voltage job is a
    `DownloadJobParams` with `offset` and `duration`: the library does not check its delivery, because the MWA ASVO
    finds the kind of download from the observation and refuses a delivery that does not suit it.
  * The types of the OpenAPI schema replace the library's own: `AsvoJobState` is `JobState` (its `Error` member has
    no message: use `AsvoJob::error_text`), `AsvoJobType` is `JobType` (the schema's string enum, with `names()` and
    `parse_name()`), `AsvoFilesArray` and `AsvoJobProduct` are `JobFile` and `JobProduct`, and `Delivery` is the
    schema's `DeliveryDestination`. `AsvoJob` wraps `JobDetailResponse` (through `Deref`); `obs_id()` and `job_id()` are methods.
  * Errors: a request to the MWA ASVO API fails with `AsvoApiError`, which holds the API's `ErrorResponse`
    (`AsvoError::AsvoApi`); `AsvoError::Reqwest` is gone. `AsvoError::HttpError` has the `job_id` of the download.
    A file or directory that cannot be created is `AsvoError::CannotCreate`, not `AsvoError::IO`. A failed download is
    reported once, by the caller; the download no longer also logs it. `AsvoError::HashMismatch` names the file by its
    path in the download directory, not by its URL, which is a signed URL. `error_response` on `AsvoApiError`,
    `AsvoError` and `ParseError` gives an error as the API's `ErrorResponse`.
* Messages, logs, `--help` and the docs write "Job ID" and "Obs ID" (were "job ID", "jobid", "obsid" and
  "Obsid"). JSON keys and argument names do not change (`job_id`, `obs_id`).
* giant-squid now authenticates against the MWA ASVO's v2 (JWT-based) login API instead of HTTP Basic Auth.
  The resulting session is cached at `$HOME/.mwa-asvo/tokens.json` (shared with mwa-cli, so logging in with
  either tool covers both) and is automatically refreshed when it expires, avoiding a fresh login on every
  command. This is transparent to users - `MWA_ASVO_API_KEY` is still how you authenticate.
* Every command except the file transfer of `download` uses the v2 API. The defaults and limits of the job
  options are the API's, and a value outside a limit is refused before any request is sent. The defaults of
  conversion and imaging jobs changed: see the migration guide.
* The default delivery format is now `tar` for every job, whatever the delivery. Use `--delivery-format files` for
  individual files. This includes `submit-volt`, whose defaults are no longer Scratch and files: they are those of
  every download job, and the MWA ASVO refuses them for a voltage job, so give `--delivery scratch
  --delivery-format files`. The voltage lines of a manta-ray-client CSV file still default to Scratch and files.
* `submit-conv` and `submit-image`: the `-p` / `--parameters` option is gone. Each key is now a command line
  option with the API's name (for example `--avg-time-res 0.5`). `--phase-centre-ra`, `--phase-centre-dec`,
  `--custom-ra`, `--custom-dec` and `--phase-center` still work.
* `submit-image` takes Obs IDs only. The new `submit-image-from-job` (alias `sifj`) images a completed conversion
  job.
* All the `submit-*` commands have a new `--json` option, which prints the MWA ASVO's reply for each job. With
  `--dry-run` they print the request body that would be sent. The `status` text of a reply ("success" or
  "failed") is for display; the exit code tells you whether a submission worked.
* `list`: new options `--date-from`, `--date-to` and `--sort-by`. `--job-states` and `--job-types` are the new
  names of `--states` and `--types`, which still work. `--days` takes 1 to 30, and without it `list` uses the
  MWA ASVO API's default (30 days, shown in `--help`) instead of fetching your full history. `wait` and
  `download` find jobs the same way.
  `--days` cannot be used with `--date-from` or `--date-to`.
* `list --json` and `wait --json` use the API's key names (for example `id`, `obs_id`, `product.files`) and
  have more keys. `--legacy-json` prints the old format, and is deprecated: it will be removed in a later release.
* The job states are the MWA ASVO API's: `completed` (was `ready`), `waitcal`, `error` and the others, in lower case,
  as the API gives them. `list --job-states` takes these names (in any case), and the table, the JSON and the log
  lines of `wait` show them. In the JSON, `job_state` of a job with an error is `"error"`, and the message is only in
  `error_text` (before, it was `{"Error": "<message>"}`). `--legacy-json` still prints the 2.x values.
* `download` and `cancel` have `-j`, `--json`, as the other commands do. `download --json` prints one line of JSON
  for each download (`job_id`, `obs_id`, `status`, `message`); `cancel --json` prints the MWA ASVO's reply to each
  request.
* With `--json`, every command prints only JSON, on standard output: no log messages and no progress bars. Each
  error is one line of JSON in the form of the MWA ASVO's error response (`error_code`, `message`, and `detail`,
  `suggestion`, `field_errors`, `request_id` when known), with the `obs_id` or `job_id` it is about. An error from
  the MWA ASVO is printed as it was sent; other errors have a giant-squid code (for example `HTTP_503`,
  `NETWORK_ERROR`, `INVALID_ARGUMENT`). A bad command line with `--json` is one such line too. `-v` cannot be used
  with `--json` or `--legacy-json`. `--dry-run --json` prints each request as JSON.
* The help and clap's messages are in colour on a terminal (as cargo's are), not only bold. `NO_COLOR` turns the
  colours off.
* All log lines now go to standard error. Standard output has only the output of the command (the job table,
  or JSON), so a script can read it.
* The submit commands try every Obs ID, and log the error of each failed submission when it happens. If any
  failed, the command ends with `Error: M of N job submissions failed` and exit code 1. In 2.x the command stopped
  at the first failure.
* `download` ends with `Error: M of N downloads failed` and exit code 1 if any download failed (it exited 0).
* `cancel` logs `Cancel request for Job ID N: <message>` for each job and `Cancel requests: N sent, M failed.` at
  the end, instead of `Cancelled N jobs.`. The MWA ASVO answers the cancellation of a job that is already
  cancelled with a normal reply, so a reply does not prove that a job was cancelled: read the message. A request
  that the MWA ASVO refuses with an HTTP error is a failure: `cancel` sends every request, then exits with code 1
  (in 2.x a failed request was skipped silently, and the exit code was 0).
* `wait` and `cancel` take Job IDs only. An Obs ID is an error that names it and nothing is sent; before, it was
  ignored without a word.
* `list --job-types` refuses text that is not a job type, instead of matching no job.
* The job types are the MWA ASVO API's. `list --job-types` and the table use the API's names (`conversion`,
  `visibility`, `metadata`, `voltage`, `cancel`, `beamformer`, `imaging`), and `--json` gives the API's code (`0` to
  `6`), or `null` for a job with no type. The 2.x names (`download_visibilities` and so on) are refused.
  `--legacy-json` still prints the 2.x names.
* A file in `--json` is the API's `JobFile`: its `type` is `acacia`, `scratch` or `dug` (was `Acacia` and so on), and
  a key that has no value is left out (was `null`).
* A job in `--json` is the API's `JobDetailResponse`, with one more key, `obs_id`. The Job ID is `id` (was
  `job_id`), and a key that has no value is left out (was `null`).
* Acacia delivers a tar only: `delivery_format` `files` with `delivery` `acacia` is refused before any request by
  every submit method (`validate::check_delivery_format`; `InvalidParameter` for `delivery_format`, `ValueError` in
  Python), and is a usage error in every `submit-*` command (`--delivery-format files` with the default or a given
  `--delivery acacia`, also from `GIANT_SQUID_DELIVERY_FORMAT`). The other deliveries take either format.
* `giant-squid --version` also prints the version of the MWA ASVO API and of the OpenAPI specification that the
  library was generated from (for example `MWA ASVO API 2.3.4`, `OpenAPI 3.1.0`). The library has them as
  `mwa_asvo::api::openapi::MWA_ASVO_API_VERSION` and `OPENAPI_VERSION`.
* CLI: an option for a field that the MWA ASVO allows to be `null` takes the word `null` (`--avg-freq-res null`,
  `--no-rfi=null`, `--allow-resubmit=null`): `--avg-freq-res`, `--avg-time-res`, `--abs-threshold`,
  `--clean-threshold`, `--no-digital-gains`, `--no-flag-dc`, `--no-geometric-delay`, `--no-passband-gains`,
  `--no-cable-delay`, `--no-rfi` and `-r`/`--allow-resubmit`. The last seven also take `true` and `false`, and
  the bare flag (`--no-rfi`, `-r`) is still `true`. `--help` shows each option's default.
* Short flags: `-f` is only `--delivery-format` (`submit-volt --from-channel` and `--to-channel` lost `-f` and `-t`),
  and `-n` is only `--dry-run` (`list --no-colour` and `wait --no-colour` lost `-n`). The other short flags did not
  change. See [docs/V3_MIGRATION.md](docs/V3_MIGRATION.md#short-flags).
* `--help` lists the allowed values of `--delivery`, `--delivery-format`, `--output`, `--centre`,
  `--output-mode`, `--pol` and `--weighting`, and the help of `list --job-states` and `--job-types` lists the
  names that the options accept (it listed `retrieving`, which is not a state).
* `list` command now shows completed date time.
* `-v` now shows all API requests and responses. `-vv` shows the full payload of requests and responses.
* The environment variable `GIANT_SQUID_DOWNLOAD_RETRY_SECS` sets how long a failing download is retried.
* Docker:
  * Updated the docker build to use a docker hardened image and a two stage build approach.

* MWA ASVO API 2.6.0 (OpenAPI 3.1.0) is applied. The download jobs (visibility, metadata, voltage and beamformer) go to
  `/v2/jobs/download`, and the conversion and imaging jobs (also an image from a conversion job) to
  `/v2/jobs/preprocess`; the job list, login and refresh endpoints have new paths. A job has `job_id` (not `id`) and a
  typed `obs_id`, and `job_type` is a name (`visibility`), not a code. A download body has no `download_type`: a
  metadata job sets `only_get_metadata`, and `submit-bf` sends the same body as `submit-vis`, so the beamformer
  `mode` and `channel_range` are gone: in a CSV file `mode` is an unknown key, and `channel_range` is checked but not
  sent. The voltage fields `offset`, `duration`, `from_channel` and `to_channel` are fields of the download body.
  `submit-image-from-job` takes no Obs ID. `list --sort-by` takes `id`, `created` or `completed`.
  `giant-squid` regenerates the library's types from the schema in `tools/generate_openapi.sh`, which now also
  keeps the request and response schemas of each operation.

### Added in 3.0.0

* `list` and `wait` with `NO_COLOR` set (and not empty) print the job table without colours, as `--no-colour` does.
* A Python package, `mwa-giant-squid` (`import mwa_giant_squid`), built from the same code, with type stubs. It
  installs the `giant-squid` command, which is the Rust program run inside the module, so it is the same
  program as the Rust one (the same commands, options, help, messages and exit codes). The two submit methods
  (`submit_download_job` and `submit_preprocess_job`) take a request body class (`DownloadJobParams`,
  `ConversionJobParams`, `ImagingJobFlow1Params` or `ImagingJobFlow2Params`). Each optional argument of a class has
  the schema's default as its default value (for example `avg_freq_res=40.0`), and `None` is sent as `null` for the
  arguments that the schema allows to be `null`. The API reference is generated with pdoc and
  published to GitHub Pages, <https://mwatelescope.github.io/giant-squid/>. See [docs/PYTHON.md](docs/PYTHON.md). The
  wheels are attached to each GitHub release.
* `download` without `--keep-tar` (stream untar) now resumes. A retry after a failed attempt (for example, a
  dropped connection) continues from where the attempt stopped. A new run of the same command continues from the
  files that are already in the download directory: complete files are not downloaded again, and a partly
  written file is continued. The SHA-1 still covers the whole archive. A resume file,
  `.<tar file name>.giant-squid-resume.json`, in the download directory lets a new run continue without reading
  the complete files again. It is deleted when the download is complete.
* `download` retries a network error during the transfer, with or without `--keep-tar`. Before, only a failed
  request was retried.
* `submit-csv` (short name `scsv`) submits the jobs of a [manta-ray-client](https://github.com/MWATelescope/manta-ray-client)
  CSV file (`mwa_client -c file.csv`): conversion, download (visibility and metadata), beamformer, voltage and
  imaging jobs, one per line. It reads and checks the whole file before it submits anything, and a submission that
  fails does not stop the others. A key that the job type does not have is an error. A key that a line gives twice,
  and a line that makes the same job as an earlier line, are warnings. A line that gives no delivery or delivery
  format uses `GIANT_SQUID_DELIVERY` and `GIANT_SQUID_DELIVERY_FORMAT`, if they are set; a voltage line that gives
  none and has none set is delivered to Scratch as files, as in manta-ray-client. See the
  [README](README.md#jobs-from-a-manta-ray-client-csv-file). The library has `AsvoClient::submit_csv_file`, which
  reads a file and returns the `JobSubmittedResponse` of each job, `AsvoClient::submit_csv_jobs`,
  `read_manta_ray_csv`, `parse_manta_ray_csv` and `CsvDefaults`. A file with no job is an error (`CsvFileError::NoJobs`). The Python module has `AsvoClient.submit_csv_file`, which returns a list
  of `JobSubmittedResponse`, and the exception `CsvSubmitError` for a file with jobs that failed. A file with no job raises `ValueError`.
* `download`, `cancel` and `wait` read their IDs from standard input when an argument is `-`. The input is the
  output of the new option `list --ids` (one Job ID on each line) or of `list --json` (the Job IDs are its keys), or
  IDs that spaces or lines separate. The commands do not filter what they read: filter the list first, for example
  `giant-squid list --job-states completed --ids | giant-squid download -`. An empty list (`list --ids` prints
  nothing, and `list --json` prints `{}`) is not an error: the command ends with exit code 0. Standard input is read
  only because of the `-`, never guessed from the kind of input, so a command with no ID and no `-` fails as in 2.x
  (its error says to give `-`). See the
  [README](README.md#use-the-output-of-list-as-the-input-of-other-commands).

### Fixed in 3.0.0

* `download`, `cancel` and `wait` use an ID that is given more than once a single time, at its first place. Before,
  `cancel 5 5` sent two requests.
* `download` starts several downloads at once in the order of the IDs, so the numbers in the log (`[1/20]`, `[2/20]`,
  ...) are in order. Before, each thread took a block of the list, and the log said `[1/20]`, `[11/20]`, `[6/20]`.
* `download` gets the job list once for all its downloads (it got it again for each Job ID or Obs ID).
* `download` of a reachable Scratch job moves the files across file systems too (by copy, then removal). Before, a
  move from another file system failed.
* `download` of Job IDs and Obs IDs together numbers the downloads from 1 to the total (`[3/4]`). Before, the Obs ID
  downloads started again from 1.
* `download` logs in once for all its downloads. Before, each download logged in on its own, and a long list could
  hit the server's limit on logins.
* `download` no longer panics on a job file with no SHA-1 hash (`AsvoError::NoHash`, `NO_HASH`), or on a download URL
  or Scratch path that names no file (`NoUrl`, `NoPath`). The time and speed in "Completed download of ..." are for
  that file, not for the job so far.
* `download --keep-tar` resume: the range request was sent with a wrong header, so the server sent the whole file
  and giant-squid appended it to the partial file. A complete file was downloaded again. The SHA-1 of a resumed
  download covered only the new bytes. All three are fixed. If a server ignores the range request, giant-squid
  now downloads the file again from the start.
* `download --no-resume` downloads a partial file again from the start. Before, it skipped the partial file and
  left it as it was. A complete `--keep-tar` file that matches the SHA-1 is still not downloaded again. A retry in
  the same run still continues its own partial file.
* `--skip-hash` now skips the SHA-1 check only for a download that runs from start to end in one attempt. A
  resumed download is always checked, with or without `--keep-tar`: after a failed attempt in the same run, or from
  the files of an earlier run (see the stream-untar resume entry above). If the check fails, the whole archive is
  downloaded again. With `--keep-tar`, a complete tar file that is already on disk is checked, as in 2.x. The help
  of `--skip-hash` now says when the check is not skipped.
* Negative numbers are accepted in the `--flag value` form, for example `submit-volt --offset -1`. Before, clap read
  the value as an unknown flag (`unexpected argument '-1'`), and only the `--flag=value` form worked.
* `--version` prints `giant-squid 3.0.0`. Before, it printed the name of the crate (`mwa_giant_squid`).
* `download` fails when its files of IDs hold no Job ID or Obs ID, and when `-d`/`--download-dir` is not a
  directory. Before, the first did nothing and exited 0, and the second was accepted.

### Security in 3.0.0

* A stream-untar download (`download` without `--keep-tar`) no longer writes outside the download directory. A
  tar entry whose path is absolute (for example `/home/user/.bashrc`), contains `..` (for example
  `../../.ssh/authorized_keys`) or has no name is not written. giant-squid shows a warning for each such entry
  and unpacks the other entries. In 2.x, such an entry was written where its path pointed, so a bad archive could
  create or overwrite a file anywhere that the user can write. The SHA-1 check still covers the whole archive.
* `download` does not write through a symbolic link that is already in the download directory. A stream-untar entry
  whose file, or a directory on the way to it, is a symbolic link fails the download with
  `AsvoError::SymlinkInDownloadDir` (the entries before it are already written). A `--keep-tar` download whose tar file
  is a symbolic link (even one that points nowhere) fails in the same way, and nothing is fetched. The download
  directory itself may be a symbolic link. On Unix, the refusal is part of the call that opens the file
  (`O_NOFOLLOW`) and of the calls that walk down the directories of a tar entry (`openat`), so a link that another
  process puts in the place after the check is refused too. Before, the
  download wrote wherever the link pointed. A Scratch job
  folder with a link that points outside the folder is moved as before (the link is moved as a link), with a warning
  for each such link.
* A stream-untar download unpacks only files and directories. A symbolic link, a hard link or a special file in the
  archive is skipped with a warning (the SHA-1 check still covers it). In 2.x, such an entry was unpacked, so a link
  could point outside the download directory.
* The logs show no credential, also at trace level (`-vv`). The login and refresh requests and replies, which hold the
  API key and the tokens, are not logged. The other request and reply bodies are logged with the URLs in them
  redacted, and so is the URL of a file of a job (a signed Acacia URL): giant-squid shows the host, the path, the
  names of the query parameters and the first four characters of a long value. An error of a file download does not
  show the signed URL either. The library no longer asks `reqwest` for a log of every byte on the wire, which had the
  cookies, the API key and the signed URLs in it.

### Removed in 3.0.0

* The version 1 API client, and the `-p` / `--parameters` option (see above).
* The `expired` job state, and the library error `AsvoError::Expired`. The MWA ASVO API has no expired state. (A job whose download links have expired is found from the links: see "Changed in 3.0.0".)
* The `Unknown` job type. A job type that the schema does not list fails the listing; a job with no type has
  `job_type` `None`.
* Library: `Obsid::from_string`. Parse each Obs ID with `str::parse::<ObsId>()`.
* Library: the `built_info` module (build-time information from the `built` crate), which nothing used.
* Library: `AsvoJobMap` and `AsvoJobVec::into_map`. `AsvoJobVec::json` still prints the jobs as an object keyed by
  Job ID.

### Housekeeping in 3.0.0

* Tests moved into their own module: a module that has tests is a folder with `mod.rs` (the code) and `tests.rs`
  (the tests).
* A test suite that runs offline against a mock MWA ASVO server (recorded responses are replayed), so that no
  test submits or cancels a job or downloads data. See [docs/TESTING.md](docs/TESTING.md).
* Tests that keep decisions from being undone by accident: only parameters of the API schema are sent, only
  end-user endpoints are called. See [docs/TESTING.md](docs/TESTING.md).
* `tools/run_live_tests.sh` runs the opt-in live tests against the MWA ASVO test server.
* CI checks that the generated API types and the Python type stubs are up to date, builds and tests the Python
  package on Linux and macOS, and attaches the wheels to releases.
* The crates.io package does not include the files that only the Python package uses.
* Unused dependencies removed: `bytes`, `litemap`, `zerofrom`, `time`, `time-core`, `reqwest-retry`,
  `task-local-extensions`, `reqwest_cookie_store`, and the dev-dependency `rand`. The API client keeps no cookie
  jar: it sends the session token in its own `Cookie` header.
* The unmaintained `backoff` crate (RUSTSEC-2025-0012, and `instant`, RUSTSEC-2024-0384) is replaced by a small
  exponential backoff with jitter in the download code. The waits are as before: from 0.5 s, growing by 1.5 times,
  at most 60 s, each moved by up to half of itself at random.

The entries of the development of 3.0.0, as they were written, are in [docs/history/CHANGELOG_3.0.0_DEVELOPMENT.md](docs/history/CHANGELOG_3.0.0_DEVELOPMENT.md).

## 2.5.1 - 2026-05-29

* Better error handling when MWA ASVO download has expired (and other 40X http status error codes).

## 2.5.0 - 2026-05-05

### Changed in 2.5.0

* Added support for imaging jobs and updated README.
* Ensure giant-squid does not fail if it encounters a job type it doesn't know about. Instead emit a warning for the user to update their giant-squid client.
* Added dependabot.yml to remove false positives (dependabot picking up vulns in packages which are not actually used, but are optional features of packages giant-squid does use).
* Updated dependencies to mitigate several security vulnerabilities.

### Fixed in 2.5.0

* Fixed divide by zero error when downloading a very small file over a very fast link.

## 2.4.0 - 2026-02-26

### Changed in 2.4.0

* Bumped MSRV to 1.85.0 to help handle dependency creep.
* Added support for downloading beamformer observation data from MWA ASVO using the `submit-bf` command (see `giant-squid submit-bf --help` or README.md for syntax).

## 2.3.1 - 2025-12-01

### Changed in 2.3.1

* You can now set the environment variable `MWA_ASVO_API_TIMEOUT` to adjust how long giant-squid will wait on API calls to the MWA ASVO server before timing out. The default value if the environment variable is not set or not present is 60 (seconds).

## 2.3.0 - 2025-08-05

### Fixed

* Giant-squid now correcly adds a user agent string to all web service requests to the MWA ASVO server. This fixes a bug where calls to the MWA ASVO server would result in a 403 forbidden http error when the MWA ASVO server's web application firewall was enabled.

### Changed

* (Developers only): Overriding the MWA ASVO webserver address using the environment variable MWA_ASVO_HOST now requires you specify the fqdn and port instead of just the hostname.

## 2.2.0 - 2025-07-15

### Changed in 2.2.0

* Bumped MSRV from 1.71.1 to 1.82, updated many old dependencies which required >1.71.1.

### Fixed in 2.2.0

* Fixed build error due to two conflicting indicatif versions.

## 2.1.2 - 2025-05-26

### Fixed in 2.1.2

* Fixed error "Unrecognised job_state! preparing".

### Changed in 2.1.2

* Docker image is no longer based on old mwalib image and will now use the latest Rust stable version when building.

## 2.1.1 - 2025-05-07

### Added in 2.1.1

* Added support for new job state - `Preparing`.
* giant-squid now sends the correct self-identifying string when making API calls.

## 2.1.0 - 2025-04-16

### Added in 2.1.0

* New delivery option `dug`. For users with Curtin University DUG access, please contact MWA ASVO support if you would like the option of delivering data directly to DUG. See the README.md file for more info on delivery options. NOTE: this feature will go live on the MWA ASVO web server no earlier than 6-May-2025.

### Changed in 2.1.0

* Added ObsID info to certain error messages when submitting jobs, so it is easier to know which ObsID failed to submit correctly.
* If downloading results in permissions or other filesystem issues/errors, giant-squid will now show the file/path it was trying to write. (No more guessing what the problem is!)

### Fixed in 2.1.0

* Fixed "[ERROR] Is a directory (os error 21)" message when stream-untaring a CASA measurement set. Issue #31.

## 2.0.1 - 2025-03-07

### Fixed in 2.0.1

* Fixed [github issue #28](https://github.com/MWATelescope/giant-squid/issues/28)- stream untar downloads are being writting to the current directory, not the directory specified on the command line (`-d` / `--download-dir`). This has now been fixed. Thanks for the bug report **@elillesk**!
* Giant squid now checks to see if your download directory exists and quits if it doesn't or it is inaccessible.
* When downloading by ObsID and you have that ObsID in multiple jobs, giant-squid will download by ObsID so long as exactly one of the jobs is ready for download. If not, then it will report that it is ambiguous and you will need to specify the JobID instead.

## 2.0.0 - 2025-03-04

### BREAKING CHANGES in 2.0.0

* NOTE: this release of giant-squid will not work correctly until after Pawsey maintenance has finished which is estimated to be 05-Mar-2025- see [Pawsey Status Page](https://status.pawsey.org.au/). Until then please use version 1.2.0.
* MWA ASVO job states have changed. Please see [MWA ASVO wiki](https://mwatelescope.atlassian.net/wiki/spaces/MP/pages/24973129/Data+Access) for more information.

### Added in 2.0.0

* You can disable colour coding of the output of `giant-squid list` by passing `--no-colour`. Useful if you have a non-back terminal background for example.
* Added progress bar support for downloading via the default "streaming untar" method (i.e. not passing --keep-tar to the `download` command).

## 1.2.0 - 2025-02-18

### Added in 1.2.0

* New feature: download resume!
  * If you are downloading from MWA ASVO using giant-squid and pass the `-k` / `--keep-tar` option (meaning giant-squid will just download the tar file and not try to stream untar it) then giant-squid will now check to see if the target file is already partially downloaded. If it is, it will attempt to resume from where it left off. If the file exists and matches the expected size and the checksum matches it will skip the file. NOTE: due to the way the `stream untar` feature works (the default when you don't pass `-k` to the download command), resume is not yet supported.
  * You can disable the resume feature by adding the `-n` / `--no-resume` argument to the `download` command.
    * If an existing partial file does exist with the `--no-resume` flag, giant-squid will abort the download and leave the file alone.
* New feature: concurrent downloads!
  * There is now a new argument for the `download` command called `--concurrent-downloads` / `-c`. It defaults to 4, and specifies how many jobs can be downloaded concurrently. Generally a setting of 2-4 is ideal. Setting `--concurrent-downloads` to 0 will set the number of concurrent downloads to the number of CPU cores on your system. Setting `--concurrent-downloads` to 1 is the equivalent of downloading the jobs one by one.
* New feature: progress bars for (`--keep-tar`) downloads. The progress bar shows current download speed, time elapsed and ETA among other things.
* Added `cancel` command to allow cancellation of in progress jobs. Pass one or more jobids to cancel.

### Changed in 1.2.0

* MSRV bumped to 1.7.1 due to naughty sub-dependencies of reqwest.
* The `-k` `--keep-zip` option of the `download` command has been renamed to `--keep-tar` since MWA ASVO has not served out `zip` files for some time, rather, it uses `tar` files.
  * The `--keep-zip` option will remain supported (and is just an alias for `--keep-tar`) for some time, although it is now depreacted and will be removed in a future release.
* Changed some console output references to "ASVO" to be "MWA ASVO".

### Fixed in 1.2.0

* Fix- when passing the `-k` (`--keep-zip` / `--keep-tar`) option to the `download` command, the `-d` / `--download-dir` argument was being ignored and defaulting to `.`. Downloading with `-k` now correctly uses the specified download directory.
* Fix- the alias "sv" was assigned to both "submit-vis" and "submit-volt". "st" has now been assigned for "submit-volt" to avoid the duplication.
* Fix- `submit-volt` command no longer defaults delivery to 'acacia' (it can only be 'scratch').

### Security fixes in 1.2.0

* Updated/migrated clap to v4.4.
* Updated dependency quinn-proto to latest to fix security vulnerability.

## 1.1.0 - 2024-08-19

* Add new option to `submit-vis`, `submit-conv` and `submit-meta`: `delivery-format`. Currently only `tar` is supported.
  * This option only applies when `delivery=scratch`
* Add new option to `submit-volt`: `from_channel` and `to_channel`. Supplying these parameters will restrict the downloaded voltage data to only the specified receiver coarse channel numbers.
  * This option is only valid for MWAX_VCS and MWAX_BUFFER mode observations.
  * MWA receiver coarse channels are numbered 0-255 with the center frequency (in MHz) of each channel calculdated via `1.28 * receiver_channel_number`. There are 24 coarse channels per observation.
  * The channel range is inclusive
* Per-obsid non-fatal errors will no longer stop giant-squid from submitting subsequent jobs when using `submit-vis`, `submit-conv`, `submit-volt` and `submit-meta` with multiple obsids. Instead it will log the error and continue.

## 1.0.3 - 2024-05-23

* BUGFIX- ensure file modification and access time of files is set to be the time the file is written by giant-squid when stream untarring files. Fixes #22.

## 1.0.2 - 2024-05-16

* BUGFIX- allow-resubmit was being passed as True regardless of the command line argument (or omission of) used.

## 1.0.1 - 2024-05-15

* Added new command line option `--download-dir` when using the `download` subcommand so you can specify the directory to download files. It defaults to `.`, if ommitted, which was the hardcoed default in previous releases of giant-squid.

## 1.0.0 - 2024-05-13

* Increased MSRV to 1.70
* Added new command line option `--allow-resubmit` for `submit-vis` `submit-conv` `submit-meta` 'submit-volt`. When present, allow a new job to be submitted which has the same parameters as an existing job that is in your queue. Default is to not allow resubmit.
* Updated releases to include MacOS 14 (arm64) in addition to MacOS 13 (x86_64) and Linux x86_64.
* Fixed clippy lints.

## 0.8.0 - 2023-11-22

* supports specifying the MWA ASVO webserver address via environment variable `MWA_ASVO_HOST` (default is asvo.mwatelescope.org)
* supports use of `scratch` delivery option (in addition to `acacia` and `astro`)
* added `delivery` column to the `list` output
* updated many dependencies to more recent versions

## 0.7.0 - 2023-07-26

* support submission of voltage download jobs

## 0.6.0 - 2023-07-04

* enable hash validation by default

## 0.5.3 - 2023-06-30

* better handling of IO errors in `download` subcommand

## 0.5.2 - 2023-03-14

* pin task-local-extensions v0.1.2

## 0.5.1 - 2023-03-14

* update prettytable-rs to 0.10.0

## 0.5.0 - 2023-02-03

* support new ASVO API and delivery methods
* add wait subcommand
* enable filtering jobs by type,status in wait and list subcommands

## 0.4.1 - 2021-08-19

Bugfix release:

* Fix measurement set directories not downloading.
* Fix GitHub tests badge.

## v0.4.0 - 2021-08-15

Rust release of giant-squid.
