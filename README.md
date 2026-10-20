# giant-squid

![Tests](https://github.com/MWATelescope/giant-squid/workflows/Cross-platform%20tests/badge.svg)
[![Code Coverage](https://github.com/MWATelescope/giant-squid/actions/workflows/coverage.yml/badge.svg)](https://github.com/MWATelescope/giant-squid/actions/workflows/coverage.yml)
[![codecov](https://codecov.io/gh/MWATelescope/giant-squid/branch/main/graph/badge.svg)](https://app.codecov.io/gh/MWATelescope/giant-squid/)
[![Crates.io](https://img.shields.io/crates/v/mwa_giant_squid)](https://crates.io/crates/mwa_giant_squid)
![Crates.io](https://img.shields.io/crates/d/mwa_giant_squid)
![Crates.io](https://img.shields.io/crates/l/mwa_giant_squid)
[![docs](https://docs.rs/mwa_giant_squid/badge.svg)](https://docs.rs/crate/mwa_giant_squid/latest)

An alternative [MWA ASVO](https://asvo.mwatelescope.org/) client. For general help on using
the MWA ASVO, please visit: [MWA ASVO wiki](https://mwatelescope.atlassian.net/wiki/spaces/MP/pages/24973129/Data+Access).

---
## Upgrading from giant-squid 2.x

giant-squid 3.0.0 uses version 2 of the MWA ASVO API, and some options, defaults and outputs
changed. See [docs/V3_MIGRATION.md](docs/V3_MIGRATION.md) for what to change in your commands and
scripts.

---
## NOTE FOR HPC USERS

Please read [this wiki article](https://mwatelescope.atlassian.net/wiki/spaces/MP/pages/65405030/MWA+ASVO+Use+with+HPC+Systems)
if you are running giant-squid on HPC systems.

---

`giant-squid` was originally created as a library to do MWA ASVO related tasks
in the Haskell programming language (it is now available in Rust). However, it's not
just a library; the `giant-squid` executable acts as an alternative to the
[manta-ray-client](https://github.com/ICRAR/manta-ray-client) and may better
suit users for a few reasons:

1. By default, `giant-squid` _stream untars_ the downloads from MWA ASVO. In other
   words, rather than downloading a potentially large (> 100 GiB!) tar file and
   then untarring it yourself (thereby occupying double the space of the
   original tar and performing a very expensive IO operation), it is possible to
   get the files without performing an untar using `--keep-tar`

2. `giant-squid` resumes interrupted or failed downloads, with or without
   `--keep-tar`. If you run the download command again, it continues where it
   stopped, and it does not download again the files that are already complete.
   See [Resuming Interrupted Downloads](#resuming-interrupted-downloads).

3. `giant-squid` does not require a CSV file to submit jobs; this is instead
   handled by command line arguments.

4. For any commands that accept Obs IDs or Job IDs, it is possible use text files
   instead. These files are unpacked as if you had typed them out manually, and
   each entry of the text file(s) are checked for validity (all ints and all
   10-digits long); any exceptions are reported and the command fails.

5. One can ask `giant-squid` to print your MWA ASVO queue as JSON; this makes
   parsing the state of your jobs in another programming language much simpler.

6. By default, `giant-squid` will validate the hash of the archive. You can skip
   this check with `--skip-hash`, except when `giant-squid` resumes a download
   or uses files that are already on disk (see
   [Resuming Interrupted Downloads](#resuming-interrupted-downloads))

---

## Table of Contents

- [Authentication](#authentication)
- [Usage](#usage)
  - [Print help text](#print-help-text)
  - [Print the giant-squid version](#print-the-giant-squid-version)
  - [Submit MWA ASVO jobs](#submit-mwa-asvo-jobs)
    - [A Note On Delivery Options](#a-note-on-delivery-options)    
    - [Options that every submit command has](#options-that-every-submit-command-has)
    - [Conversion downloads](#conversion-downloads)
    - [Imaging downloads](#imaging-downloads)
    - [Imaging from a conversion job](#imaging-from-a-conversion-job)
    - [Metadata downloads](#metadata-downloads)
    - [Visibility downloads](#visibility-downloads)
    - [Beamformer downloads](#beamformer-downloads)
    - [Voltage downloads](#voltage-downloads)    
    - [Jobs from a manta-ray-client CSV file](#jobs-from-a-manta-ray-client-csv-file)
    - [Resubmitting jobs](#resubmitting-jobs)
  - [List MWA ASVO jobs](#list-mwa-asvo-jobs)
  - [List MWA ASVO jobs in JSON](#list-mwa-asvo-jobs-in-json)
  - [Filter MWA ASVO job listing](#filter-mwa-asvo-job-listing)
  - [Example: manual hash validation with Bash and jq](#example-manual-hash-validation-with-bash-and-jq)
  - [Wait for MWA ASVO jobs](#wait-for-mwa-asvo-jobs)
  - [Cancel MWA ASVO jobs](#cancel-mwa-asvo-jobs)
  - [Download MWA ASVO jobs](#download-mwa-asvo-jobs)
  - [Use the output of `list` as the input of other commands](#use-the-output-of-list-as-the-input-of-other-commands)
- [Installation](#installation)
  - [Pre-compiled](#pre-compiled)
  - [Building from crates.io](#building-from-cratesio)
  - [Building from source](#building-from-source)
- [Docker](#docker)
- [Using giant-squid as a Rust library](#using-giant-squid-as-a-rust-library)
- [Environment Variables](#environment-variables)
- [Background](#background)

---

## Authentication

`giant-squid` authenticates with the MWA ASVO API using an API key. You must set the following
environment variable before running any commands:

```bash
export MWA_ASVO_API_KEY="your-api-key-here"
```

To obtain your API key:

1. Log in to the [MWA ASVO portal](https://asvo.mwatelescope.org/)
2. Navigate to your profile/account settings
3. Copy your API key

It is recommended to add the `export` line to your shell profile (e.g. `~/.bashrc` or
`~/.bash_profile`) so it is set automatically in every session.

### Session caching

Behind the scenes, `giant-squid` exchanges your API key for a short-lived session (JWT access/refresh
tokens) with the MWA ASVO. This session is cached at `$HOME/.mwa-asvo/tokens.json` (permissions `0600` from the moment the file exists)
and reused across commands until it expires, so you won't see a fresh login on every invocation. The
file also holds the host and a hash of the API key (never the key
itself): `giant-squid` uses a session only for the host and the API key that it was made for, so a command with
another API key logs in again and replaces it. A file written by `mwa-cli` or by an older `giant-squid` has neither,
so `giant-squid` does not use it either. `giant-squid` makes the `.mwa-asvo` directory with permissions `0700`. If the directory
is already there and other users have any permission on it, `giant-squid` logs a warning (with the `chmod` command that
closes it) and does not change it. You can safely delete this file at any time - `giant-squid` will 
just log in again using your API key.

---

## Usage

### Print help text

```bash
giant-squid --help
```

This also applies to all of the commands, e.g.

```bash
giant-squid download --help
```

### Print the `giant-squid` version

(Useful if things are changing over time!)

```bash
giant-squid --version
```

It also prints the version of the MWA ASVO API and of the OpenAPI specification that this version of giant-squid was
built for:

```
giant-squid 3.0.0
MWA ASVO API 2.3.4
OpenAPI 3.1.0
```

A Rust program can read them as `mwa_giant_squid::mwa_asvo::api::openapi::MWA_ASVO_API_VERSION` and
`OPENAPI_VERSION`.

### Submit MWA ASVO jobs

If you upgrade from giant-squid 2.x, the job options changed: see the [3.0 migration guide](docs/V3_MIGRATION.md).

Each kind of job has its own command, and each command has a short name:

| Command | Short name | Job |
|---|---|---|
| `submit-vis` | `sv` | [Visibility download](#visibility-downloads) |
| `submit-meta` | `sm` | [Metadata download](#metadata-downloads) |
| `submit-conv` | `sc` | [Conversion](#conversion-downloads) |
| `submit-image` | `si` | [Imaging](#imaging-downloads) |
| `submit-image-from-job` | `sifj` | [Imaging from a conversion job](#imaging-from-a-conversion-job) |
| `submit-bf` | `sb` | [Beamformer download](#beamformer-downloads) |
| `submit-volt` | `st` | [Voltage download](#voltage-downloads) |
| `submit-csv` | `scsv` | [The jobs of a manta-ray-client CSV file](#jobs-from-a-manta-ray-client-csv-file) |

Every submit command except `submit-csv` takes one or more Obs IDs, or text files that hold Obs IDs. It submits one job for
each Obs ID and carries on if one fails. It ends with a summary and a non-zero exit code if any failed.

#### Options that every submit command has

| Option | Meaning |
|---|---|
| `-d`, `--delivery` | Where MWA ASVO delivers the data: see [A Note On Delivery Options](#a-note-on-delivery-options). `submit-volt` takes `scratch` only |
| `-f`, `--delivery-format` | `tar` (the default) or `files`. `submit-volt` does not have it, because voltage data is always delivered as files |
| `-r`, `--allow-resubmit` | Submit the job even if an identical one has completed (`-r` alone is true; the value can be true, false or null): see [Resubmitting jobs](#resubmitting-jobs) |
| `-w`, `--wait` | Do not exit until the jobs are ready for download |
| `-n`, `--dry-run` | Do not submit. Print the request that would be sent for each Obs ID |
| `-j`, `--json` | Print the MWA ASVO's reply for each submitted job as one line of JSON on standard output. See [JSON output and errors](#json-output-and-errors) |
| `-v`, `--verbosity` | Show more log messages. Repeat it for more. Cannot be used with `--json` |

#### Sending `null`

The MWA ASVO allows some request fields to be `null`, and `null` is not the same as the field's default. An option for such a field takes the word `null` (in any case): `--avg-freq-res null`, `--no-rfi=null` or `--allow-resubmit=null`. The options are `--avg-freq-res`, `--avg-time-res`, `--abs-threshold`, `--clean-threshold`, `--no-digital-gains`, `--no-flag-dc`, `--no-geometric-delay`, `--no-passband-gains`, `--no-cable-delay`, `--no-rfi` and `-r`/`--allow-resubmit`. The flags also take `true` and `false` (`--no-rfi=false`); the bare flag, for example `--no-rfi` or `-r`, is `true`. The MWA ASVO does not say what it does with a `null` for each field: use `--dry-run` to see the request.

Log messages go to standard error, so standard output has only the output of the command (for example, the
`--json` lines).

#### JSON output and errors

With `-j`, `--json`, every command prints only JSON, on standard output. It prints no log messages and no
progress bars, and nothing on standard error. Each error is one line of JSON with `"status": "failed"`, then the MWA ASVO's error
response: `error_code` and `message`, and `detail`, `suggestion`, `field_errors` and `request_id` when they are
known. An error about one Obs ID or job also has its `obs_id` or `job_id`, and an error about one line of a CSV file
(`submit-csv`) has its `line`, counted from 1. A script finds an error line by its
`error_code` key. The line of a result has `status` as its first key, and it is `success` or `failed`: the reply of
the MWA ASVO to a submission or a cancellation (as the MWA ASVO sent it; a refused cancellation of a job that is
already cancelled is a reply with `failed`), a download (`success`) or an error (`failed`). A listing, and a dry run, are not results of that kind and have no `status`. The exit code is 1 if anything failed, and 2 for a bad command line.

An error that the MWA ASVO sent in this form is printed as the MWA ASVO sent it. Other errors have a giant-squid
error code, for example `HTTP_503` (an HTTP error, with the body of the reply as the `detail`), `NETWORK_ERROR`,
`INVALID_ARGUMENT` or `JOB_NOT_FOUND`:

```bash
$ giant-squid submit-vis 1065880128 --json
{"status":"failed","error_code":"HTTP_503","message":"Service Unavailable","detail":"<html>...</html>\n","obs_id":1065880128}
``` To check that your command works without submitting anything, use `--dry-run`:

```bash
$ giant-squid submit-vis 1065880128 --dry-run
13:10:06 [INFO] [dry run] Would POST /api/v2/download_vis_job for Obs ID 1065880128:
{
  "allow_resubmit": false,
  "delivery": "acacia",
  "delivery_format": "tar",
  "download_type": "vis",
  "obs_id": 1065880128
}
13:10:06 [INFO] [dry run] Would have submitted 1 Obs IDs to /api/v2/download_vis_job. Nothing was sent.
```

The defaults of the job options are those of the MWA ASVO API. Run `giant-squid <command> --help` to see them.

#### A Note On Delivery Options

Before submitting any MWA ASVO job, you will need to decide _where_ you want the data to be delivered. There are up to three options depending on your user profile.

##### Delivery: Acacia (Default)

- The default option for all job types except voltage downloads (`Voltage` jobs are not able to be delivered to Acacia due to their size).
- Files are tarred up and uploaded to Pawsey's Acacia object store.
- To submit a job with the Acacia delivery option specify `--delivery=acacia` on any job submission command.
- A URL which expires in 7 days is generated- allowing you to download the file via giant-squid, wget, curl, etc from anywhere in the world.
- Acacia delivers a tar only: `--delivery-format=files` with the Acacia delivery is an error, in giant-squid before anything is sent. Use `--delivery-format=tar` (the default), or the scratch or DUG delivery to get the individual files.

##### Delivery: Pawsey Scratch Filesystem

- You can request that your job's files be delivered to Pawsey's /scratch filesystem.
- To submit a job with the scratch delivery option, specify `--delivery=scratch` on any job submission command.
- MWA ASVO delivers a tar of the files by default. To get the individual files instead, pass `--delivery-format=files`.
- This option is only available to users who have a Pawsey account with MWA group access and your `Pawsey Group` has been set in your MWA ASVO profile by an MWA ASVO administrator.
  - Please contact support to request this.  
- NOTE: all Pawsey users in the specified Pawsey Group can access your job's files. If you prefer to keep your data private to only you, you should choose the `acacia` delivery option as only you have the download URL.

##### Delivery: Down Under Geosolutions (DUG) Filesystem

- You can request that your job's files be delivered to DUG's filesystem.
- MWA ASVO delivers a tar of the files by default. To get the individual files instead, pass `--delivery-format=files`.
- To submit a job with the DUG delivery option, specify `--delivery=dug` on any job submission command.
- Visibility, metadata, beamformer, conversion and imaging jobs can be delivered to DUG. `Voltage` jobs cannot (they are delivered to Scratch only).
- This option is only open to users who have a Curtin University DUG account and your `DUG Group` has been set in your MWA ASVO profile by an MWA administrator.
  - Please contact support to request this.
- NOTE: all DUG users in the specified DUG Group can access your job's files. If you prefer to keep your data private to only you, you should choose the `acacia` delivery option as only you have the download URL.

##### Changing Your Default Delivery Option

- You can set the environment variable `GIANT_SQUID_DELIVERY` to `acacia`, `scratch` or `dug` if you don't want to keep specifying the delivery option on the command line.
- In the same way, `GIANT_SQUID_DELIVERY_FORMAT` (`tar` or `files`) sets the default delivery format.

#### Conversion downloads

Conversion jobs refer to jobs which convert raw visibilities into either CASA measurement set or UVFITS format while optionally RFI flagging, averaging, correcting and applying calibration solutions to the converted data.

Conversion jobs use the Birli software package to preprocess MWA raw visibilities. For more information about Birli please see: [Birli on GitHub](https://github.com/MWATelescope/Birli).

```text
Submit MWA ASVO preprocessing/conversion jobs

Usage: giant-squid submit-conv [OPTIONS] [OBS_ID]...

Arguments:
  [OBS_ID]...  The Obs IDs to be submitted. Files containing Obs IDs are also accepted

Options:
  -d, --delivery <DELIVERY>
          Tell MWA ASVO where to deliver the data [env: GIANT_SQUID_DELIVERY=] [default: acacia] [possible values: acacia, scratch, dug]
  -f, --delivery-format <DELIVERY_FORMAT>
          Tell MWA ASVO to deliver the data in a particular format. Must be tar when the delivery is acacia [env: GIANT_SQUID_DELIVERY_FORMAT=] [default: tar] [possible values: tar, files]
  -o, --output <OUTPUT>
          Output format: "ms" (measurement set) or "uvfits" [default: ms] [possible values: ms, uvfits]
      --avg-freq-res <AVG_FREQ_RES>
          Frequency resolution to average to (kHz), or null [default: 40]
      --avg-time-res <AVG_TIME_RES>
          Time resolution to average to (s), or null [default: 2]
      --flag-edge-width <FLAG_EDGE_WIDTH>
          Width of frequency edge flagging (kHz) [default: 80]
      --apply-di-cal[=<APPLY_DI_CAL>]
          Whether to apply the DI calibration solution [default: false] [possible values: true, false]
      --centre <CENTRE>
          Phase centre: "phase", "pointing", or "custom". If "custom", also supply --custom-centre-ra and --custom-centre-dec [default: phase] [possible values: phase, pointing, custom]
      --custom-centre-ra <CUSTOM_CENTRE_RA>
          Custom phase centre right ascension (degrees). Requires --centre custom
      --custom-centre-dec <CUSTOM_CENTRE_DEC>
          Custom phase centre declination (degrees). Requires --centre custom
      --no-apply-amps[=<NO_APPLY_AMPS>]
          Whether to skip applying amplitude calibration solutions [default: false] [possible values: true, false]
      --no-digital-gains[=<NO_DIGITAL_GAINS>]
          Whether to skip applying digital gains. Takes true, false or null [default: false]
      --no-flag-dc[=<NO_FLAG_DC>]
          Whether to skip flagging the DC channel. Takes true, false or null [default: false]
      --no-geometric-delay[=<NO_GEOMETRIC_DELAY>]
          Whether to skip applying geometric delay corrections. Takes true, false or null [default: false]
      --no-passband-gains[=<NO_PASSBAND_GAINS>]
          Whether to skip applying passband gain corrections. Takes true, false or null [default: false]
      --no-cable-delay[=<NO_CABLE_DELAY>]
          Whether to skip applying cable delay corrections. Takes true, false or null [default: false]
      --no-rfi[=<NO_RFI>]
          Whether to skip RFI flagging. Takes true, false or null [default: false]
  -r, --allow-resubmit[=<ALLOW_RESUBMIT>]
          Allow resubmitting a job even if an identical one has completed. Takes true, false or null [default: false]
  -w, --wait
          Do not exit giant-squid until the specified Obs IDs are ready for download
  -n, --dry-run
          Don't actually submit; print information on what would've happened instead
  -j, --json
          Print each submitted job's response from the MWA ASVO as one line of JSON on stdout, and each error as an MWA ASVO ErrorResponse with the obs_id. No logs are printed
  -v, --verbosity...
          The verbosity of the program. The default is to print high-level information. Cannot be used with --json
  -h, --help
          Print help
  -V, --version
          Print version
```

To submit a conversion job for Obs ID 1065880128:

```bash
giant-squid submit-conv 1065880128
```

Text files containing Obs IDs may be used too.

To change the conversion options, give them as options. For example, to average to 0.5 s and 10 kHz and write a
UVFITS file:

```bash
giant-squid submit-conv 1065880128 --output uvfits --avg-time-res 0.5 --avg-freq-res 10
```

To use a custom phase centre:

```bash
giant-squid submit-conv 1065880128 --centre custom --custom-centre-ra 12.5 --custom-centre-dec -26.7
```

The two coordinates go together with `--centre custom`: `giant-squid` refuses one of them without the other, and
refuses either of them with another `--centre`. The same holds for `submit-image`.

Conversion jobs can be delivered to Acacia, Scratch or DUG.

##### Options for conversion jobs

In addition to the [options that every submit command has](#options-that-every-submit-command-has):

| Option | Meaning | Values | Default |
|---|---|---|---|
| `-o`, `--output` | Output data format (CASA measurement set or UVFITS) | one of `ms`, `uvfits` | ms |
| `--avg-freq-res` | Output frequency resolution in kHz. Must be a multiple of, or equal to, the correlator frequency resolution | 0 to 1280, or null | 40 |
| `--avg-time-res` | Output time resolution in seconds. Must be a multiple of, or equal to, the correlator time resolution | 0 or more, or null | 2 |
| `--flag-edge-width` | Width in kHz to flag at each coarse channel edge. Must be a multiple of, or equal to, the correlator frequency resolution | 0 to 640 | 80 |
| `--apply-di-cal` | Apply the basic direction-independent calibration solution (if available). `--apply-di-cal` alone is `--apply-di-cal=true` | `true` or `false` | false |
| `--centre` | Phase centre to use | one of `phase`, `pointing`, `custom` | phase |
| `--custom-centre-ra` | Right ascension in decimal degrees of the custom phase centre. Needs `--centre custom` | 0 to 359.999999 | none |
| `--custom-centre-dec` | Declination in decimal degrees of the custom phase centre. Needs `--centre custom` | -90 to 90 | none |
| `--no-apply-amps` | Whether to skip applying amplitude calibration solutions | flag, or `true` or `false` | false |
| `--no-digital-gains` | Do not correct the digital gains | flag, or true, false or null | false |
| `--no-flag-dc` | Do not flag the DC channel | flag, or true, false or null | false |
| `--no-geometric-delay` | Do not correct geometric delays (only applicable if not already applied by the correlator) | flag, or true, false or null | false |
| `--no-passband-gains` | Do not correct the passband gains | flag, or true, false or null | false |
| `--no-cable-delay` | Do not correct cable length delays (only applicable if not already applied by the correlator) | flag, or true, false or null | false |
| `--no-rfi` | Will disable radio frequency interference (RFI) flagging | flag, or true, false or null | false |

#### Imaging downloads

An "imaging download job" takes the raw visibilities of an Obs ID and produces an image. The raw visibilities are converted to a CASA measurement set first, just like a regular [Conversion](#conversion-downloads) job. To image an existing, completed conversion job instead, see [Imaging from a conversion job](#imaging-from-a-conversion-job).

The MWA ASVO imaging features uses the WSClean software by André Offringa to generated images from CASA measurement sets. For comprehensive documentation about WSClean, please see: [WSClean readthedocs](https://wsclean.readthedocs.io/).

```text
Submit MWA ASVO imaging jobs

Usage: giant-squid submit-image [OPTIONS] [OBS_ID]...

Arguments:
  [OBS_ID]...  The Obs IDs to submit for imaging. Files containing Obs IDs are also accepted. All Obs IDs in one invocation share the same parameters above

Options:
  -d, --delivery <DELIVERY>
          Tell MWA ASVO where to deliver the data [env: GIANT_SQUID_DELIVERY=] [default: acacia] [possible values: acacia, scratch, dug]
  -f, --delivery-format <DELIVERY_FORMAT>
          Tell MWA ASVO to deliver the data in a particular format. Must be tar when the delivery is acacia [env: GIANT_SQUID_DELIVERY_FORMAT=] [default: tar] [possible values: tar, files]
      --avg-freq-res <AVG_FREQ_RES>
          Frequency resolution to average to (kHz), or null [default: 40]
      --avg-time-res <AVG_TIME_RES>
          Time resolution to average to (s), or null [default: 2]
      --flag-edge-width <FLAG_EDGE_WIDTH>
          Width of frequency edge flagging (kHz) [default: 80]
      --apply-di-cal[=<APPLY_DI_CAL>]
          Whether to apply the DI calibration solution [default: true] [possible values: true, false]
      --centre <CENTRE>
          Phase centre: "phase", "pointing", or "custom". If "custom", also supply --custom-centre-ra and --custom-centre-dec [default: phase] [possible values: phase, pointing, custom]
      --custom-centre-ra <CUSTOM_CENTRE_RA>
          Custom phase centre right ascension (degrees). Requires --centre custom
      --custom-centre-dec <CUSTOM_CENTRE_DEC>
          Custom phase centre declination (degrees). Requires --centre custom
      --no-apply-amps[=<NO_APPLY_AMPS>]
          Whether to skip applying amplitude calibration solutions [default: false] [possible values: true, false]
      --no-digital-gains[=<NO_DIGITAL_GAINS>]
          Whether to skip applying digital gains. Takes true, false or null [default: false]
      --no-flag-dc[=<NO_FLAG_DC>]
          Whether to skip flagging the DC channel. Takes true, false or null [default: false]
      --no-geometric-delay[=<NO_GEOMETRIC_DELAY>]
          Whether to skip applying geometric delay corrections. Takes true, false or null [default: false]
      --no-passband-gains[=<NO_PASSBAND_GAINS>]
          Whether to skip applying passband gain corrections. Takes true, false or null [default: false]
      --no-cable-delay[=<NO_CABLE_DELAY>]
          Whether to skip applying cable delay corrections. Takes true, false or null [default: false]
      --no-rfi[=<NO_RFI>]
          Whether to skip RFI flagging. Takes true, false or null [default: false]
      --apply-primary-beam[=<APPLY_PRIMARY_BEAM>]
          Whether to apply the primary beam correction [default: true] [possible values: true, false]
      --auto-mask <AUTO_MASK>
          WSClean -auto-mask value [default: 3]
      --auto-threshold <AUTO_THRESHOLD>
          WSClean -auto-threshold value [default: 0.5]
      --abs-threshold <ABS_THRESHOLD>
          Absolute cleaning threshold (Jy), or null [default: 0.001]
      --channels-out <CHANNELS_OUT>
          Number of output channel groups [default: 4]
      --clean-iterations <CLEAN_ITERATIONS>
          WSClean -niter value (max clean iterations) [default: 100000]
      --clean-threshold <CLEAN_THRESHOLD>
          WSClean cleaning threshold (Jy), or null [default: 0.001]
      --image-size <IMAGE_SIZE>
          WSClean image size in pixels [default: 3072]
      --join-channels[=<JOIN_CHANNELS>]
          Join output channel groups for cleaning [default: true] [possible values: true, false]
      --join-polarizations[=<JOIN_POLARIZATIONS>]
          Join polarisations for cleaning [default: false] [possible values: true, false]
      --mgain <MGAIN>
          WSClean -mgain value [default: 0.8]
      --multiscale[=<MULTISCALE>]
          Enable WSClean multiscale cleaning [default: false] [possible values: true, false]
      --nmiter <NMITER>
          WSClean -nmiter value (max major cleaning iterations) [default: 10]
      --nwlayers <NWLAYERS>
          Number of w-projection layers. Leave unset to let the server decide
  -o, --output-mode <OUTPUT_MODE>
          The output mode / product to request [default: fits] [possible values: fits, all_fits, all_files]
      --pixel-scale <PIXEL_SCALE>
          Pixel scale (arcsec/pixel) [default: 20]
      --pol <POL>
          Polarisation to image: XX, YY or XXYY [default: XXYY] [possible values: XX, YY, XXYY]
      --robust <ROBUST>
          WSClean -robust (Briggs robustness) value [default: -0.5]
      --uvw-max <UVW_MAX>
          Maximum uv distance to image, in wavelengths (upper bound on the range that can be requested)
      --uvw-min <UVW_MIN>
          Minimum uv distance to image, in wavelengths [default: 75]
      --weighting <WEIGHTING>
          WSClean weighting scheme [default: briggs] [possible values: briggs, uniform, natural]
      --wstack-nwlayers <WSTACK_NWLAYERS>
          Number of w-stacking layers. Leave unset to let the server decide
  -r, --allow-resubmit[=<ALLOW_RESUBMIT>]
          Allow resubmitting a job even if an identical one has completed. Takes true, false or null [default: false]
  -w, --wait
          Do not exit giant-squid until the specified Obs IDs are ready for download
  -n, --dry-run
          Don't actually submit; print information on what would've happened instead
  -j, --json
          Print each submitted job's response from the MWA ASVO as one line of JSON on stdout, and each error as an MWA ASVO ErrorResponse with the obs_id. No logs are printed
  -v, --verbosity...
          The verbosity of the program. The default is to print high-level information. Cannot be used with --json
  -h, --help
          Print help
  -V, --version
          Print version
```

To submit an imaging job for the Obs ID 1065880128, give any conversion options as well as imaging options:

```bash
giant-squid submit-image 1065880128 --avg-time-res 0.5 --avg-freq-res 10 --image-size 2048 --multiscale
```

Imaging jobs can be delivered to Acacia, Scratch or DUG.

##### Options for imaging jobs

In addition to the [options that every submit command has](#options-that-every-submit-command-has):

| Option | Meaning | Values | Default |
|---|---|---|---|
| `--apply-di-cal` | Apply the basic direction-independent calibration solution (if available) | `true` or `false`, given as `--apply-di-cal=false` | true |
| `--apply-primary-beam` | Calculate and apply the primary beam and save images for the Jones components, with weighting identical to the weighting as used by the imager | `true` or `false`, given as `--apply-primary-beam=false` | true |
| `--auto-mask` | WSClean -auto-mask value | 2 to 512 | 3 |
| `--auto-threshold` | Relative clean threshold. Estimate noise level using a robust estimator and stop at sigma x stddev | 0.1 to 5 | 0.5 |
| `--abs-threshold` | Absolute cleaning threshold (Jy) | 0 to 10, or null | 0.001 |
| `--avg-freq-res` | Output frequency resolution in kHz. Must be a multiple of, or equal to, the correlator frequency resolution | 0 to 1280, or null | 40 |
| `--avg-time-res` | Output time resolution in seconds. Must be a multiple of, or equal to, the correlator time resolution | 0 or more, or null | 2 |
| `--channels-out` | Number of output channel groups | any whole number | 4 |
| `--clean-iterations` | Maximum number of clean iterations to perform | at most 1000000 | 100000 |
| `--clean-threshold` | Absolute stopping clean thresholding in Jy | 0 to 10, or null | 0.001 |
| `--custom-centre-dec` | Declination in decimal degrees of the custom phase centre. Needs `--centre custom` | -90 to 90 | none |
| `--custom-centre-ra` | Right ascension in decimal degrees of the custom phase centre. Needs `--centre custom` | 0 to 359.999999 | none |
| `--flag-edge-width` | Width in kHz to flag at each coarse channel edge. Must be a multiple of, or equal to, the correlator frequency resolution | 0 to 640 | 80 |
| `--image-size` | width and height in pixels of output image | one of `512`, `1024`, `2048`, `3072`, `4096`, `8192` | 3072 |
| `--join-channels` | Join output channel groups for cleaning | `true` or `false`, given as `--join-channels=false` | true |
| `--join-polarizations` | Join polarisations for cleaning | flag, or `true` or `false` | false |
| `--mgain` | WSClean -mgain value | 0.1 to 1 | 0.8 |
| `--multiscale` | Clean on different scales. This is a new algorithm. This parameter invokes the optimized multiscale algorithm published by Offringa & Smirnov (2017) | flag, or `true` or `false` | false |
| `--nmiter` | WSClean -nmiter value (max major cleaning iterations) | 1 to 500 | 10 |
| `--nwlayers` | Number of w-layers to use | 32 to 512 | none |
| `-o`, `--output-mode` | The output mode / product to request | one of `fits`, `all_fits`, `all_files` | fits |
| `--centre` | Where to centre the image: "phase", "pointing", or "custom". If "custom", also supply --custom-centre-ra and --custom-centre-dec | one of `phase`, `pointing`, `custom` | phase |
| `--pixel-scale` | Number of arcsecs per pixel | 10 to 120 | 20 |
| `--pol` | Polarisation to image: XX, YY or XXYY | one of `XX`, `YY`, `XXYY` | XXYY |
| `--robust` | Robustness parameter- only used if `weighting=briggs` | -2 to 2 | -0.5 |
| `--uvw-max` | Maximum uv distance to image, in wavelengths (upper bound on the range that can be requested) | 1 to 5000 | none |
| `--uvw-min` | Minimum uv distance to image, in wavelengths | up to 100 | 75 |
| `--weighting` | Type of weighting to apply | one of `briggs`, `uniform`, `natural` | briggs |
| `--wstack-nwlayers` | Number of w-stacking layers. Leave unset to let the server decide | 32 to 512 | none |
| `--no-apply-amps` | Whether to skip applying amplitude calibration solutions. Leave at the default (false) unless you know you need this | flag, or `true` or `false` | false |
| `--no-digital-gains` | Do not correct the digital gains | flag, or true, false or null | false |
| `--no-flag-dc` | Do not flag the DC channel | flag, or true, false or null | false |
| `--no-geometric-delay` | Do not correct geometric delays (only applicable if not already applied by the correlator) | flag, or true, false or null | false |
| `--no-passband-gains` | Do not correct the passband gains | flag, or true, false or null | false |
| `--no-cable-delay` | Do not correct cable length delays (only applicable if not already applied by the correlator) | flag, or true, false or null | false |
| `--no-rfi` | Will disable radio frequency interference (RFI) flagging | flag, or true, false or null | false |

The options `--apply-di-cal`, `--apply-primary-beam` and `--join-channels` are true by default. To turn one off,
join the value to the option with an equals sign, for example `--join-channels=false`.

#### Imaging from a conversion job

`submit-image-from-job` makes an image from a conversion job that has already completed. It skips the conversion
step, so it takes none of the conversion options. It needs exactly one Obs ID (the Obs ID of the conversion job)
and the ID of the conversion job:

```bash
giant-squid submit-image-from-job --source-job-id 12345 1065880128 --image-size 2048 --multiscale
```

```text
Submit MWA ASVO imaging jobs from an existing conversion job. Unlike submit-image, this skips the conversion step and images directly from the output of a previous conversion job

Usage: giant-squid submit-image-from-job [OPTIONS] --source-job-id <SOURCE_JOB_ID> [OBS_ID]...

Arguments:
  [OBS_ID]...  The Obs ID to image. Exactly one Obs ID is required (the source_job_id identifies the conversion job for this Obs ID)

Options:
      --source-job-id <SOURCE_JOB_ID>
          The MWA ASVO conversion Job ID to image from. Required
  -d, --delivery <DELIVERY>
          Tell MWA ASVO where to deliver the data [env: GIANT_SQUID_DELIVERY=] [default: acacia] [possible values: acacia, scratch, dug]
  -f, --delivery-format <DELIVERY_FORMAT>
          Tell MWA ASVO to deliver the data in a particular format. Must be tar when the delivery is acacia [env: GIANT_SQUID_DELIVERY_FORMAT=] [default: tar] [possible values: tar, files]
      --apply-primary-beam[=<APPLY_PRIMARY_BEAM>]
          Whether to apply the primary beam correction [default: true] [possible values: true, false]
      --auto-mask <AUTO_MASK>
          WSClean -auto-mask value [default: 3]
      --auto-threshold <AUTO_THRESHOLD>
          WSClean -auto-threshold value [default: 0.5]
      --abs-threshold <ABS_THRESHOLD>
          Absolute cleaning threshold (Jy), or null [default: 0.001]
      --channels-out <CHANNELS_OUT>
          Number of output channel groups [default: 4]
      --clean-iterations <CLEAN_ITERATIONS>
          WSClean -niter value (max clean iterations) [default: 100000]
      --clean-threshold <CLEAN_THRESHOLD>
          WSClean cleaning threshold (Jy), or null [default: 0.001]
      --image-size <IMAGE_SIZE>
          WSClean image size in pixels [default: 3072]
      --join-channels[=<JOIN_CHANNELS>]
          Join output channel groups for cleaning [default: true] [possible values: true, false]
      --join-polarizations[=<JOIN_POLARIZATIONS>]
          Join polarisations for cleaning [default: false] [possible values: true, false]
      --mgain <MGAIN>
          WSClean -mgain value [default: 0.8]
      --multiscale[=<MULTISCALE>]
          Enable WSClean multiscale cleaning [default: false] [possible values: true, false]
      --nmiter <NMITER>
          WSClean -nmiter value (max major cleaning iterations) [default: 10]
      --nwlayers <NWLAYERS>
          Number of w-projection layers. Leave unset to let the server decide
  -o, --output-mode <OUTPUT_MODE>
          The output mode / product to request [default: fits] [possible values: fits, all_fits, all_files]
      --pixel-scale <PIXEL_SCALE>
          Pixel scale (arcsec/pixel) [default: 20]
      --pol <POL>
          Polarisation to image: XX, YY or XXYY [default: XXYY] [possible values: XX, YY, XXYY]
      --robust <ROBUST>
          WSClean -robust (Briggs robustness) value [default: -0.5]
      --uvw-max <UVW_MAX>
          Maximum uv distance to image, in wavelengths (upper bound on the range that can be requested)
      --uvw-min <UVW_MIN>
          Minimum uv distance to image, in wavelengths [default: 75]
      --weighting <WEIGHTING>
          WSClean weighting scheme [default: briggs] [possible values: briggs, uniform, natural]
      --wstack-nwlayers <WSTACK_NWLAYERS>
          Number of w-stacking layers. Leave unset to let the server decide
  -r, --allow-resubmit[=<ALLOW_RESUBMIT>]
          Allow resubmitting a job even if an identical one has completed. Takes true, false or null [default: false]
  -w, --wait
          Do not exit giant-squid until the specified Obs IDs are ready for download
  -n, --dry-run
          Don't actually submit; print information on what would've happened instead
  -j, --json
          Print each submitted job's response from the MWA ASVO as one line of JSON on stdout, and each error as an MWA ASVO ErrorResponse with the obs_id. No logs are printed
  -v, --verbosity...
          The verbosity of the program. The default is to print high-level information. Cannot be used with --json
  -h, --help
          Print help
  -V, --version
          Print version
```

It takes the imaging options of `submit-image` (`--apply-di-cal`, `--avg-freq-res`, `--avg-time-res`, `--custom-centre-dec`, `--custom-centre-ra`, `--flag-edge-width`, `--centre`, `--no-apply-amps`, `--no-digital-gains`, `--no-flag-dc`, `--no-geometric-delay`, `--no-passband-gains`, `--no-cable-delay` and `--no-rfi` excepted), and `--source-job-id`, which is required.

Some notes about imaging a conversion job:
* only conversion jobs which output a CASA measurement set are able to be imaged.
* only conversion jobs where the data was delivered to Acacia or Scratch are able to be imaged.

#### Metadata downloads

A "metadata download job" refers to a job which provides a tar containing a
metafits file and cotter flags for a single Obs ID.

```text
Submit MWA ASVO jobs to download MWA metadata — metafits (with PPDs for each tile) and RFI flags (if available)

Usage: giant-squid submit-meta [OPTIONS] [OBS_ID]...

Arguments:
  [OBS_ID]...  The Obs IDs to be submitted. Files containing Obs IDs are also accepted

Options:
  -d, --delivery <DELIVERY>
          Tell MWA ASVO where to deliver the data [env: GIANT_SQUID_DELIVERY=] [default: acacia] [possible values: acacia, scratch, dug]
  -f, --delivery-format <DELIVERY_FORMAT>
          Tell MWA ASVO to deliver the data in a particular format. Must be tar when the delivery is acacia [env: GIANT_SQUID_DELIVERY_FORMAT=] [default: tar] [possible values: tar, files]
  -r, --allow-resubmit[=<ALLOW_RESUBMIT>]
          Allow resubmitting a job even if an identical one has completed. Takes true, false or null [default: false]
  -w, --wait
          Do not exit giant-squid until the specified Obs IDs are ready for download
  -n, --dry-run
          Don't actually submit; print information on what would've happened instead
  -j, --json
          Print each submitted job's response from the MWA ASVO as one line of JSON on stdout, and each error as an MWA ASVO ErrorResponse with the obs_id. No logs are printed
  -v, --verbosity...
          The verbosity of the program. The default is to print high-level information. Cannot be used with --json
  -h, --help
          Print help
  -V, --version
          Print version
```

To submit a metadata download job for the Obs ID 1065880128:

```bash
giant-squid submit-meta 1065880128
```

Text files containing Obs IDs may be used too.

#### Visibility downloads

A "visibility download job" refers to a job which provides a tar containing
raw visibility files, a metafits file and flags for a single Obs ID. This type of job is suited to advanced users who want to do their own preprocessing.

```text
Submit MWA ASVO jobs to download MWA raw visibilities

Usage: giant-squid submit-vis [OPTIONS] [OBS_ID]...

Arguments:
  [OBS_ID]...  The Obs IDs to be submitted. Files containing Obs IDs are also accepted

Options:
  -d, --delivery <DELIVERY>
          Tell MWA ASVO where to deliver the data [env: GIANT_SQUID_DELIVERY=] [default: acacia] [possible values: acacia, scratch, dug]
  -f, --delivery-format <DELIVERY_FORMAT>
          Tell MWA ASVO to deliver the data in a particular format. Must be tar when the delivery is acacia [env: GIANT_SQUID_DELIVERY_FORMAT=] [default: tar] [possible values: tar, files]
  -r, --allow-resubmit[=<ALLOW_RESUBMIT>]
          Allow resubmitting a job even if an identical one has completed. Takes true, false or null [default: false]
  -w, --wait
          Do not exit giant-squid until the specified Obs IDs are ready for download
  -n, --dry-run
          Don't actually submit; print information on what would've happened instead
  -j, --json
          Print each submitted job's response from the MWA ASVO as one line of JSON on stdout, and each error as an MWA ASVO ErrorResponse with the obs_id. No logs are printed
  -v, --verbosity...
          The verbosity of the program. The default is to print high-level information. Cannot be used with --json
  -h, --help
          Print help
  -V, --version
          Print version
```

To submit a visibility download job for the Obs ID 1065880128:

```bash
giant-squid submit-vis 1065880128
```

Text files containing Obs IDs may be used too.

#### Beamformer downloads

A "beamformer download job" refers to a job which provides a tar containing beamformer files (generally VDIF and HDR for coherent beams and SIGPROC Filterbank for incoherent beams) for a single Obs ID.

```text
Submit MWA ASVO jobs to download MWA beamformer files (vdif,hdr,fil)

Usage: giant-squid submit-bf [OPTIONS] [OBS_ID]...

Arguments:
  [OBS_ID]...  The Obs IDs to be submitted. Files containing Obs IDs are also accepted

Options:
  -d, --delivery <DELIVERY>
          Tell MWA ASVO where to deliver the data [env: GIANT_SQUID_DELIVERY=] [default: acacia] [possible values: acacia, scratch, dug]
  -f, --delivery-format <DELIVERY_FORMAT>
          Tell MWA ASVO to deliver the data in a particular format. Must be tar when the delivery is acacia [env: GIANT_SQUID_DELIVERY_FORMAT=] [default: tar] [possible values: tar, files]
  -r, --allow-resubmit[=<ALLOW_RESUBMIT>]
          Allow resubmitting a job even if an identical one has completed. Takes true, false or null [default: false]
  -w, --wait
          Do not exit giant-squid until the specified Obs IDs are ready for download
  -n, --dry-run
          Don't actually submit; print information on what would've happened instead
  -j, --json
          Print each submitted job's response from the MWA ASVO as one line of JSON on stdout, and each error as an MWA ASVO ErrorResponse with the obs_id. No logs are printed
  -v, --verbosity...
          The verbosity of the program. The default is to print high-level information. Cannot be used with --json
  -h, --help
          Print help
  -V, --version
          Print version
```

To submit a beamformer download job for the Obs ID 1065880128:

```bash
giant-squid submit-bf 1065880128
```

Text files containing Obs IDs may be used too.

#### Voltage downloads

A "voltage download job" refers to a job which provides the raw voltages for one or more Obs IDs.

```text
Submit MWA ASVO jobs to download MWA voltages

Usage: giant-squid submit-volt [OPTIONS] --offset <OFFSET> --duration <DURATION> [OBS_ID]...

Arguments:
  [OBS_ID]...  The Obs IDs to be submitted. Files containing Obs IDs are also accepted

Options:
  -d, --delivery <DELIVERY>
          Tell MWA ASVO where to deliver the data. The only valid value for a voltage job is "scratch", which requires the "mwavcs" Pawsey Group on your MWA ASVO profile [env: GIANT_SQUID_DELIVERY=] [default: scratch]
  -o, --offset <OFFSET>
          The offset in seconds from the start GPS time of the observation
  -u, --duration <DURATION>
          The duration (in seconds) to download
      --from-channel <FROM_CHANNEL>
          The 'from' receiver channel number (0-255)
      --to-channel <TO_CHANNEL>
          The 'to' receiver channel number (0-255)
  -r, --allow-resubmit[=<ALLOW_RESUBMIT>]
          Allow resubmitting a job even if an identical one has completed. Takes true, false or null [default: false]
  -w, --wait
          Do not exit giant-squid until the specified Obs IDs are ready for download
  -n, --dry-run
          Don't actually submit; print information on what would've happened instead
  -j, --json
          Print each submitted job's response from the MWA ASVO as one line of JSON on stdout, and each error as an MWA ASVO ErrorResponse with the obs_id. No logs are printed
  -v, --verbosity...
          The verbosity of the program. The default is to print high-level information. Cannot be used with --json
  -h, --help
          Print help
  -V, --version
          Print version
```

To submit a voltage download job for the Obs ID 1065880128:

```bash
giant-squid submit-volt --delivery scratch --offset 0 --duration 8 1065880128
```

Text files containing Obs IDs may be used too.

For MWAX_VCS or MWAX_BUFFER voltage observations you can optionally pass `--from-channel` and `--to-channel` to restrict the job to
only the receiver coarse channel range specified (inclusive). MWA receiver channel numbers range from 0-255, and multiplying by 1.28
will result in the center frequency (in MHz) of that channel. Each MWA observation nominally has 24 coarse channels.

Unlike other jobs, you cannot choose to have your files tarred up and uploaded to Pawsey's Acacia for remote
download or DUG's filesystem, as the data is generally too large. If you are in the `mwaops` or `mwavcs` Pawsey groups and you have asked an MWA ASVO admin to
set the pawsey group in your MWA ASVO profile, you can request that the files be left on Pawsey's /scratch filesystem. To submit
a job with the /scratch option, set the environment variable `GIANT_SQUID_DELIVERY=scratch` or pass `--delivery scratch`.

#### Jobs from a manta-ray-client CSV file

`submit-csv` submits the jobs of a CSV file that [manta-ray-client](https://github.com/MWATelescope/manta-ray-client)
(`mwa_client -c file.csv`) reads, so you can keep your files:

```
giant-squid submit-csv jobs.csv
giant-squid submit-csv --dry-run jobs.csv   # print the request of each line, and send nothing
```

Each line of the file is one job, a list of `key=value` cells that commas separate. A blank line and a line that
starts with `#` are ignored. The spaces around a key or a value do not matter, and the line ends can be `\n` or
`\r\n`.

```csv
# A conversion job, and a download of the metadata of another observation
obs_id=1349904240, job_type=c, avg_time_res=1.0, avg_freq_res=80.0, output=uvfits, no_rfi=true
obs_id=1349904184, job_type=d, download_type=vis_meta, delivery=acacia, delivery_format=tar
```

`obs_id` and `job_type` are on every line. `job_type` is `c` (conversion), `d` (visibility or metadata download),
`b` (beamformer), `v` (voltage) or `i` (imaging from an Obs ID). The other keys, with the same names and values
as in the manta-ray-client README (and the other fields of the request body of the MWA ASVO API), are:

| `job_type` | Keys |
|---|---|
| `c` | `output`, `avg_time_res`, `avg_freq_res`, `flag_edge_width`, `apply_di_cal`, `centre`, `phase_centre_ra`, `phase_centre_dec`, `no_rfi`, `no_geometric_delay`, `no_cable_delay`, `no_digital_gains`, `no_passband_gains`, `no_flag_dc`, `no_apply_amps`, `delivery`, `delivery_format`, `allow_resubmit` |
| `d` | `download_type` (`vis` or `vis_meta`; required), `delivery`, `delivery_format`, `allow_resubmit` |
| `b` | `mode`, `delivery`, `delivery_format`, `allow_resubmit` |
| `v` | `offset` and `duration` (required), `from_channel`, `to_channel`, `channel_range`, `delivery`, `allow_resubmit` |
| `i` | `avg_time_res`, `avg_freq_res`, `flag_edge_width`, `apply_di_cal`, `centre`, `phase_centre_ra`, `phase_centre_dec`, `image_size`, `pixel_scale`, `weighting`, `robust`, `clean_iterations`, `nmiter`, `mgain`, `auto_mask`, `uvw_min`, `uvw_max`, `join_polarizations`, `apply_primary_beam`, `multiscale`, `channels_out`, `clean_threshold`, `abs_threshold`, `auto_threshold`, `nwlayers`, `wstack_nwlayers`, `join_channels`, `pol`, `output_mode`, `no_apply_amps`, the six Birli flags of the conversion job (`no_rfi` and so on), `delivery`, `delivery_format`, `allow_resubmit` |

How `submit-csv` works:

- It reads and checks every line before it submits anything, also with `--dry-run`. A mistake on any line submits
  nothing. The error lists each bad line by its line number (with `--json`, each bad line is an error line of its
  own, with its number in the key `line`). The values have the same limits as the options of the
  other submit commands.
- A key that a line does not give has the default of the MWA ASVO. Only `delivery` and `delivery_format` differ:
  if `GIANT_SQUID_DELIVERY` or `GIANT_SQUID_DELIVERY_FORMAT` is set, it is the default of the lines that do not
  give that key. A line that gives the key wins. `--allow-resubmit` sets `allow_resubmit=true` for the lines that do
  not give it.
- A submission that fails does not stop the others. Each failure is reported with its line number (with `--json`,
  in the key `line` of its error line), and the command ends with a summary and a non-zero exit code. `--wait`, `--json` and `--dry-run` work as for the other submit
  commands.

It differs from manta-ray-client where that client sends a mistake to the server without a word:

- A key that the job type does not have is an error (a typo such as `no_rf=true` cannot change a job unseen).
- A key that a line gives twice, and a line that makes the same job as an earlier line, are warnings. As in
  manta-ray-client, the last value of a key is used.
- A cell with a key and no value (`delivery=`) is ignored, as in manta-ray-client.
- A value cannot be in quotes.
- `job_type=p` (an image of a conversion job) is an error. Use `submit-image-from-job`.

#### Resubmitting jobs

By default, the MWA ASVO server will not allow you to submit a new job which is has the exact same settings/parameters as an existing job in your queue (except errored jobs). You can, however override this behaviour by specifying `--allow-resubmit` on any job submission.

### List MWA ASVO jobs

Use this command to view the state of all of your MWA ASVO jobs.

```text
List your current and recent MWA ASVO jobs

Usage: giant-squid list [OPTIONS] [JOB_ID_OR_OBS_ID]...

Arguments:
  [JOB_ID_OR_OBS_ID]...  Job IDs or Obs IDs to filter by. Files containing Job IDs or Obs IDs are also accepted

Options:
  -j, --json                    Print the jobs as a simple JSON. An error is printed as one line of JSON (an MWA ASVO ErrorResponse), and no logs are printed
      --legacy-json             Print the jobs as JSON in the old format of giant-squid before 3.0.0 (camelCase keys: obsid, jobId, jobType, jobState, fileUrl, ...). Deprecated: this option will be removed in a later release. Use --json
      --ids                     Print only the Job ID of each job, one on each line, in the order of the list, and no table. It is the input of `download`, `cancel` and `wait`, which read it on standard input when given `-`: for example `giant-squid list --job-states completed --ids | giant-squid download -`. A list with no job prints nothing
  -v, --verbosity...            The verbosity of the program. The default is to print high-level information. Cannot be used with --json or --legacy-json
      --job-states <JOB_STATE>  Show only the jobs in these states (comma separated). The case, hyphens and underscores do not matter [possible values: preparing, queued, waitcal, staging, staged, downloading, preprocessing, imaging, delivering, completed, error, cancelled]
      --job-types <JOB_TYPE>    Show only the jobs of these types (comma separated). The case, hyphens and underscores do not matter [possible values: conversion, visibility, metadata, voltage, cancel, beamformer, imaging]
      --no-colour               Disables colouring of output. Useful when you have a non-black terminal background for example
      --days <DAYS>             Only fetch jobs from the past N days (1 to 30). Cannot be used with --date-from or --date-to, which select the jobs by date instead [default: 30]
      --date-from <DATE_FROM>   Only jobs created at or after this time: RFC 3339 (for example 2026-09-01T00:00:00Z) or a date (2026-09-01, midnight UTC)
      --date-to <DATE_TO>       Only jobs created at or before this time: RFC 3339 or a date (midnight UTC)
      --sort-by <SORT_BY>       The column to sort the jobs by, for example "id"
  -h, --help                    Print help
  -V, --version                 Print version
```

Example:
```bash
giant-squid list
```

### List MWA ASVO jobs in JSON

```bash
giant-squid list --json
```

Example output:

```bash
giant-squid list --json
{"325430":{"obs_id":1090528304,"completed":"2026-09-08T06:00:00Z","created":"2026-09-08T05:41:54.757232Z","first_name":"Jane","id":325430,"job_params":{"obs_id":1090528304,"delivery":"acacia","delivery_format":"tar","download_type":"vis"},"job_state":"completed","job_type":1,"last_name":"Citizen","modified":"2026-09-08T06:00:00Z","product":{"files":[{"format":"tar","sha1":"ca0e89e56cbeb05816dad853f5bab0b4075097da","size":10762878689,"type":"acacia","url":"https://..."}]},"started":"2026-09-08T05:42:10Z","user_id":4242}}
```

The output is an object keyed by Job ID. Each job is the MWA ASVO API's `JobDetailResponse`, with one more
key, `obs_id` (which the API has only in `job_params`): `id` (the Job ID), `job_type`, `job_state`, `product`,
`created`, `started`, `completed`, `modified`, `error_code`, `error_text`, `user_id`, `first_name`, `last_name`
and `job_params`. As in the API, a key that has no value is left out (for example `completed` for a job that
is not finished). `product` is left out until the job has files; then its `files` list has, for each file, the keys of the API's
`JobFile`: `type` (where it is delivered: `acacia`, `scratch` or `dug`), `url`, `path`, `size`, `sha1`
and `format` (as the MWA ASVO gives it). As in the API, a file key that has no value is left out (for
example `path` for an Acacia file).

Before giant-squid 3.0.0 the keys were different (`obsid`, `jobId`, `jobType`, `jobState`, and
`fileUrl`, `filePath`, `fileSize`, `fileHash` for each file in a top-level `files` list; the
delivery type was also under `jobType`). See [docs/V3_MIGRATION.md](docs/V3_MIGRATION.md) for the
full list. For now, `--legacy-json` (on `list` and `wait`) prints the old format, with a warning
on stderr. It is deprecated and will be removed in a later release, so update scripts to the new
keys.

`job_type` is the MWA ASVO API's code for the job type, or `null` if the server gives none. The table
and `--job-types` use the API's name for each code:

| Code | Name |
|---|---|
| 0 | `conversion` |
| 1 | `visibility` |
| 2 | `metadata` |
| 3 | `voltage` |
| 4 | `cancel` |
| 5 | `beamformer` |
| 6 | `imaging` |

`job_state` is the MWA ASVO API's value, one of `preparing`, `queued`, `waitcal`, `staging`, `staged`,
`downloading`, `preprocessing`, `imaging`, `delivering`, `completed`, `error` or `cancelled`. A job is
ready for download when it is `completed` and its download links have not expired (see
[Expired download links](#expired-download-links)). For a job in the `error` state, the message is in
`error_text` and the server's code is in `error_code`.

Example reading this in Python:

```bash
$ giant-squid list --json > /tmp/asvo.json
$ ipython
Python 3.8.0 (default, Oct 23 2019, 18:51:26)
Type 'copyright', 'credits' or 'license' for more information
IPython 7.10.1 -- An enhanced Interactive Python. Type '?' for help.

In [1]: import json

In [2]: with open("/tmp/asvo.json", "r") as h:
   ...:     q = json.load(h)
   ...:

In [3]: q.keys()
Out[3]: dict_keys(['216087', '216241', '217628'])
```

### Filter MWA ASVO job listing

`giant-squid list` takes an optional list of identifiers that can be used to filter the job listing,
these identifiers can either be a list of Job IDs or a list of Obs IDs, but not both.

Additionally, the `--job-states` and `--job-types` options can be used to further filter the output.
(The older names `--states` and `--types` still work.) `--days` (1 to 30; the default is the MWA ASVO API's,
shown in `--help`, so `list` shows your recent jobs and not your full history), `--date-from` and `--date-to`
(a date such as `2026-09-01`, which is midnight UTC, or an RFC 3339 time such as
`2026-09-01T12:00:00Z`) limit the listing by when the jobs were created, and `--sort-by` sets the
order. The jobs are selected by `--days` or by the dates, not by both: `--days` with `--date-from` or `--date-to` is
an error, and with a date the default of `--days` is not used.

These both take a comma-separated, case-insensitive list of the job type names and job states
above. These can be provided in `TitleCase`, `UPPERCASE`, `lowercase`,
`kebab-case`, `snake_case`, or even `SPoNgeBOb-CAse`

example: show only jobs that match both of the following conditions:

- Obs ID is `1234567890` or `1234567891`
- job_type is `visibility`, `metadata` or `cancel`
- job_state is `Preprocessing` or `Queued`

```bash
giant-squid list \
   --job-types visibility,Metadata,CANCEL \
   --job-states preprocessing, Queued \
   1234567890 1234567891
```

### Example: manual hash validation with Bash and jq

This example demonstrates how it is possible to stream the output of `giant-squid list --json` into
[`jq`](https://stedolan.github.io/jq/). This is the equivalent of what `giant-squid download` does,
but with the extra overhead of storing the tar to disk (`-k`).

```bash
set -eux
giant-squid list --json --job-types visibility --job-states completed \
  | jq -r '.[]|[.id,.product.files[0].url//"",.product.files[0].size//"",.product.files[0].sha1//""]|@tsv' \
  | tee ready.tsv
while read -r jobid url size hash; do
   # note: it's a good idea to check you have enough disk space here using $size.
   wget $url -O ${jobid}.tar --progress=dot:giga --wait=60 --random-wait
   sha1=$(sha1sum ${jobid}.tar | cut -d' ' -f1)
   if [ "$sha1" != "$hash" ]; then
      echo "Download failed, hash mismatch. Expected $hash, got $sha1"
      exit 1
   fi
   tar -xf ${jobid}.tar
done < ready.tsv
```

### Wait for MWA ASVO jobs

Use this command to wait until MWA ASVO jobs are ready for download. It checks your job list once a minute and
logs a job's state when it changes. When every job is ready it prints the jobs, as `list` does (as a table, or
as JSON with `--json`), and exits.

If a check fails in a way that may pass (a network error, or a reply with an HTTP status of 429 or 5xx), `wait`
logs a warning and tries again after a short pause that grows. It gives up when such failures have lasted 15
minutes without one success. Any other failure, such as a refusal with a 4xx status, ends the wait at once.

```text
Wait for MWA ASVO jobs to complete, then print them

Usage: giant-squid wait [OPTIONS] [JOB_ID]...

Arguments:
  [JOB_ID]...  The Job IDs to wait for. Files containing Job IDs are also accepted. `-` reads standard input: the output of `giant-squid list --ids` or `giant-squid list --json` (every job in it is waited for), or Job IDs that spaces or lines separate. It can be used with other IDs

Options:
  -j, --json          Print the jobs as a simple JSON after waiting. An error is printed as one line of JSON (an MWA ASVO ErrorResponse), and no logs are printed
      --legacy-json   Print the jobs as JSON in the old format of giant-squid before 3.0.0 (camelCase keys: obsid, jobId, jobType, jobState, fileUrl, ...). Deprecated: this option will be removed in a later release. Use --json
  -v, --verbosity...  The verbosity of the program. The default is to print high-level information. Cannot be used with --json or --legacy-json
      --no-colour     Disables colouring of output. Useful when you have a non-black terminal background for example
  -h, --help          Print help
  -V, --version       Print version
```

Example:

```bash
$ giant-squid wait 31 32
13:41:02 [INFO] Waiting for 2 jobs to be ready...
13:41:04 [INFO] Job ID 31 (Obs ID: 1065880128): is queued
13:41:04 [INFO] Job ID 32 (Obs ID: 1065880248): is preprocessing
13:46:06 [INFO] Job ID 31 (Obs ID: 1065880128): is completed (ready)
13:52:08 [INFO] Job ID 32 (Obs ID: 1065880248): is completed (ready)
13:52:08 [INFO] All 2 MWA ASVO jobs are ready for download.
```

- `wait` takes Job IDs only (they can also be in files, as for the other commands). An Obs ID, alone or next to Job
  IDs, is an error that names it, and nothing is waited for. To find the Job IDs of an Obs ID, use
  `giant-squid list <OBS_ID>`.
- It stops at once, with a non-zero exit code, if a job is not in your job list, has an error or has been
  cancelled. Waiting longer would not change that.
- It waits for as long as it takes. Press Ctrl-C to stop.
- Every submit command has the same wait as the `-w`, `--wait` option.
- Log messages go to standard error. With `--json`, no log messages are printed, and an error is one line of
  JSON: see [JSON output and errors](#json-output-and-errors).

### Cancel MWA ASVO jobs

Use this command to ask the MWA ASVO to cancel jobs.

```text
Cancel MWA ASVO jobs

Usage: giant-squid cancel [OPTIONS] [JOB_ID]...

Arguments:
  [JOB_ID]...  The Job IDs to be cancelled. Files containing Job IDs are also accepted. `-` reads standard input: the output of `giant-squid list --ids` or `giant-squid list --json`. Every job in it is cancelled, so filter the list first, for example with --job-states, and check it with --dry-run. It can be used with other IDs

Options:
  -n, --dry-run       Don't actually cancel; print information on what would've happened instead
  -j, --json          Print the MWA ASVO's reply to each cancel request as one line of JSON on stdout, and each error as an MWA ASVO ErrorResponse with the job_id. No logs are printed
  -v, --verbosity...  The verbosity of the program. The default is to print high-level information. Cannot be used with --json
  -h, --help          Print help
  -V, --version       Print version
```

Example:

```bash
$ giant-squid cancel 31 32
13:20:41 [INFO] Cancel request for Job ID 31: Job cancelled
13:20:41 [INFO] Cancel request for Job ID 32: Unable to cancel job 32
13:20:41 [INFO] Cancel requests: 2 sent, 0 failed.
```

With `-j`, `--json`, `cancel` prints the MWA ASVO's reply to each request as one line of JSON on standard
output (`{"job_id":31,"message":"Job cancelled","status":"success"}`), as the submit commands do. A request
that the MWA ASVO refuses with an error prints an error line with the `job_id`: see
[JSON output and errors](#json-output-and-errors).

The log says "Cancel request" and not "Cancelled" because a reply from the MWA ASVO does not prove that the job
was cancelled:

- If a job is already cancelled, the MWA ASVO replies as normal, and only the message says that it did not cancel
  the job (job 32 above). Read the message of each job.
- If the MWA ASVO refuses a request in any other way (for example, there is no such job), `giant-squid` logs
  `Failed to cancel MWA ASVO job ID N: <reason>` and counts the request as failed. It carries on with the next
  job. At the end, the exit code is 1 if any request failed.

Like `wait`, `cancel` takes Job IDs only: an Obs ID is an error that names it, and nothing is sent.

To check what `cancel` would send, without sending it, use `--dry-run`. To see the state of the jobs after a
cancel, use [`list`](#list-mwa-asvo-jobs).

### Download MWA ASVO jobs

Once an MWA ASVO job is `completed`, its data is ready to be downloaded. If you set `--delivery=scratch` or `--delivery=dug` the data will be waiting for you on those filesystems and there is no 'downloading' to do: `download` of a DUG job fails with `FILES_NOT_REACHABLE` (see below for Scratch).

```text
Download an MWA ASVO job

Usage: giant-squid download [OPTIONS] [JOB_ID_OR_OBS_ID]...

Arguments:
  [JOB_ID_OR_OBS_ID]...  The Job IDs or Obs IDs to be downloaded. Files containing Job IDs or Obs IDs are also accepted. `-` reads standard input: the output of `giant-squid list --ids` or `giant-squid list --json` (every job in it is downloaded, so filter the list first, for example with --job-states), or IDs that spaces or lines separate. It can be used with other IDs

Options:
  -d, --download-dir <DOWNLOAD_DIR>
          Which dir should downloads be written to. Each job goes into a directory of its own below it, named for the Job ID (for example `-d /data` gives `/data/12345`), unless you use --no-job-subdir [default: .]
      --no-job-subdir
          Download the files of every job into the download directory itself, as giant-squid did before 3.0.0, and not into a directory for each job. The files of two jobs of the same observation then share a directory. This has no effect on a Scratch job, which is moved into the download directory in its own folder
  -k, --keep-tar
          Acacia delivery jobs only: Don't untar the contents of your download [alias: --keep-zip]
  -r, --no-resume
          Do not resume a partial download: download it again from the start. A complete keep-tar file that matches the MWA ASVO hash is still skipped. Without this option, a rerun after an interruption carries on where it stopped, with or without --keep-tar
  -c, --concurrent-downloads <CONCURRENT_DOWNLOADS>
          Download up to this number of jobs concurrently. 2-4 is a good number for most users. Set this to 0 to use the number of CPU cores your machine has [default: 4]
      --skip-hash
          Don't verify the downloaded contents against the upstream hash. The hash is still checked when a download is resumed (after a failed attempt or from an earlier run), and when a complete --keep-tar file is already on disk
  -n, --dry-run
          Don't actually download; print information on what would've happened instead
  -v, --verbosity...
          The verbosity of the program. The default is to print high-level information. Cannot be used with --json
  -j, --json
          Print the result of each download as one line of JSON on stdout, in the order of the Job IDs and then the Obs IDs: status, job_id, obs_id and message for a success, status "failed" and an MWA ASVO ErrorResponse with the job_id or obs_id for a failure. No logs or progress bars are printed
  -h, --help
          Print help
  -V, --version
          Print version
```

To download Job ID 12345 to your current directory '.':

```bash
giant-squid download 12345
```

To download Obs ID 1065880128 to your current directory '.' (assuming you have one `completed` job for that Obs ID):

```bash
giant-squid download 1065880128
```

(`giant-squid` differentiates between Job IDs and Obs IDs by the length of the
number specified; 10-digit numbers are treated as Obs IDs.)

Text files containing Job IDs or Obs IDs may be used too.

With `-j`, `--json`, `download` prints the result of each download as one line of JSON on standard output,
in the order of the Job IDs and then the Obs IDs, failures too:

```bash
$ giant-squid download --json 12345 1090008640
{"job_id":12345,"obs_id":1065880128,"status":"success","message":"Downloaded 1.23 GiB to ./12345"}
{"error_code":"JOB_NOT_READY","message":"No job for Obs ID 1090008640 is ready for download.","obs_id":1090008640}
```

A failed download is an error line, as described in [JSON output and errors](#json-output-and-errors), with the
`job_id` or `obs_id` that was asked for. The exit code is not zero when a download failed.

#### Expired download links

The MWA ASVO signs the download links of a job, and they work for 7 days. After that, the file server refuses
them (HTTP 403, `AccessDenied`) and the job has to be resubmitted to get new links. The MWA ASVO API has no state
for this: a job stays `completed`. `giant-squid` reads the end of the time from the link itself (the `Expires` of
its URL), so it knows without a request:

- `list` and `wait` show a completed job as `completed (ready)` when its links work and `completed (expired)`
  when they have expired. This is only how the state is shown. The JSON keeps the API's `job_state`
  (`completed`), and `--job-states completed` still matches both.
- `download` of a job whose links have expired fails at once, with no request for the file:
  `The download links of Job ID 1112133 expired on 2026-10-19T03:22:52Z (they last 7 days). Resubmit the job to
  get new ones.` In `--json` the error code is `JOB_EXPIRED`. The same error is given when the file server answers
  a download with `AccessDenied`, for example if the link has no readable `Expires`.
- `download` of an Obs ID uses its completed job whose links work. If every completed job of the Obs ID has
  expired, it reports the one that expired last.
- In the library, `AsvoJob::expires`, `is_expired` and `display_state` give the same facts, and the error is
  `AsvoError::JobExpired`. In Python they are `AsvoJob.expires`, `AsvoJob.is_expired` and
  `AsvoJob.display_state`, and `AsvoError` of kind `JobExpired`.

To choose the directory to download to, give `download` the `-d`, `--download-dir` option. Without it, the
download goes to the current directory `.`.

To download Obs ID 1065880128 to your `/tmp` directory:

```bash
giant-squid download --download-dir /tmp 1065880128
```

#### A directory for each job

`download` puts the files of each job into a directory of its own below the download directory, named for the
Job ID. The command above writes the files of the one job for Obs ID 1065880128 to `/tmp/12345`, if that is its Job
ID, and `giant-squid download --download-dir /data 12345 12346` writes to `/data/12345` and `/data/12346`. The
directory is made if it is not there. This way the files of two jobs of the same observation (for example, two
conversion jobs with different options) do not meet in one directory, where the second download would find the
files of the first.

To download into the download directory itself, as giant-squid before 3.0.0 did, use `--no-job-subdir`:

```bash
giant-squid download --no-job-subdir --download-dir /tmp 12345
```

- A Scratch job is not put in a second directory. When this host can reach it, `download` moves it into the
  download directory in the folder that the MWA ASVO made for it (`/scratch/mwaops/asvo/12345` is moved to
  `<download directory>/12345`).
- A download resumes from the files that are in the directory of its job. A download that an earlier version of
  giant-squid, or `--no-job-subdir`, left in the download directory itself is not found in `<download directory>/12345`:
  to carry on with it, use `--no-job-subdir` again.
- A symbolic link where the directory of the job should be is refused, as a link inside a download is.
- The Python module does the same by default (`job_subdirectory=True`; `False` is the layout of 2.x). A Rust program
  sets `DownloadOptions::job_subdirectory` itself.
- When you download several jobs at once (`-c`, `--concurrent-downloads`), they start in the order that you gave,
  and the log numbers them in that order (`[1/20]`, `[2/20]`, ...).

By default, `giant-squid` will perform stream untaring. Disable this with `--keep-tar`.

The MWA ASVO provides a SHA-1 of its downloads. `giant-squid` will verify the integrity
of your download by default. Give a `--skip-hash` to the `download` command to skip.
`--skip-hash` skips the check only for a download that runs from start to end in one
attempt. It does not skip the check in two cases, because then the files on disk must be
checked:

- The download is resumed, with or without `--keep-tar`: after a failed attempt in the
  same run, or from the files of an earlier run.
- With `--keep-tar`, the complete tar file is already on disk. If it matches the SHA-1,
  `giant-squid` does not download it again.

Jobs which were submitted with the /scratch data delivery option behave differently
than jobs submitted with the other data delivery options. When attempting to download
a /scratch job, if the path of the job (eg /scratch/mwaops/asvo/12345) is reachable from
the current host, it will be moved into the download directory (`-d`, `--download-dir`). Otherwise, the
download of that job fails with `FILES_NOT_REACHABLE`, and the exit code is not zero.

#### Download performance: Concurrent Downloads

By default, `giant-squid` will download 4 jobs concurrently (assuming you have specified 4 or more jobs to download).
This can help throughput if you have a good Internet connection, otherwise you may set the value manually by specifying:
`--concurrent-downloads N` where N is an integer equal or greater than 1.

#### Download performance: Changing the buffer size

By default, when downloading, `giant-squid` stores `DEFAULT_DOWNLOAD_BUFFER_SIZE`
(a library constant) of the download in memory before writing to disk. This is friendlier on disks (especially those
belonging to supercomputers!), and can make downloads faster.

The amount of data to cache before writing can be tuned by setting
`GIANT_SQUID_BUF_SIZE`. e.g.

```bash
export GIANT_SQUID_BUF_SIZE=50
giant-squid download 12345
```

would use 50 MiB of memory to cache the download before writing.

#### Resuming Interrupted Downloads

If a download fails part way through (for example, the network connection
drops), `giant-squid` retries it. The retry continues from where the failed
attempt stopped. `GIANT_SQUID_DOWNLOAD_RETRY_SECS` sets how long `giant-squid`
retries (default 900 s).

If you stop `giant-squid`, or the retries stop, run the same `download` command
again with the same `--download-dir`. The new run continues from the files that
are already on disk, in the directory of the job (see
[A directory for each job](#a-directory-for-each-job)):

- With `--keep-tar`, `giant-squid` continues a partial tar file from its end.
  If the tar file is complete and matches the MWA ASVO SHA-1, `giant-squid`
  does not download it again.
- Without `--keep-tar` (stream untar), `giant-squid` does not download again
  the files of the archive that are already complete, and continues a file
  that is partly written. While it downloads, it keeps a resume file,
  `.<tar file name>.giant-squid-resume.json`, next to the files. With
  this file, a new run does not have to read the complete files again to
  calculate the SHA-1. `giant-squid` deletes the resume file when the download
  is complete. If you change a complete file after a failed run, `giant-squid`
  does not use the resume file, and reads the files on disk again.

The SHA-1 always covers the whole archive. If a download is resumed,
`giant-squid` checks the SHA-1 even with `--skip-hash`. If the check fails,
`giant-squid` downloads the whole archive again.

Use `--no-resume` to download again from the start. A complete `--keep-tar`
file that matches the SHA-1 is still not downloaded again.

When it stream untars, `giant-squid` does not write a tar entry whose path is
absolute, contains `..` or has no name. It shows a warning for each entry that
it does not write. It also skips, with a warning, a link, a hard link or a
special file in the archive: it unpacks only files and directories.

#### Symbolic links in the download directory

`giant-squid` never writes through a symbolic link that is already in the
download directory:

- The download directory itself (`-d`, `--download-dir`) may be a symbolic
  link. Only what is below it is checked.
- If the tar file (`--keep-tar`), or a file or directory of an archive that
  `giant-squid` unpacks, is a symbolic link, or has one on the way to it
  (even one that points nowhere), the download fails with
  `SymlinkInDownloadDir`. For `--keep-tar`, nothing is fetched. For stream
  untar, the entries before it are already written. Remove the link, or use
  another download directory. On Unix, a link that another process puts in
  the place of a file or a directory after `giant-squid` has checked it is
  refused as well: the refusal is part of the calls that open them.
- When `giant-squid` moves the files of a Scratch job into the download
  directory, it moves the links in the job folder as links. It shows a warning
  for each link that points outside the job folder, because the place that it
  points to may not be there for you. A link at the place where the job folder
  would be moved to is an error, as is anything else that is there. The one
  exception is a directory there when the job's Scratch folder is gone: that
  is the result of an earlier download, so `giant-squid` reports that the files
  are already in the download directory and does not fail.

### Use the output of `list` as the input of other commands

`download`, `cancel` and `wait` read their IDs from standard input when one of their arguments is `-`, and only then.
The IDs that `list` prints are then their input. `-` can be used with other IDs (`giant-squid cancel 12345 -`).

```bash
# Download every completed job.
giant-squid list --job-states completed --ids | giant-squid download --download-dir /data -

# Cancel the queued jobs. Look at what would be cancelled first.
giant-squid list --job-states queued --ids | giant-squid cancel --dry-run -
giant-squid list --job-states queued --ids | giant-squid cancel -

# Wait for the jobs that are not ready.
giant-squid list --job-states queued,staging,preprocessing --json | giant-squid wait -
```

What the commands read:

- `list --ids`: one Job ID on each line.
- `list --json` (also `--legacy-json`, and the output of `wait --json`): the Job IDs, which are the keys. The Obs IDs in
  it are not used, because an Obs ID can have more than one job.
- IDs that spaces or lines separate, as in a file. A number that is a valid Obs ID is an Obs ID, and any other number is
  a Job ID. `cancel` and `wait` refuse an Obs ID.

The table that `list` prints by default cannot be read: the error says to use `--ids` or `--json`.

Things to know:

- **Nothing is filtered.** The commands act on every ID that they read. `download` fails for a job that is not ready,
  and `wait` fails for a job that has an error, was cancelled, or is not in the list, as they do for an ID that you
  type. Filter the list first, with `--job-states`, `--job-types`, `--days`
  and the other options of `list`. `cancel` logs `Read N Job IDs from standard input.` before it sends the
  requests, and `--dry-run` shows what it would cancel.
- **A repeated ID is used once.** An ID that you give more than once (in the arguments, in a file, or on standard
  input) is used once, at its first place.
- **An empty list is not an error.** `list --ids` prints nothing for a list with no job, and `list --json` prints
  `{}`. In both cases the next command ends with exit code 0 and says that there is nothing to do, so a script (cron,
  for example) does not fail when no job is in the state that it asks for.
- **Only a `-` reads standard input.** A command that is given no ID and no `-` fails, also when a pipe is open on its
  input (the error says to give `-`). `giant-squid` does not guess: a program that runs it (cron, CI, `ssh`, a
  wrapper) can leave a pipe open on its input, and a command that read it unasked could wait for ever, or cancel jobs
  from input that was not meant for it.
- **A failed `list`.** The one line of JSON that `list --json` prints for an error is not a job list: the next command
  refuses it and prints its message. But `list --ids` prints nothing for an error, as it does for an empty list, so
  the next command does nothing and ends with exit code 0. A shell does not stop a pipe when its first command fails,
  unless you use `set -o pipefail`. Use it in a script.
- **The job window.** `download` and `wait` look for a job in your jobs of the past 30 days, which is the default of
  `list`. A job that `list --date-from` shows from an earlier date is not found.

## Installation

### Pre-compiled

Have a look at the [GitHub releases page](https://github.com/MWATelescope/giant-squid/releases).

### Python (pip)

- Run `pip install mwa-giant-squid`

  - This installs the `giant-squid` command, which is this Rust program run inside the
    `mwa_giant_squid` Python module, and the module. See [docs/PYTHON.md](docs/PYTHON.md).

### Building from crates.io

- Install [Rust](https://www.rust-lang.org/tools/install)

- Run `cargo install mwa_giant_squid`

  - The final executable will be at `~/.cargo/bin/giant-squid`

  - This destination can be configured with the `CARGO_HOME` environment
    variable.

### Building from source

- Install [Rust](https://www.rust-lang.org/tools/install)

- Clone this repo and `cd` into it

  `git clone https://github.com/MWATelescope/giant-squid && cd giant-squid`

- Run `cargo install --path .`

  - The final executable will be at `~/.cargo/bin/giant-squid`

  - This destination can be configured with the `CARGO_HOME` environment
    variable.

---

## Docker

You can run giant-squid using docker

```bash
docker run mwatelescope/giant-squid:latest --help
```

---

## Using giant-squid as a Rust library

The `mwa_giant_squid` crate is also a Rust library. The `giant-squid`
command uses it, and your own Rust programs can use it too. The library:

- reads no environment variables. Your program gives it the host, API key,
  timeout and token cache path in an `AsvoClientConfig`.
- prints nothing. It logs through the [`log`](https://crates.io/crates/log)
  crate, and it reports download progress through an optional callback.
- returns typed errors: `AsvoApiError` for MWA ASVO API calls, and
  `AsvoError` for downloads and job checks.
- does not do poll loops. To wait for jobs, your program calls
  `AsvoJobVec::all_ready` in its own loop.
- has an `AsvoClient` that is `Send + Sync`, so threads can share one
  client. If the server rejects the session token, only one thread logs in
  again, and the other threads use the new token.
- has one path for each item: the client, jobs, errors and downloads are at
  the crate root (`mwa_giant_squid::AsvoClient`), and the types of the MWA
  ASVO OpenAPI schema are in `mwa_giant_squid::mwa_asvo::api::openapi` (for
  example `JobState`).

Add the crate without its default `bin` feature, so that the command line
dependencies are not built. The API below is from version 3.0.0.

```toml
[dependencies]
mwa_giant_squid = { version = "3", default-features = false }
```

An example:

```rust
use std::error::Error;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use mwa_giant_squid::mwa_asvo::api::openapi::DownloadJobParams;
use mwa_giant_squid::mwa_asvo::api::openapi::JobState;
use mwa_giant_squid::{
    default_token_cache_path, AsvoClient, AsvoClientConfig, DownloadOptions, DownloadProgress,
    JobsFilter, DEFAULT_ASVO_HOST, DEFAULT_DOWNLOAD_BUFFER_SIZE, DEFAULT_DOWNLOAD_RETRY_DURATION,
};

fn main() -> Result<(), Box<dyn Error>> {
    // The library reads no environment variables: the caller supplies everything.
    let mut config = AsvoClientConfig::new(DEFAULT_ASVO_HOST, "my-api-key");
    config.token_cache_path = Some(default_token_cache_path(Path::new("/home/me")));
    let client = AsvoClient::new(config)?;

    // List the ready jobs from the past 7 days. The server filters them;
    // `AsvoJobVec::filter` can then filter by several states or types.
    let ready = client.get_jobs(&JobsFilter {
        days: Some(7),
        job_state: Some(JobState::Completed),
        ..JobsFilter::default()
    })?;
    for job in &ready.0 {
        println!("{} {} {}", job.job_id(), job.obs_id(), job.job_state);
    }

    // Submit a visibility download job. Fields you do not set use the
    // MWA ASVO API's own defaults.
    let params: DownloadJobParams = DownloadJobParams::builder()
        .obs_id(1090008640)
        .try_into()?;
    let resp = client.submit_download_vis_job(&params)?;
    let job_id = resp.job_id.get();

    // Wait for it: the library checks once, the caller loops and sleeps.
    while !client.get_jobs(&JobsFilter::default())?.all_ready(&[job_id])? {
        sleep(Duration::from_secs(60));
    }

    // Download it, with an optional progress callback.
    let progress = |event: DownloadProgress| {
        if let DownloadProgress::Advanced { bytes } = event {
            eprint!("+{bytes} ");
        }
    };
    let opts = DownloadOptions {
        keep_tar: false,
        no_resume: false,
        hash: true,
        download_dir: ".",
        // The files of the job go in `./<job_id>`, as for the `giant-squid` command.
        job_subdirectory: true,
        progress: Some(&progress),
        download_number: 1,
        download_count: 1,
        buffer_size: DEFAULT_DOWNLOAD_BUFFER_SIZE,
        retry_duration: DEFAULT_DOWNLOAD_RETRY_DURATION,
        should_stop: None,
    };
    client.download_job(job_id, &opts)?;
    // To download several jobs, get the job list once with `get_jobs` and
    // call `download_job_from(&jobs, job_id, &opts)` for each.
    Ok(())
}
```

To use a server other than the production MWA ASVO (for example, to test
API features that are not live yet), give its URL as the host, for example
`AsvoClientConfig::new("https://test-asvo.mwatelescope.org", api_key)`.
With `token_cache_path = None`, the session is kept in memory only.

For a complete program that reads its settings from the environment, see
[`examples/list_jobs.rs`](examples/list_jobs.rs):

```bash
MWA_ASVO_API_KEY=<your key> cargo run --no-default-features --example list_jobs
```

The full API is on [docs.rs](https://docs.rs/mwa_giant_squid).

---

### Submitting the jobs of a manta-ray-client CSV file

`AsvoClient::submit_csv_file` reads a [manta-ray-client](https://github.com/MWATelescope/manta-ray-client) CSV file
(see [Jobs from a manta-ray-client CSV file](#jobs-from-a-manta-ray-client-csv-file)), checks every line, and submits
the jobs in the order of the file. It returns the `JobSubmittedResponse` of each job. Nothing is submitted if the file
cannot be read, has no job, or has a bad line. A job that fails does not stop the others: the error,
`CsvSubmitError::Failed`, has the replies of the jobs that were submitted and the jobs that were not.

```rust
use std::path::Path;

use mwa_giant_squid::{AsvoClient, CsvDefaults, CsvSubmitError};

fn submit(client: &AsvoClient) -> Result<(), CsvSubmitError> {
    // `CsvDefaults::from_env()` gives the defaults of the `giant-squid` command (the library itself reads no
    // environment variable). `CsvDefaults::default()` leaves the defaults of the MWA ASVO.
    let replies = client.submit_csv_file(Path::new("jobs.csv"), &CsvDefaults::default())?;
    for reply in replies {
        println!("Submitted Job ID {}", reply.job_id);
    }
    Ok(())
}
```

`read_manta_ray_csv` reads the jobs of a file without a client (it refuses a file with no job, as `submit_csv_file`
does), and `parse_manta_ray_csv` reads the text of a file. `AsvoClient::submit_csv_jobs` submits the jobs that they
return, with a function that is called for each job when its request is done. Its error, `CsvJobsFailed`, has the
replies of the jobs that were submitted and the jobs that were not.

## Environment Variables

These variables are read by the `giant-squid` command. The Rust library
reads no environment variables (see
[Using giant-squid as a Rust library](#using-giant-squid-as-a-rust-library)).

| Variable | Description | Default |
|---|---|---|
| `MWA_ASVO_API_KEY` | Your MWA ASVO API key. **Required** for all operations. | — |
| `MWA_ASVO_HOST` | The MWA ASVO server, for example a test server. | `DEFAULT_ASVO_HOST` in the library (the production MWA ASVO) |
| `MWA_ASVO_API_TIMEOUT` | The timeout of one API request, in whole seconds (at least 1). | `DEFAULT_API_TIMEOUT` in the library |
| `HOME` | The session cache is `$HOME/.mwa-asvo/tokens.json`, shared with mwa-cli. Without `HOME`, the session is not cached. | — |
| `GIANT_SQUID_DELIVERY` | The default of `-d`/`--delivery` on the submit commands, and the `delivery` of the lines of a `submit-csv` file that have none. | The schema's default for each job type |
| `GIANT_SQUID_DELIVERY_FORMAT` | The default of `-f`/`--delivery-format` on the submit commands, and the `delivery_format` of the lines of a `submit-csv` file that have none. | The schema's default for each job type |
| `GIANT_SQUID_BUF_SIZE` | Download buffer size in whole MiB (at least 1): the amount of data held in memory before it is written to disk. | `DEFAULT_DOWNLOAD_BUFFER_SIZE` in the library |
| `GIANT_SQUID_DOWNLOAD_RETRY_SECS` | How long a download retries transient failures, in whole seconds. `0` disables retrying. | `DEFAULT_DOWNLOAD_RETRY_DURATION` in the library |
| `CARGO_HOME` | Controls where `cargo install` places the `giant-squid` binary. | `~/.cargo` |

A value of `MWA_ASVO_API_TIMEOUT`, `GIANT_SQUID_BUF_SIZE` or `GIANT_SQUID_DOWNLOAD_RETRY_SECS` that is not a whole
number is an error. So is a `GIANT_SQUID_BUF_SIZE` or `MWA_ASVO_API_TIMEOUT` of `0`.

---

## Background

`giant-squid` was originally written in Haskell by chjordan and is still available on
[GitLab](https://gitlab.com/chjordan/giant-squid). It was later rewritten in Rust for
improved performance and maintainability. The Rust version is now the actively maintained
implementation and is what this repository contains.
