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

2. If `--keep-tar` is specified, then giant-squid will support resuming partial
   downloads and continue where it left off if the download command is run after
   a download was interrupted or failed. In addition, if the file to download
   already exists and matches the expected file size and checksum, then
   giant-squid will skip downloading the file again.

3. `giant-squid` does not require a CSV file to submit jobs; this is instead
   handled by command line arguments.

4. For any commands that accept obsids or job IDs, it is possible use text files
   instead. These files are unpacked as if you had typed them out manually, and
   each entry of the text file(s) are checked for validity (all ints and all
   10-digits long); any exceptions are reported and the command fails.

5. One can ask `giant-squid` to print your MWA ASVO queue as JSON; this makes
   parsing the state of your jobs in another programming language much simpler.

6. By default, `giant-squid` will validate the hash of the archive. You can skip
   this check with `--skip-hash`

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
    - [Resubmitting jobs](#resubmitting-jobs)
  - [List MWA ASVO jobs](#list-mwa-asvo-jobs)
  - [List MWA ASVO jobs in JSON](#list-mwa-asvo-jobs-in-json)
  - [Filter MWA ASVO job listing](#filter-mwa-asvo-job-listing)
  - [Example: manual hash validation with Bash and jq](#example-manual-hash-validation-with-bash-and-jq)
  - [Wait for MWA ASVO jobs](#wait-for-mwa-asvo-jobs)
  - [Cancel MWA ASVO jobs](#cancel-mwa-asvo-jobs)
  - [Download MWA ASVO jobs](#download-mwa-asvo-jobs)
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
tokens) with the MWA ASVO. This session is cached at `$HOME/.mwa-asvo/tokens.json` (permissions `0600`)
and reused across commands until it expires, so you won't see a fresh login on every invocation. This
cache is shared with `manta-ray-client` (if you use it too), so logging in with either client makes a 
valid session available to the other. You can safely delete this file at any time - `giant-squid` will 
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

Every submit command takes one or more obsids, or text files that hold obsids. It submits one job for
each obsid and carries on if one fails. It ends with a summary and a non-zero exit code if any failed.

#### Options that every submit command has

| Option | Meaning |
|---|---|
| `-d`, `--delivery` | Where MWA ASVO delivers the data: see [A Note On Delivery Options](#a-note-on-delivery-options). `submit-volt` takes `scratch` only |
| `-f`, `--delivery-format` | `tar` (the default) or `files`. `submit-volt` does not have it, because voltage data is always delivered as files |
| `-r`, `--allow-resubmit` | Submit the job even if an identical one has completed: see [Resubmitting jobs](#resubmitting-jobs) |
| `-w`, `--wait` | Do not exit until the jobs are ready for download |
| `-n`, `--dry-run` | Do not submit. Print the request that would be sent for each obsid |
| `-j`, `--json` | Print the MWA ASVO's reply for each submitted job as one line of JSON on standard output |
| `-v`, `--verbosity` | Show more log messages. Repeat it for more |

Log messages go to standard error, so standard output has only the output of the command (for example, the
`--json` lines). To check that your command works without submitting anything, use `--dry-run`:

```bash
$ giant-squid submit-vis 1065880128 --dry-run
13:10:06 [INFO] [dry run] Would POST /api/v2/download_vis_job for obsid 1065880128:
{
  "allow_resubmit": false,
  "delivery": "acacia",
  "delivery_format": "tar",
  "download_type": "vis",
  "obs_id": 1065880128
}
13:10:06 [INFO] [dry run] Would have submitted 1 obsids to /api/v2/download_vis_job. Nothing was sent.
```

The defaults of the job options are those of the MWA ASVO API. Run `giant-squid <command> --help` to see them.

#### A Note On Delivery Options

Before submitting any MWA ASVO job, you will need to decide _where_ you want the data to be delivered. There are up to three options depending on your user profile.

##### Delivery: Acacia (Default)

- The default option for all job types except voltage downloads (`Voltage` jobs are not able to be delivered to Acacia due to their size).
- Files are tarred up and uploaded to Pawsey's Acacia object store.
- To submit a job with the Acacia delivery option specify `--delivery=acacia` on any job submission command.
- A URL which expires in 7 days is generated- allowing you to download the file via giant-squid, wget, curl, etc from anywhere in the world.

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
- Only visibility, metadata and beamformer downloads can be delivered to DUG. `Voltage`, conversion and imaging jobs cannot.
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
  [OBS_ID]...  The obsids to be submitted. Files containing obsids are also accepted

Options:
  -d, --delivery <DELIVERY>
          Tell MWA ASVO where to deliver the data [env: GIANT_SQUID_DELIVERY=] [default: acacia] [possible values: acacia, dug, scratch]
  -f, --delivery-format <DELIVERY_FORMAT>
          Tell MWA ASVO to deliver the data in a particular format [env: GIANT_SQUID_DELIVERY_FORMAT=] [default: tar] [possible values: files, tar]
  -o, --output <OUTPUT>
          Output format: "ms" (measurement set) or "uvfits" [default: ms] [possible values: ms, uvfits]
      --avg-freq-res <AVG_FREQ_RES>
          Frequency resolution to average to (kHz) [default: 40]
      --avg-time-res <AVG_TIME_RES>
          Time resolution to average to (s) [default: 2]
      --flag-edge-width <FLAG_EDGE_WIDTH>
          Width of frequency edge flagging (kHz) [default: 80]
      --apply-di-cal
          Whether to apply the DI calibration solution
      --centre <CENTRE>
          Phase centre mode: "phase", "pointing", or "custom". If "custom", also supply --custom-centre-ra and --custom-centre-dec [default: phase] [possible values: custom, phase, pointing]
      --custom-centre-ra <CUSTOM_CENTRE_RA>
          Custom phase centre right ascension (degrees). Requires --centre custom
      --custom-centre-dec <CUSTOM_CENTRE_DEC>
          Custom phase centre declination (degrees). Requires --centre custom
      --no-apply-amps
          Whether to skip applying amplitude calibration solutions
      --no-digital-gains
          Whether to skip applying digital gains
      --no-flag-dc
          Whether to skip flagging the DC channel
      --no-geometry-delay
          Whether to skip applying geometric delay corrections
      --no-passband-gains
          Whether to skip applying passband gain corrections
      --no-cable-delay
          Whether to skip applying cable delay corrections
      --no-rfi
          Whether to skip RFI flagging
  -r, --allow-resubmit
          Allow resubmitting a job even if an identical one has completed
  -w, --wait
          Do not exit giant-squid until the specified obsids are ready for download
  -n, --dry-run
          Don't actually submit; print information on what would've happened instead
  -j, --json
          Print each submitted job's response from the MWA ASVO as one line of JSON on stdout
  -v, --verbosity...
          The verbosity of the program. The default is to print high-level information
  -h, --help
          Print help
```

To submit a conversion job for obsid 1065880128:

```bash
giant-squid submit-conv 1065880128
```

Text files containing obsids may be used too.

To change the conversion options, give them as options. For example, to average to 0.5 s and 10 kHz and write a
UVFITS file:

```bash
giant-squid submit-conv 1065880128 --output uvfits --avg-time-res 0.5 --avg-freq-res 10
```

To use a custom phase centre:

```bash
giant-squid submit-conv 1065880128 --centre custom --custom-centre-ra 12.5 --custom-centre-dec -26.7
```

Conversion jobs can be delivered to Acacia or Scratch (not DUG).

##### Options for conversion jobs

In addition to the [options that every submit command has](#options-that-every-submit-command-has):

| Option | Meaning | Values | Default |
|---|---|---|---|
| `-o`, `--output` | Output data format (CASA measurement set or UVFITS) | one of `ms`, `uvfits` | ms |
| `--avg-freq-res` | Output frequency resolution in kHz. Must be a multiple of, or equal to, the correlator frequency resolution | 0 to 1280 | 40 |
| `--avg-time-res` | Output time resolution in seconds. Must be a multiple of, or equal to, the correlator time resolution | 0 or more | 2 |
| `--flag-edge-width` | Width in kHz to flag at each coarse channel edge. Must be a multiple of, or equal to, the correlator frequency resolution | 0 to 640 | 80 |
| `--apply-di-cal` | Apply the basic direction-independent calibration solution (if available) | flag | off |
| `--centre` | Phase centre to use | one of `phase`, `pointing`, `custom` | phase |
| `--custom-centre-ra` | Right ascension in decimal degrees of the custom phase centre. Needs `--centre custom` | 0 to 359.999999 | none |
| `--custom-centre-dec` | Declination in decimal degrees of the custom phase centre. Needs `--centre custom` | -90 to 90 | none |
| `--no-apply-amps` | Whether to skip applying amplitude calibration solutions | flag | off |
| `--no-digital-gains` | Do not correct the digital gains | flag | off |
| `--no-flag-dc` | Do not flag the DC channel | flag | off |
| `--no-geometry-delay` | Do not correct geometric delays (only applicable if not already applied by the correlator) | flag | off |
| `--no-passband-gains` | Do not correct the passband gains | flag | off |
| `--no-cable-delay` | Do not correct cable length delays (only applicable if not already applied by the correlator) | flag | off |
| `--no-rfi` | Will disable radio frequency interference (RFI) flagging | flag | off |

#### Imaging downloads

An "imaging download job" takes the raw visibilities of an obsid and produces an image. The raw visibilities are converted to a CASA measurement set first, just like a regular [Conversion](#conversion-downloads) job. To image an existing, completed conversion job instead, see [Imaging from a conversion job](#imaging-from-a-conversion-job).

The MWA ASVO imaging features uses the WSClean software by André Offringa to generated images from CASA measurement sets. For comprehensive documentation about WSClean, please see: [WSClean readthedocs](https://wsclean.readthedocs.io/).

```text
Submit MWA ASVO imaging jobs

Usage: giant-squid submit-image [OPTIONS] [OBS_ID]...

Arguments:
  [OBS_ID]...  The obsids to submit for imaging. Files containing obsids are also accepted. All obsids in one invocation share the same parameters above

Options:
  -d, --delivery <DELIVERY>
          Tell MWA ASVO where to deliver the data [env: GIANT_SQUID_DELIVERY=] [default: acacia] [possible values: acacia, dug, scratch]
  -f, --delivery-format <DELIVERY_FORMAT>
          Tell MWA ASVO to deliver the data in a particular format [env: GIANT_SQUID_DELIVERY_FORMAT=] [default: tar] [possible values: files, tar]
      --apply-di-cal[=<APPLY_DI_CAL>]
          Whether to apply the DI calibration solution [default: true] [possible values: true, false]
      --apply-primary-beam[=<APPLY_PRIMARY_BEAM>]
          Whether to apply the primary beam correction [default: true] [possible values: true, false]
      --auto-mask <AUTO_MASK>
          WSClean -auto-mask value [default: 3]
      --auto-threshold <AUTO_THRESHOLD>
          WSClean -auto-threshold value [default: 0.5]
      --abs-threshold <ABS_THRESHOLD>
          Absolute cleaning threshold (Jy). Overridden by auto_threshold unless explicitly set [default: 0.001]
      --avg-freq-res <AVG_FREQ_RES>
          Frequency resolution to average to before imaging (kHz) [default: 40]
      --avg-time-res <AVG_TIME_RES>
          Time resolution to average to before imaging (s) [default: 2]
      --channels-out <CHANNELS_OUT>
          Number of output channel groups [default: 4]
      --clean-iterations <CLEAN_ITERATIONS>
          WSClean -niter value (max clean iterations) [default: 100000]
      --clean-threshold <CLEAN_THRESHOLD>
          WSClean cleaning threshold (Jy). Takes precedence over auto_threshold if set [default: 0.001]
      --custom-centre-dec <CUSTOM_CENTRE_DEC>
          Custom phase centre declination (degrees). Requires --centre custom
      --custom-centre-ra <CUSTOM_CENTRE_RA>
          Custom phase centre right ascension (degrees). Requires --centre custom
      --flag-edge-width <FLAG_EDGE_WIDTH>
          Width of frequency edge flagging (kHz) [default: 80]
      --image-size <IMAGE_SIZE>
          WSClean image size in pixels [default: 3072]
      --join-channels[=<JOIN_CHANNELS>]
          Join output channel groups for cleaning [default: true] [possible values: true, false]
      --join-polarizations
          Join polarisations for cleaning
      --mgain <MGAIN>
          WSClean -mgain value [default: 0.8]
      --multiscale
          Enable WSClean multiscale cleaning
      --nmiter <NMITER>
          WSClean -nmiter value (max major cleaning iterations) [default: 10]
      --nwlayers <NWLAYERS>
          Number of w-projection layers. Leave unset to let the server decide
  -o, --output-mode <OUTPUT_MODE>
          The output mode / product to request [default: fits] [possible values: all_files, all_fits, fits]
      --centre <CENTRE>
          Where to centre the image: "phase", "pointing", or "custom". If "custom", also supply --custom-centre-ra and --custom-centre-dec [default: phase] [possible values: custom, phase, pointing]
      --pixel-scale <PIXEL_SCALE>
          Pixel scale (arcsec/pixel) [default: 20]
      --pol <POL>
          Polarisation to image: XX, YY or XXYY [default: XXYY] [possible values: XX, XXYY, YY]
      --robust <ROBUST>
          WSClean -robust (Briggs robustness) value [default: -0.5]
      --uvw-max <UVW_MAX>
          Maximum uv distance to image, in wavelengths (upper bound on the range that can be requested)
      --uvw-min <UVW_MIN>
          Minimum uv distance to image, in wavelengths [default: 75]
      --weighting <WEIGHTING>
          WSClean weighting scheme [default: briggs] [possible values: briggs, natural, uniform]
      --wstack-nwlayers <WSTACK_NWLAYERS>
          Number of w-stacking layers. Leave unset to let the server decide
      --no-apply-amps
          Whether to skip applying amplitude calibration solutions. Leave at the default (false) unless you know you need this
  -r, --allow-resubmit
          Allow resubmitting a job even if an identical one has completed
  -w, --wait
          Do not exit giant-squid until the specified obsids are ready for download
  -n, --dry-run
          Don't actually submit; print information on what would've happened instead
  -j, --json
          Print each submitted job's response from the MWA ASVO as one line of JSON on stdout
  -v, --verbosity...
          The verbosity of the program. The default is to print high-level information
  -h, --help
          Print help
```

To submit an imaging job for the obsid 1065880128, give any conversion options as well as imaging options:

```bash
giant-squid submit-image 1065880128 --avg-time-res 0.5 --avg-freq-res 10 --image-size 2048 --multiscale
```

Imaging jobs can be delivered to Acacia or Scratch (not DUG).

##### Options for imaging jobs

In addition to the [options that every submit command has](#options-that-every-submit-command-has):

| Option | Meaning | Values | Default |
|---|---|---|---|
| `--apply-di-cal` | Apply the basic direction-independent calibration solution (if available) | `true` or `false`, given as `--apply-di-cal=false` | true |
| `--apply-primary-beam` | Calculate and apply the primary beam and save images for the Jones components, with weighting identical to the weighting as used by the imager | `true` or `false`, given as `--apply-primary-beam=false` | true |
| `--auto-mask` | WSClean -auto-mask value | 2 to 512 | 3 |
| `--auto-threshold` | Relative clean threshold. Estimate noise level using a robust estimator and stop at sigma x stddev | 0.1 to 5 | 0.5 |
| `--abs-threshold` | Absolute cleaning threshold (Jy). Overridden by auto_threshold unless explicitly set | 0 to 10 | 0.001 |
| `--avg-freq-res` | Output frequency resolution in kHz. Must be a multiple of, or equal to, the correlator frequency resolution | 0 to 1280 | 40 |
| `--avg-time-res` | Output time resolution in seconds. Must be a multiple of, or equal to, the correlator time resolution | 0 or more | 2 |
| `--channels-out` | Number of output channel groups | any whole number | 4 |
| `--clean-iterations` | Maximum number of clean iterations to perform | 0 to 1000000 | 100000 |
| `--clean-threshold` | Absolute stopping clean thresholding in Jy | 0 to 10 | 0.001 |
| `--custom-centre-dec` | Declination in decimal degrees of the custom phase centre. Needs `--centre custom` | -90 to 90 | none |
| `--custom-centre-ra` | Right ascension in decimal degrees of the custom phase centre. Needs `--centre custom` | 0 to 359.999999 | none |
| `--flag-edge-width` | Width in kHz to flag at each coarse channel edge. Must be a multiple of, or equal to, the correlator frequency resolution | 0 to 640 | 80 |
| `--image-size` | width and height in pixels of output image | one of `512`, `1024`, `2048`, `3072`, `4096`, `8192` | 3072 |
| `--join-channels` | Join output channel groups for cleaning | `true` or `false`, given as `--join-channels=false` | true |
| `--join-polarizations` | Join polarisations for cleaning | flag | off |
| `--mgain` | WSClean -mgain value | 0.1 to 1 | 0.8 |
| `--multiscale` | Clean on different scales. This is a new algorithm. This parameter invokes the optimized multiscale algorithm published by Offringa & Smirnov (2017) | flag | off |
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
| `--no-apply-amps` | Whether to skip applying amplitude calibration solutions. Leave at the default (false) unless you know you need this | flag | off |

The options `--apply-di-cal`, `--apply-primary-beam` and `--join-channels` are true by default. To turn one off,
join the value to the option with an equals sign, for example `--join-channels=false`.

#### Imaging from a conversion job

`submit-image-from-job` makes an image from a conversion job that has already completed. It skips the conversion
step, so it takes none of the conversion options. It needs exactly one obsid (the obsid of the conversion job)
and the ID of the conversion job:

```bash
giant-squid submit-image-from-job --source-job-id 12345 1065880128 --image-size 2048 --multiscale
```

```text
Submit MWA ASVO imaging jobs from an existing conversion job. Unlike submit-image, this skips the conversion step and images directly from the output of a previous conversion job

Usage: giant-squid submit-image-from-job [OPTIONS] --source-job-id <SOURCE_JOB_ID> [OBS_ID]...

Arguments:
  [OBS_ID]...  The obsid to image. Exactly one obsid is required (the source_job_id identifies the conversion job for this obsid)

Options:
      --source-job-id <SOURCE_JOB_ID>
          The MWA ASVO conversion job ID to image from. Required
  -d, --delivery <DELIVERY>
          Tell MWA ASVO where to deliver the data [env: GIANT_SQUID_DELIVERY=] [default: acacia] [possible values: acacia, dug, scratch]
  -f, --delivery-format <DELIVERY_FORMAT>
          Tell MWA ASVO to deliver the data in a particular format [env: GIANT_SQUID_DELIVERY_FORMAT=] [default: tar] [possible values: files, tar]
      --apply-primary-beam[=<APPLY_PRIMARY_BEAM>]
          Whether to apply the primary beam correction [default: true] [possible values: true, false]
      --auto-mask <AUTO_MASK>
          WSClean -auto-mask value [default: 3]
      --auto-threshold <AUTO_THRESHOLD>
          WSClean -auto-threshold value [default: 0.5]
      --abs-threshold <ABS_THRESHOLD>
          Absolute cleaning threshold (Jy). Overridden by auto_threshold unless explicitly set [default: 0.001]
      --channels-out <CHANNELS_OUT>
          Number of output channel groups [default: 4]
      --clean-iterations <CLEAN_ITERATIONS>
          WSClean -niter value (max clean iterations) [default: 100000]
      --clean-threshold <CLEAN_THRESHOLD>
          WSClean cleaning threshold (Jy). Takes precedence over auto_threshold if set [default: 0.001]
      --image-size <IMAGE_SIZE>
          WSClean image size in pixels [default: 3072]
      --join-channels[=<JOIN_CHANNELS>]
          Join output channel groups for cleaning [default: true] [possible values: true, false]
      --join-polarizations
          Join polarisations for cleaning
      --mgain <MGAIN>
          WSClean -mgain value [default: 0.8]
      --multiscale
          Enable WSClean multiscale cleaning
      --nmiter <NMITER>
          WSClean -nmiter value (max major cleaning iterations) [default: 10]
      --nwlayers <NWLAYERS>
          Number of w-projection layers. Leave unset to let the server decide
  -o, --output-mode <OUTPUT_MODE>
          The output mode / product to request [default: fits] [possible values: all_files, all_fits, fits]
      --pixel-scale <PIXEL_SCALE>
          Pixel scale (arcsec/pixel) [default: 20]
      --pol <POL>
          Polarisation to image: XX, YY or XXYY [default: XXYY] [possible values: XX, XXYY, YY]
      --robust <ROBUST>
          WSClean -robust (Briggs robustness) value [default: -0.5]
      --uvw-max <UVW_MAX>
          Maximum uv distance to image, in wavelengths (upper bound on the range that can be requested)
      --uvw-min <UVW_MIN>
          Minimum uv distance to image, in wavelengths [default: 75]
      --weighting <WEIGHTING>
          WSClean weighting scheme [default: briggs] [possible values: briggs, natural, uniform]
      --wstack-nwlayers <WSTACK_NWLAYERS>
          Number of w-stacking layers. Leave unset to let the server decide
  -r, --allow-resubmit
          Allow resubmitting a job even if an identical one has completed
  -w, --wait
          Do not exit giant-squid until the specified obsids are ready for download
  -n, --dry-run
          Don't actually submit; print information on what would've happened instead
  -j, --json
          Print each submitted job's response from the MWA ASVO as one line of JSON on stdout
  -v, --verbosity...
          The verbosity of the program. The default is to print high-level information
  -h, --help
          Print help
```

It takes the imaging options of `submit-image` (`--apply-di-cal`, `--avg-freq-res`, `--avg-time-res`, `--custom-centre-dec`, `--custom-centre-ra`, `--flag-edge-width`, `--centre`, `--no-apply-amps` excepted), and `--source-job-id`, which is required.

Some notes about imaging a conversion job:
* only conversion jobs which output a CASA measurement set are able to be imaged.
* only conversion jobs where the data was delivered to Acacia or Scratch are able to be imaged.

#### Metadata downloads

A "metadata download job" refers to a job which provides a tar containing a
metafits file and cotter flags for a single obsid.

```text
Submit MWA ASVO jobs to download MWA metadata — metafits (with PPDs for each tile) and RFI flags (if available)

Usage: giant-squid submit-meta [OPTIONS] [OBS_ID]...

Arguments:
  [OBS_ID]...  The obsids to be submitted. Files containing obsids are also accepted

Options:
  -d, --delivery <DELIVERY>
          Tell MWA ASVO where to deliver the data [env: GIANT_SQUID_DELIVERY=] [default: acacia] [possible values: acacia, dug, scratch]
  -f, --delivery-format <DELIVERY_FORMAT>
          Tell MWA ASVO to deliver the data in a particular format [env: GIANT_SQUID_DELIVERY_FORMAT=] [default: tar] [possible values: files, tar]
  -r, --allow-resubmit
          Allow resubmitting a job even if an identical one has completed
  -w, --wait
          Do not exit giant-squid until the specified obsids are ready for download
  -n, --dry-run
          Don't actually submit; print information on what would've happened instead
  -j, --json
          Print each submitted job's response from the MWA ASVO as one line of JSON on stdout
  -v, --verbosity...
          The verbosity of the program. The default is to print high-level information
  -h, --help
          Print help
```

To submit a metadata download job for the obsid 1065880128:

```bash
giant-squid submit-meta 1065880128
```

Text files containing obsids may be used too.

#### Visibility downloads

A "visibility download job" refers to a job which provides a tar containing
raw visibility files, a metafits file and flags for a single obsid. This type of job is suited to advanced users who want to do their own preprocessing.

```text
Submit MWA ASVO jobs to download MWA raw visibilities

Usage: giant-squid submit-vis [OPTIONS] [OBS_ID]...

Arguments:
  [OBS_ID]...  The obsids to be submitted. Files containing obsids are also accepted

Options:
  -d, --delivery <DELIVERY>
          Tell MWA ASVO where to deliver the data [env: GIANT_SQUID_DELIVERY=] [default: acacia] [possible values: acacia, dug, scratch]
  -f, --delivery-format <DELIVERY_FORMAT>
          Tell MWA ASVO to deliver the data in a particular format [env: GIANT_SQUID_DELIVERY_FORMAT=] [default: tar] [possible values: files, tar]
  -r, --allow-resubmit
          Allow resubmitting a job even if an identical one has completed
  -w, --wait
          Do not exit giant-squid until the specified obsids are ready for download
  -n, --dry-run
          Don't actually submit; print information on what would've happened instead
  -j, --json
          Print each submitted job's response from the MWA ASVO as one line of JSON on stdout
  -v, --verbosity...
          The verbosity of the program. The default is to print high-level information
  -h, --help
          Print help
```

To submit a visibility download job for the obsid 1065880128:

```bash
giant-squid submit-vis 1065880128
```

Text files containing obsids may be used too.

#### Beamformer downloads

A "beamformer download job" refers to a job which provides a tar containing beamformer files (generally VDIF and HDR for coherent beams and SIGPROC Filterbank for incoherent beams) for a single obsid.

```text
Submit MWA ASVO jobs to download MWA beamformer files (vdif,hdr,fil)

Usage: giant-squid submit-bf [OPTIONS] [OBS_ID]...

Arguments:
  [OBS_ID]...  The obsids to be submitted. Files containing obsids are also accepted

Options:
  -d, --delivery <DELIVERY>
          Tell MWA ASVO where to deliver the data [env: GIANT_SQUID_DELIVERY=] [default: acacia] [possible values: acacia, dug, scratch]
  -f, --delivery-format <DELIVERY_FORMAT>
          Tell MWA ASVO to deliver the data in a particular format [env: GIANT_SQUID_DELIVERY_FORMAT=] [default: tar] [possible values: files, tar]
  -r, --allow-resubmit
          Allow resubmitting a job even if an identical one has completed
  -w, --wait
          Do not exit giant-squid until the specified obsids are ready for download
  -n, --dry-run
          Don't actually submit; print information on what would've happened instead
  -j, --json
          Print each submitted job's response from the MWA ASVO as one line of JSON on stdout
  -v, --verbosity...
          The verbosity of the program. The default is to print high-level information
  -h, --help
          Print help
```

To submit a beamformer download job for the obsid 1065880128:

```bash
giant-squid submit-bf 1065880128
```

Text files containing obsids may be used too.

#### Voltage downloads

A "voltage download job" refers to a job which provides the raw voltages for one or more obsids.

```text
Submit MWA ASVO jobs to download MWA voltages

Usage: giant-squid submit-volt [OPTIONS] --offset <OFFSET> --duration <DURATION> [OBS_ID]...

Arguments:
  [OBS_ID]...  The obsids to be submitted. Files containing obsids are also accepted

Options:
  -d, --delivery <DELIVERY>          Tell MWA ASVO where to deliver the data. The only valid value for a voltage job is "scratch", which requires the "mwavcs" Pawsey Group on your MWA ASVO profile [env: GIANT_SQUID_DELIVERY=] [default: scratch]
  -o, --offset <OFFSET>              The offset in seconds from the start GPS time of the observation
  -u, --duration <DURATION>          The duration (in seconds) to download
  -f, --from-channel <FROM_CHANNEL>  The 'from' receiver channel number (0-255)
  -t, --to-channel <TO_CHANNEL>      The 'to' receiver channel number (0-255)
  -r, --allow-resubmit               Allow resubmitting a job even if an identical one has completed
  -w, --wait                         Do not exit giant-squid until the specified obsids are ready for download
  -n, --dry-run                      Don't actually submit; print information on what would've happened instead
  -j, --json                         Print each submitted job's response from the MWA ASVO as one line of JSON on stdout
  -v, --verbosity...                 The verbosity of the program. The default is to print high-level information
  -h, --help                         Print help
```

To submit a voltage download job for the obsid 1065880128:

```bash
giant-squid submit-volt --delivery scratch --offset 0 --duration 8 1065880128
```

Text files containing obsids may be used too.

For MWAX_VCS or MWAX_BUFFER voltage observations you can optionally pass `--from-channel` (`-f`) and `--to-channel` (`-t`) to restrict the job to
only the receiver coarse channel range specified (inclusive). MWA receiver channel numbers range from 0-255, and multiplying by 1.28
will result in the center frequency (in MHz) of that channel. Each MWA observation nominally has 24 coarse channels.

Unlike other jobs, you cannot choose to have your files tarred up and uploaded to Pawsey's Acacia for remote
download or DUG's filesystem, as the data is generally too large. If you are in the `mwaops` or `mwavcs` Pawsey groups and you have asked an MWA ASVO admin to
set the pawsey group in your MWA ASVO profile, you can request that the files be left on Pawsey's /scratch filesystem. To submit
a job with the /scratch option, set the environment variable `GIANT_SQUID_DELIVERY=scratch` or pass `--delivery scratch`.

#### Resubmitting jobs

By default, the MWA ASVO server will not allow you to submit a new job which is has the exact same settings/parameters as an existing job in your queue (except errored jobs). You can, however override this behaviour by specifying `--allow-resubmit` on any job submission.

### List MWA ASVO jobs

Use this command to view the state of all of your MWA ASVO jobs.

```text
List your current and recent MWA ASVO jobs

Usage: giant-squid list [OPTIONS] [JOB_ID_OR_OBS_ID]...

Arguments:
  [JOB_ID_OR_OBS_ID]...  job IDs or obsids to filter by. Files containing job IDs or obsids are also accepted

Options:
  -j, --json                    Print the jobs as a simple JSON
      --legacy-json             Print the jobs as JSON in the old format of giant-squid before 3.0.0 (camelCase keys: obsid, jobId, jobType, jobState, fileUrl, ...). Deprecated: this option will be removed in the release after 3.0.0. Use --json
  -v, --verbosity...            The verbosity of the program. The default is to print high-level information
      --job-states <JOB_STATE>  show only jobs matching the provided states, case insensitive. Options: queued, waitcal, staging, staged, downloading, preparing, preprocessing, imaging, delivering, ready, error, expired, cancelled
      --job-types <JOB_TYPE>    filter job list by type, case insensitive with underscores. Options: conversion, download_visibilities, download_metadata, download_voltages, download_beamformer, imaging or cancel_job
  -n, --no-colour               Disables colouring of output. Useful when you have a non-black terminal background for example
      --days <DAYS>             Only fetch jobs from the past N days (1 to 30) [default: 30]
      --date-from <DATE_FROM>   Only jobs created at or after this time: RFC 3339 (for example 2026-09-01T00:00:00Z) or a date (2026-09-01, midnight UTC)
      --date-to <DATE_TO>       Only jobs created at or before this time: RFC 3339 or a date (midnight UTC)
      --sort-by <SORT_BY>       The column to sort the jobs by, for example "id"
  -h, --help                    Print help
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
{"325430":{"obs_id":1090528304,"job_id":325430,"job_type":"DownloadVisibilities","job_state":"Ready","product":{"files":[{"type":"Acacia","url":"https://...","path":null,"size":10762878689,"sha1":"ca0e89e56cbeb05816dad853f5bab0b4075097da","format":"tar"}]},"created":"2026-09-08T05:41:54.757232Z","started":"2026-09-08T05:42:10Z","completed":"2026-09-08T06:00:00Z","modified":"2026-09-08T06:00:00Z","error_code":null,"error_text":null,"user_id":4242,"first_name":"Jane","last_name":"Citizen","job_params":{"obs_id":1090528304,"delivery":"acacia","delivery_format":"tar","download_type":"vis"}}}
```

The output is an object keyed by job ID. Each job has the keys `obs_id`, `job_id`, `job_type`,
`job_state`, `product`, `created`, `started`, `completed`, `modified`, `error_code`, `error_text`, `user_id`,
`first_name`, `last_name` and `job_params`, which are the MWA ASVO API's (OpenAPI) names. `product`
is `null` until the job has files; then its `files` list has, for each file, `type` (where it is
delivered: `Acacia`, `Scratch` or `Dug`), `url`, `path`, `size`, `sha1` and `format` (as the MWA
ASVO gives it, or `null`).

Before giant-squid 3.0.0 the keys were different (`obsid`, `jobId`, `jobType`, `jobState`, and
`fileUrl`, `filePath`, `fileSize`, `fileHash` for each file in a top-level `files` list; the
delivery type was also under `jobType`). See [docs/V3_MIGRATION.md](docs/V3_MIGRATION.md) for the
full list. For one release, `--legacy-json` (on `list` and `wait`) prints the old format, with
a warning on stderr. It will be removed in the release after 3.0.0, so update scripts to the new
keys.

`job_type` is any of:

- `Conversion`
- `DownloadVisibilities`
- `DownloadMetadata`
- `DownloadVoltage`
- `CancelJob`
- `DownloadBeamformer`
- `Imaging`
- `Unknown`

`job_state` is any of:

- `Queued`
- `WaitCal`
- `Staging`
- `Staged`
- `Downloading`
- `Preprocessing`
- `Imaging`
- `Delivering`
- `Ready`
- `Error`, which carries the error message, so it is an object: `{"Error": "some error message"}`
- `Expired`
- `Cancelled`

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
these identifiers can either be a list of jobIDs or a list of obsIDs, but not both.

Additionally, the `--job-states` and `--job-types` options can be used to further filter the output.
(The older names `--states` and `--types` still work.) `--days` (1 to 30; the default is the MWA ASVO API's,
shown in `--help`, so `list` shows your recent jobs and not your full history), `--date-from` and `--date-to`
(a date such as `2026-09-01`, which is midnight UTC, or an RFC 3339 time such as
`2026-09-01T12:00:00Z`) limit the listing by when the jobs were created, and `--sort-by` sets the
order.

These both take a comma-separated, case-insensitive list of values from the `job_type` and
`job_state` lists above. These can be provided in `TitleCase`, `UPPERCASE`, `lowercase`,
`kebab-case`, `snake_case`, or even `SPoNgeBOb-CAse`

example: show only jobs that match both of the following conditions:

- obsid is `1234567890` or `1234567891`
- job_type is `DownloadVisibilities`, `DownloadMetadata` or `CancelJob`
- job_state is `Preprocessing` or `Queued`

```bash
giant-squid list \
   --job-types download_visibilities,download-metadata,CANCELJOB \
   --job-states preprocessing, Queued \
   1234567890 1234567891
```

### Example: manual hash validation with Bash and jq

This example demonstrates how it is possible to stream the output of `giant-squid list --json` into
[`jq`](https://stedolan.github.io/jq/). This is the equivalent of what `giant-squid download` does,
but with the extra overhead of storing the tar to disk (`-k`).

```bash
set -eux
giant-squid list --json --job-types download_visibilities --job-states ready \
  | jq -r '.[]|[.job_id,.product.files[0].url//"",.product.files[0].size//"",.product.files[0].sha1//""]|@tsv' \
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

```text
Wait for MWA ASVO jobs to complete, return the urls

Usage: giant-squid wait [OPTIONS] [JOB_ID]...

Arguments:
  [JOB_ID]...  The job IDs to wait for. Files containing job IDs are also accepted

Options:
  -j, --json          Print the jobs as a simple JSON after waiting
      --legacy-json   Print the jobs as JSON in the old format of giant-squid before 3.0.0 (camelCase keys: obsid, jobId, jobType, jobState, fileUrl, ...). Deprecated: this option will be removed in the release after 3.0.0. Use --json
  -v, --verbosity...  The verbosity of the program. The default is to print high-level information
  -n, --no-colour     Disables colouring of output. Useful when you have a non-black terminal background for example
  -h, --help          Print help
```

Example:

```bash
$ giant-squid wait 31 32
13:41:02 [INFO] Waiting for 2 jobs to be ready...
13:41:04 [INFO] Job ID 31 (obsid: 1065880128): is Queued
13:41:04 [INFO] Job ID 32 (obsid: 1065880248): is Preprocessing
13:46:06 [INFO] Job ID 31 (obsid: 1065880128): is Ready
13:52:08 [INFO] Job ID 32 (obsid: 1065880248): is Ready
13:52:08 [INFO] All 2 MWA ASVO jobs are ready for download.
```

- `wait` takes job IDs only (they can also be in files, as for the other commands). An obsid, alone or next to job
  IDs, is an error that names it, and nothing is waited for. To find the job IDs of an obsid, use
  `giant-squid list <obsid>`.
- It stops at once, with a non-zero exit code, if a job is not in your job list, has an error, has expired or
  has been cancelled. Waiting longer would not change that.
- It waits for as long as it takes. Press Ctrl-C to stop.
- Every submit command has the same wait as the `-w`, `--wait` option.
- Log messages go to standard error, so the standard output of `wait --json` is only the JSON.

### Cancel MWA ASVO jobs

Use this command to ask the MWA ASVO to cancel jobs.

```text
Cancel MWA ASVO job

Usage: giant-squid cancel [OPTIONS] [JOB_ID]...

Arguments:
  [JOB_ID]...  The job IDs to be cancelled. Files containing job IDs are also accepted

Options:
  -n, --dry-run       Don't actually cancel; print information on what would've happened instead
  -v, --verbosity...  The verbosity of the program. The default is to print high-level information
  -h, --help          Print help
```

Example:

```bash
$ giant-squid cancel 31 32
13:20:41 [INFO] Cancel request for job 31: Job cancelled
13:20:41 [INFO] Cancel request for job 32: Unable to cancel job 32
13:20:41 [INFO] Cancel requests: 2 sent, 0 failed.
```

The log says "Cancel request" and not "Cancelled" because a reply from the MWA ASVO does not prove that the job
was cancelled:

- If a job is already cancelled, the MWA ASVO replies as normal, and only the message says that it did not cancel
  the job (job 32 above). Read the message of each job.
- If the MWA ASVO refuses a request in any other way (for example, there is no such job), `giant-squid` logs
  `Failed to cancel MWA ASVO job ID N: <reason>` and counts the request as failed. It carries on with the next
  job, and the exit code is still zero.

Like `wait`, `cancel` takes job IDs only: an obsid is an error that names it, and nothing is sent.

To check what `cancel` would send, without sending it, use `--dry-run`. To see the state of the jobs after a
cancel, use [`list`](#list-mwa-asvo-jobs).

### Download MWA ASVO jobs

Once an MWA ASVO job is "ready" the data is ready to be downloaded. If you set `--delivery=scratch` or `--delivery=dug` the data will be waiting for you on those filesystems and there is no 'downloading' to do.

```text
Download an MWA ASVO job

Usage: giant-squid download [OPTIONS] [JOB_ID_OR_OBS_ID]...

Arguments:
  [JOB_ID_OR_OBS_ID]...  The job IDs or obsids to be downloaded. Files containing job IDs or obsids are also accepted

Options:
  -d, --download-dir <DOWNLOAD_DIR>
          Which dir should downloads be written to [default: .]
  -k, --keep-tar
          Acacia delivery jobs only: Don't untar the contents of your download. NOTE: This option allows resuming downloads by rerunning giant-squid after an interruption. Giant-squid will resume where it left off [alias: --keep-zip]
  -r, --no-resume
          Do not attempt to resume a partial download. Leave the partial file alone
  -c, --concurrent-downloads <CONCURRENT_DOWNLOADS>
          Download up to this number of jobs concurrently. 2-4 is a good number for most users. Set this to 0 to use the number of CPU cores you machine has [default: 4]
      --skip-hash
          Don't verify the downloaded contents against the upstream hash
  -n, --dry-run
          Don't actually download; print information on what would've happened instead
  -v, --verbosity...
          The verbosity of the program. The default is to print high-level information
  -h, --help
          Print help
```

To download job ID 12345 to your current directory '.':

```bash
giant-squid download 12345
```

To download obsid 1065880128 to your current directory '.' (assuming your have a 'ready' job for that obsid):

```bash
giant-squid download 1065880128
```

(`giant-squid` differentiates between job IDs and obsids by the length of the
number specified; 10-digit numbers are treated as obsids.)

Text files containing job IDs or obsids may be used too.

You can specify the directory to download to by providing the `download_dir` parameter
to the `download` command. Ommitting this will default to your current dir `.`.

To download obsid 1065880128 to your `/tmp` directory:

```bash
giant-squid download --download-dir /tmp 1065880128
```

By default, `giant-squid` will perform stream untaring. Disable this with `--keep-tar`.

The MWA ASVO provides a SHA-1 of its downloads. `giant-squid` will verify the integrity
of your download by default. Give a `--skip-hash` to the `download` command to skip.

Jobs which were submitted with the /scratch data delivery option behave differently
than jobs submitted with the other data delivery options. When attempting to download
a /scratch job, if the path of the job (eg /scratch/mwaops/asvo/12345) is reachable from
the current host, it will be moved to the current working directory. Otherwise, it will
be skipped.

#### Download performance: Concurrent Downloads

By default, `giant-squid` will download 4 jobs concurrently (assuming you have specified 4 or more jobs to download).
This can help throughput if you have a good Internet connection, otherwise you may set the value manually by specifying:
`--concurrent-downloads N` where N is an integer equal or greater than 1.

#### Download performance: Changing the buffer size

By default, when downloading, `giant-squid` will store 100 MiB of the download
in memory before writing to disk. This is friendlier on disks (especially those
belonging to supercomputers!), and can make downloads faster.

The amount of data to cache before writing can be tuned by setting
`GIANT_SQUID_BUF_SIZE`. e.g.

```bash
export GIANT_SQUID_BUF_SIZE=50
giant-squid download 12345
```

would use 50 MiB of memory to cache the download before writing.

#### Resuming Interrupted Downloads

- `giant-squid` will attempt to resume an existing/interrupted download when the download command includes the `--keep-tar` option.
- Without the `--keep-tar` option, `giant-squid` _stream untars_ files (i.e. it downloads the tar from MWA ASVO and, in memory, untars files on the fly) which means it is not possible for `giant-squid` to be able to reliably resume an interrupted download.

## Installation

### Pre-compiled

Have a look at the [GitHub releases page](https://github.com/MWATelescope/giant-squid/releases).

### Python (pip)

- Run `pip install mwa-giant-squid`

  - This installs the `giant-squid` command, with the same commands and options as the Rust program,
    and the `mwa_giant_squid` Python module. See [docs/PYTHON.md](docs/PYTHON.md).

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

use mwa_giant_squid::asvo::apiv2::openapi::DownloadJobParams;
use mwa_giant_squid::{
    default_token_cache_path, AsvoClient, AsvoClientConfig, AsvoJobState, DownloadOptions,
    DownloadProgress, JobsFilter, DEFAULT_ASVO_HOST, DEFAULT_DOWNLOAD_BUFFER_SIZE,
    DEFAULT_DOWNLOAD_RETRY_DURATION,
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
        job_state: Some(AsvoJobState::Ready),
        ..JobsFilter::default()
    })?;
    for job in &ready.0 {
        println!("{} {} {}", job.job_id, job.obs_id, job.job_state);
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
        progress: Some(&progress),
        download_number: 1,
        download_count: 1,
        buffer_size: DEFAULT_DOWNLOAD_BUFFER_SIZE,
        retry_duration: DEFAULT_DOWNLOAD_RETRY_DURATION,
        should_stop: None,
    };
    client.download_job(job_id, &opts)?;
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

## Environment Variables

These variables are read by the `giant-squid` command. The Rust library
reads no environment variables (see
[Using giant-squid as a Rust library](#using-giant-squid-as-a-rust-library)).

| Variable | Description | Default |
|---|---|---|
| `MWA_ASVO_API_KEY` | Your MWA ASVO API key. **Required** for all operations. | — |
| `GIANT_SQUID_DELIVERY` | Default delivery option: `acacia`, `scratch`, or `dug`. Avoids needing to pass `-d` on every command. | `acacia` |
| `GIANT_SQUID_BUF_SIZE` | Download buffer size in MiB. Amount of data held in memory before writing to disk. | `100` |
| `CARGO_HOME` | Controls where `cargo install` places the `giant-squid` binary. | `~/.cargo` |

---

## Background

`giant-squid` was originally written in Haskell by chjordan and is still available on
[GitLab](https://gitlab.com/chjordan/giant-squid). It was later rewritten in Rust for
improved performance and maintainability. The Rust version is now the actively maintained
implementation and is what this repository contains.
