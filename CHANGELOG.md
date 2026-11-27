# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 3.0.0 - 2026-10-??

giant-squid 3.0.0 uses version 2 of the MWA ASVO API. Several defaults, option names and output formats changed:
**read [docs/V3_MIGRATION.md](docs/V3_MIGRATION.md) before you upgrade scripts.** The changes to the Rust library and
the Python API are in [docs/V3_LIBRARY.md](docs/V3_LIBRARY.md).

### Changed in 3.0.0

* The job options, their names, defaults and limits are the MWA ASVO API's. The defaults of conversion and imaging
  jobs changed: see [Conversion jobs][m-conv] and [Imaging jobs][m-img].
* `-p`/`--parameters` is gone. Each key is an option of its own, with the API's name, for example
  `--avg-time-res 0.5`. See [No more `--parameters`][m-params].
* `submit-image` takes Obs IDs only: image a conversion job with the new `submit-image-from-job`.
  `-o`/`--output-mode` is required. See [Imaging jobs][m-img].
* `download` puts the files of each job into a directory named for its Job ID (`-d /data` gives `/data/12345`). Use
  `--no-job-subdir` for the 2.x layout. See [Downloading][m-dl].
* The default delivery format is `tar` for every job. `submit-volt` has no delivery options. See
  [Other submit commands][m-submit].
* All log lines go to standard error. With `--json`, every command prints only JSON on standard output, also its
  errors. See [Output][m-out].
* `list`, `wait` and `download` of an Obs ID see only the past 30 days by default. `list --json` and `wait --json`
  use the API's keys, job states and job types. See [Listing jobs][m-list] and [JSON output][m-json].
* The submit commands, `download` and `cancel` try every ID, and exit with code 1 if any failed. `wait` and `cancel`
  refuse an Obs ID. See [Exit codes][m-exit] and [Waiting and cancelling][m-wait].

### Added in 3.0.0

* `submit-download` (`sd`) submits every kind of download job, and `submit-preprocess` (`sp`) a conversion job. See
  [Other submit commands][m-submit].
