# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

# 3.0.0 - 2026-10-??

giant-squid 3.0.0 uses version 2 of the MWA ASVO API. The job options, their names, their defaults and their
limits now come from the API's OpenAPI schema, so giant-squid, the API and its documentation use the same names.
Several defaults, option names and output formats changed: **read [docs/V3_MIGRATION.md](docs/V3_MIGRATION.md)
before you upgrade scripts.**

### Changed in 3.0.0

* giant-squid now authenticates against the MWA ASVO's v2 (JWT-based) login API instead of HTTP Basic Auth.
  The resulting session is cached at `$HOME/.mwa-asvo/tokens.json` (shared with mwa-cli, so logging in with
  either tool covers both) and is automatically refreshed when it expires, avoiding a fresh login on every
  command. This is transparent to users - `MWA_ASVO_API_KEY` is still how you authenticate.
* Every command except the file transfer of `download` uses the v2 API. The defaults and limits of the job
  options are the API's, and a value outside a limit is refused before any request is sent. The defaults of
  conversion and imaging jobs changed: see the migration guide.
* The default delivery format is now `tar` for every job except voltage jobs, whatever the delivery. Use
  `--delivery-format files` for individual files.
* `submit-conv` and `submit-image`: the `-p` / `--parameters` option is gone. Each key is now a command line
  option with the API's name (for example `--avg-time-res 0.5`). `--phase-centre-ra`, `--phase-centre-dec`,
  `--custom-ra`, `--custom-dec` and `--phase-center` still work.
* `submit-image` takes obsids only. The new `submit-image-from-job` (alias `sifj`) images a completed conversion
  job.
* All the `submit-*` commands have a new `--json` option, which prints the MWA ASVO's reply for each job. With
  `--dry-run` they print the request body that would be sent. The `status` text of a reply ("success" or
  "failed") is for display; the exit code tells you whether a submission worked.
* `list`: new options `--date-from`, `--date-to` and `--sort-by`. `--job-states` and `--job-types` are the new
  names of `--states` and `--types`, which still work. `--days` takes 1 to 30, and without it `list` uses the
  MWA ASVO API's default (30 days, shown in `--help`) instead of fetching your full history. `wait` and
  `download` find jobs the same way.
* `list --json` and `wait --json` use the API's key names (for example `id`, `obs_id`, `product.files`) and
  have more keys. `--legacy-json` prints the old format, and is deprecated: it will be removed in a later release.
* The job states are the MWA ASVO API's: `completed` (was `ready`), `waitcal`, `error` and the others, in lower case,
  as the API gives them. `list --job-states` takes these names (in any case), and the table, the JSON and the log
  lines of `wait` show them. In the JSON, `job_state` of a job with an error is `"error"`, and the message is only in
  `error_text` (before, it was `{"Error": "<message>"}`). `--legacy-json` still prints the 2.x values.
* Library: `AsvoJobState` is replaced by the OpenAPI schema's `JobState` (`JobState::Completed` and so on). Its
  `Error` member has no message: use `AsvoJob::error_text`. In Python, `AsvoJobState` is replaced by `JobState`, with
  the same members.
* `download` and `cancel` have `-j`, `--json`, as the other commands do. `download --json` prints one line of JSON
  for each download (`job_id`, `obs_id`, `status`, `message`); `cancel --json` prints the MWA ASVO's reply to each
  request.
* With `--json`, every command prints only JSON, on standard output: no log messages and no progress bars. Each
  error is one line of JSON in the form of the MWA ASVO's error response (`error_code`, `message`, and `detail`,
  `suggestion`, `field_errors`, `request_id` when known), with the `obs_id` or `job_id` it is about. An error from
  the MWA ASVO is printed as it was sent; other errors have a giant-squid code (for example `HTTP_503`,
  `NETWORK_ERROR`, `INVALID_ARGUMENT`). A bad command line with `--json` is one such line too. `-v` cannot be used
  with `--json` or `--legacy-json`. `--dry-run --json` prints each request as JSON.
