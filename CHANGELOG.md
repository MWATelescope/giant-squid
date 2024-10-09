# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic
Versioning](https://semver.org/spec/v2.0.0.html).

## 1.2.0 - 2024-10-08

* Updated/migrated clap to v4.5.
* Updated quinn-proto to latest to fix security vulnerability.
* BUGFIX- the alias "sv" was assigned to both "submit-vis" and "submit-volt". "st" has now been assigned for "submit-volt" to avoid the duplication.
* Changed some console output references to "ASVO" to be "MWA ASVO".

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