* `submit-csv` (`scsv`) submits the jobs of a manta-ray-client CSV file. See the
  [README](README.md#jobs-from-a-manta-ray-client-csv-file).
* `download` without `--keep-tar` resumes. `download` retries a failed transfer for `GIANT_SQUID_DOWNLOAD_RETRY_SECS`
  (default 900 s). See [Downloading][m-dl].
* `list` and `wait` show a completed job as `completed (ready)` or `completed (expired)`. `download` skips a job whose
  links have expired (`JOB_EXPIRED`). See [Expired download links][m-exp].
* `download`, `cancel` and `wait` read IDs from standard input when an argument is `-`, for example the output of the
  new `list --ids`. See [Job IDs and Obs IDs][m-ids].
* `list`: `--date-from`, `--date-to`, `--sort-by` and `--fields`. `--job-states` and `--job-types` replace `--states`
  and `--types`, which still work. See [Listing jobs][m-list].
* `--json` on every submit command, `download` and `cancel`. `--dry-run` prints the request body. `--version` prints
  the MWA ASVO API version. Each option that can be `null` takes the word `null`. See [Option values][m-values].
* A Python package, `mwa-giant-squid`, with the API of the library and the `giant-squid` command. See
  [docs/PYTHON.md](docs/PYTHON.md) and [docs/V3_LIBRARY.md](docs/V3_LIBRARY.md#python-module).

### Fixed in 3.0.0

* `download --keep-tar` resume sent a wrong range header. The whole file was appended to the partial file, and the
  SHA-1 covered only the new bytes. See [Fixes to `download`][m-fix].
* An ID that is given twice is used once. `download` of a Job ID and of its Obs ID no longer downloads the job twice
  into the same directory, which could corrupt its files.
* `download` of a DUG job, or of a Scratch job that this host cannot reach, fails with a non-zero exit code. Before,
  it reported success.
* `download` of a reachable Scratch job moves the files into the download directory, also across file systems.
  Before, they went to the current directory.
* `download` logs in and gets the job list once. Before, a long list of downloads could hit the server's limit on
  logins.
* `download` no longer panics on a job file with no SHA-1 hash, URL or path.
* `--no-resume` downloads a partial file again. `--skip-hash` does not skip the check of a resumed download.
* Negative numbers work in the `--option value` form (`--offset -1`). A pipe that closes early
  (`giant-squid list | head -1`) no longer makes giant-squid panic.

### Security in 3.0.0

* A stream-untar download does not write a tar entry whose path is absolute, contains `..` or has no name. In 2.x, a
  bad archive could overwrite any file that the user can write. See [Safety of the download directory][m-safe].
* A stream-untar download unpacks only files and directories. It skips a link or a special file with a warning.
* `download` does not write through a symbolic link that is in the download directory. See the
  [README](README.md#symbolic-links-in-the-download-directory).
* The logs show no credential, also with `-vv`: no API key, no token and no complete signed URL.
* The text of a job, and the `Debug` text of the client and its config, show no signed URL, API key or token. See
  [docs/V3_LIBRARY.md](docs/V3_LIBRARY.md#security).
* The login, refresh and API requests follow a redirect only to the same host and port.
* A client for an `http://` host warns that the API key and the session are sent in clear text.
* A Scratch job is moved only when its path is a folder named for its Job ID.

### Deprecated in 3.0.0

* `submit-vis`, `submit-meta`, `submit-volt`, `submit-bf` and `submit-conv` still work, but each logs a warning. They
  will be removed in a later release. See [Deprecated submit commands][m-depr].
* `--legacy-json` of `list` and `wait` prints the 2.x JSON. It will be removed in a later release.

### Removed in 3.0.0

* The version 1 API client.
* The `-p` / `--parameters` option of `submit-conv` and `submit-image`. See [No more `--parameters`][m-params].
* The `expired` job state. The MWA ASVO API has no such state. A job whose links have expired is `completed (expired)`.
* The `Unknown` job type. Every job has a job type.
* The short flags `-f` and `-t` of `submit-volt`, and `-n` of `list` and `wait`. See [Short flags][m-short].
* Library: `AsvoError::Expired`, `Obsid::from_string`, the `built_info` module and `AsvoJobMap`. See
  [docs/V3_LIBRARY.md](docs/V3_LIBRARY.md#rust-library-removed).

The entries of the development of 3.0.0, as they were written, are in [docs/history/CHANGELOG_3.0.0_DEVELOPMENT.md](docs/history/CHANGELOG_3.0.0_DEVELOPMENT.md).

[m-conv]: docs/V3_MIGRATION.md#conversion-jobs-submit-preprocess
[m-img]: docs/V3_MIGRATION.md#imaging-jobs-submit-image-and-submit-image-from-job
[m-params]: docs/V3_MIGRATION.md#no-more--p----parameters
[m-values]: docs/V3_MIGRATION.md#option-values-and-help
[m-submit]: docs/V3_MIGRATION.md#other-submit-commands
[m-depr]: docs/V3_MIGRATION.md#deprecated-submit-commands
[m-out]: docs/V3_MIGRATION.md#output-logs-go-to-stderr
[m-exit]: docs/V3_MIGRATION.md#messages-errors-and-exit-codes
[m-list]: docs/V3_MIGRATION.md#listing-jobs-list
[m-wait]: docs/V3_MIGRATION.md#waiting-and-cancelling-wait-cancel
[m-json]: docs/V3_MIGRATION.md#json-output-list---json-wait---json
[m-dl]: docs/V3_MIGRATION.md#downloading-download
[m-exp]: docs/V3_MIGRATION.md#expired-download-links
[m-fix]: docs/V3_MIGRATION.md#fixes-to-download
[m-safe]: docs/V3_MIGRATION.md#safety-of-the-download-directory
[m-ids]: docs/V3_MIGRATION.md#job-ids-and-obs-ids
[m-short]: docs/V3_MIGRATION.md#short-flags

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