* Library: `AsvoApiError::error_response`, `AsvoError::error_response` and `ParseError::error_response` give an error
  as the API's `ErrorResponse`; the codes of errors that are not the API's are the `error_response::ERROR_CODE_*`
  constants.
* Library: `AsvoClient::download_job` and `download_obs` return the `AsvoJob` that was downloaded (were `()`); in
  Python they return an `AsvoJob` (were `None`).
* The help and clap's messages are in colour on a terminal (as cargo's are), not only bold. `NO_COLOR` turns the
  colours off.
* All log lines now go to standard error. Standard output has only the output of the command (the job table,
  or JSON), so a script can read it.
* The submit commands try every obsid, and log the error of each failed submission when it happens. If any
  failed, the command ends with `Error: M of N job submissions failed` and exit code 1. In 2.x the command stopped
  at the first failure.
* `cancel` logs `Cancel request for job N: <message>` for each job and `Cancel requests: N sent, M failed.` at
  the end, instead of `Cancelled N jobs.`. The MWA ASVO answers the cancellation of a job that is already
  cancelled with a normal reply, so a reply does not prove that a job was cancelled: read the message. A request
  that the MWA ASVO refuses with an HTTP error is a failure: `cancel` sends every request, then exits with code 1
  (in 2.x a failed request was skipped silently, and the exit code was 0).
* `wait` and `cancel` take job IDs only. An obsid is an error that names it and nothing is sent; before, it was
  ignored without a word.
* `list --job-types` refuses text that is not a job type, instead of matching no job.
* The job types are the MWA ASVO API's. `list --job-types` and the table use the API's names (`conversion`,
  `visibility`, `metadata`, `voltage`, `cancel`, `beamformer`, `imaging`), and `--json` gives the API's code (`0` to
  `6`), or `null` for a job with no type. The 2.x names (`download_visibilities` and so on) are refused.
  `--legacy-json` still prints the 2.x names.
* A file in `--json` is the API's `JobFile`: its `type` is `acacia`, `scratch` or `dug` (was `Acacia` and so on), and
  a key that has no value is left out (was `null`).
* A job in `--json` is the API's `JobDetailResponse`, with one more key, `obs_id`. The job ID is `id` (was
  `job_id`), and a key that has no value is left out (was `null`).
* Library: `asvo::apiv2::job_args` has one argument struct per job type (`DownloadArgs`, `ConversionArgs`,
  `ImagingArgs`, `ImageFromJobArgs`, `VoltageArgs`, `BeamformerArgs`), each with `into_params(obs_id)`, which makes the
  request body. A field that is `None` is left to the schema's default. The CLI and the Python module both make their
  request bodies with them.
* Library: every type generated from the OpenAPI schema derives `PartialEq`, and `JobType` also `Copy`, `Eq` and
  `Hash` (set in `build.rs`, so they survive regeneration). `AsvoJob`, `AsvoJobVec` and `AsvoJobMap` derive
  `PartialEq` again.
* Library: `AsvoApiError::MissingAuthKey` has a field `variable`, and its message no longer names `MWA_ASVO_API_KEY`
  unless the key was read from the environment (`No MWA ASVO API key was given: set the MWA_ASVO_API_KEY environment
  variable.`). `AsvoError::Reqwest` is removed: a request error of a download is `AsvoError::AsvoApi` with
  `AsvoApiError::Reqwest`, as for every other request (in Python, `AsvoApiError` of kind `Reqwest`).
* Library: `JobsFilter::days` and `JobQuery::days` are `Option<NonZeroU64>`, the schema's type (were `Option<i64>`);
  a value above 30 is still refused before any request. `i64::from(ObsId)` gives an obsid as the schema's `obs_id`.
* Library: `AsvoClient::cancel_job` returns the OpenAPI schema's `JobCancelledResponse` (was `JobSubmittedResponse`). In
  Python it is the new class `JobCancelledResponse`, and the `status` of both reply classes is the new enum `Status`
  (`Status.Success` or `Status.Failed`; `str()` is the API value), not a `str`.
* Library: `AsvoJobId` is a `NonZeroU64`, as the schema's `job_id` is (it was a `u64`). The commands refuse a job ID
  of 0 (`0 is not a job ID or an obsid`), and Python raises `ValueError` for it.
* Library: `AsvoJob` wraps the OpenAPI schema's `JobDetailResponse` (through `Deref`, so `job.job_state` and the other
  fields work as before), with its obsid and job ID checked: `AsvoJob::obs_id()` and `AsvoJob::job_id()` are methods
  now, and `AsvoJob::try_from(JobDetailResponse)` makes one (`AsvoError::InvalidJob` for a job that cannot be used).
* Library: `AsvoFilesArray`, `AsvoJobProduct` and `Delivery` are replaced by the OpenAPI schema's `JobFile`,
  `JobProduct` and `Type` (the delivery of a file). `JobFile::size_bytes` gives the size as a `u64`. `Delivery` is
  now the schema's `Delivery`, the delivery of a submission. In Python, `AsvoFilesArray` and `AsvoJobProduct` are
  replaced by `JobFile` and `JobProduct`, and `JobFile.type` is a `Type`.
* Library: `AsvoJobType` is replaced by the OpenAPI schema's `JobType` (a code), with `name()`, `names()` and
  `parse_name()`. `AsvoJob::job_type` is `Option<JobType>`. In Python, `AsvoJobType` is replaced by `JobType`, whose
  members are the codes (`JobType.Visibility == 1`).
* `--help` lists the allowed values of `--delivery`, `--delivery-format`, `--output`, `--centre`,
  `--output-mode`, `--pol` and `--weighting`, and the help of `list --job-states` and `--job-types` lists the
  names that the options accept (it listed `retrieving`, which is not a state).
* A failed job reports the MWA ASVO's `error_code` as well as its error text.
* `list` command now shows completed date time.
* `-v` now shows all API requests and responses. `-vv` shows the full payload of requests and responses.
* The environment variable `GIANT_SQUID_DOWNLOAD_RETRY_SECS` sets how long a failing download is retried.
* Docker:
  * Updated the docker build to use a docker hardened image and a two stage build approach.

### Added in 3.0.0

* `submit-image` takes six more options, as the MWA ASVO API (schema 1.13.0) does: `--no-digital-gains`, `--no-flag-dc`,
  `--no-geometry-delay`, `--no-passband-gains`, `--no-cable-delay` and `--no-rfi`, the same as `submit-conv` has. In
  Python they are the arguments of `submit_imaging_job`.
* giant-squid is now a Rust library with a public API that reads no environment variables and prints nothing
  (see "Using giant-squid as a Rust library" in the README). The library uses `jiff` for times, not `chrono`.
* The environment variables of the `giant-squid` command are read in the library, in one place, for both commands:
  the Rust functions `client_config_from_env` and `DownloadSettings::from_env`. The Python module does not read the
  environment. A
  `GIANT_SQUID_DOWNLOAD_RETRY_SECS` that is not a whole number of seconds is now a warning (the command ignored it
  silently), and a negative `GIANT_SQUID_BUF_SIZE` is refused.
* The names of the job states and job types (`queued`, `visibility` and so on) are listed once, in the library, and
  the help of `list --job-states` and `--job-types` is built from that list.
* The checks of the IDs and times that the command makes (`parse_obs_ids_only`, `parse_job_ids_only` and
  `parse_utc_time`) are in the library, once. The message for a job ID given to a command that takes obsids
  now says `job IDs`, not `exceptions`. `--image-size` shows the same message as the library
  (`Invalid image_size: ...`).
* A Python package, `mwa-giant-squid` (`import mwa_giant_squid`), built from the same code, with type stubs. It
  installs the `giant-squid` command, which is the Rust program run inside the module, so it is the same
  program as the Rust one (the same commands, options, help, messages and exit codes). See
  [docs/PYTHON.md](docs/PYTHON.md). The wheels are attached to each GitHub release.

* `download` without `--keep-tar` (stream untar) now resumes. A retry after a failed attempt (for example, a
  dropped connection) continues from where the attempt stopped. A new run of the same command continues from the
  files that are already in the download directory: complete files are not downloaded again, and a partly
  written file is continued. The SHA-1 still covers the whole archive. A resume file,
  `.<tar file name>.giant-squid-resume.json`, in the download directory lets a new run continue without reading
  the complete files again. It is deleted when the download is complete.
* `download` retries a network error during the transfer, with or without `--keep-tar`. Before, only a failed
  request was retried.

### Fixed in 3.0.0

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
* Negative numbers are accepted in the `--flag value` form, for example `--custom-dec -26.7`, `--robust -1.5`
  or `submit-volt --offset -1`. Before, clap read the value as an unknown flag (`unknown argument '-2'`), and
  only the `--flag=value` form worked. This applies to `submit-conv`, `submit-image`, `submit-image-from-job`
  and `submit-volt`.
* `--version` prints `giant-squid 3.0.0`. Before, it printed the name of the crate (`mwa_giant_squid`).

### Security in 3.0.0

* A stream-untar download (`download` without `--keep-tar`) no longer writes outside the download directory. A
  tar entry whose path is absolute (for example `/home/user/.bashrc`), contains `..` (for example
  `../../.ssh/authorized_keys`) or has no name is not written. giant-squid shows a warning for each such entry
  and unpacks the other entries. In 2.x, such an entry was written where its path pointed, so a bad archive could
  create or overwrite a file anywhere that the user can write. The SHA-1 check still covers the whole archive.
* `download` does not write through a symbolic link that is already in the download directory. A stream-untar entry
  whose file, or a directory on the way to it, is a symbolic link is skipped with a warning. A `--keep-tar` download
  whose tar file is a symbolic link (even one that points nowhere) fails with `AsvoError::SymlinkInDownloadDir`, and
  nothing is fetched. Before, the download wrote wherever the link pointed.

### Removed in 3.0.0

* The version 1 API client, and the `-p` / `--parameters` option (see above).
* The `expired` job state, and the library error `AsvoError::JobExpired`. The MWA ASVO API has no expired state.
* The `Unknown` job type. A job type that the schema does not list fails the listing; a job with no type has
  `job_type` `None`.

### Housekeeping in 3.0.0

* Tests moved into their own module: a module that has tests is a folder with `mod.rs` (the code) and `tests.rs`
  (the tests).
* A test suite that runs offline against a mock MWA ASVO server (recorded responses are replayed), so that no
  test submits or cancels a job or downloads data. See [docs/TESTING.md](docs/TESTING.md).
* Tests that keep decisions from being undone by accident: only parameters of the API schema are sent, only
  end-user endpoints are called, `staging_count` is never sent. See [docs/TESTING.md](docs/TESTING.md).
* `tools/run_live_tests.sh` runs the opt-in live tests against the MWA ASVO test server.
* CI checks that the generated API types and the Python type stubs are up to date, builds and tests the Python
  package on Linux and macOS, and attaches the wheels to releases.
* The crates.io package does not include the files that only the Python package uses.

# 2.5.1 - 2026-05-29

* Better error handling when MWA ASVO download has expired (and other 40X http status error codes).

# 2.5.0 - 2026-05-05

### Changed in 2.5.0

* Added support for imaging jobs and updated README.
* Ensure giant-squid does not fail if it encounters a job type it doesn't know about. Instead emit a warning for the user to update their giant-squid client.
* Added dependabot.yml to remove false positives (dependabot picking up vulns in packages which are not actually used, but are optional features of packages giant-squid does use).
* Updated dependencies to mitigate several security vulnerabilities.

### Fixed in 2.5.0

* Fixed divide by zero error when downloading a very small file over a very fast link.

# 2.4.0 - 2026-02-26

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
