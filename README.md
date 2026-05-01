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
    - [Conversion downloads](#conversion-downloads)
    - [Imaging downloads](#imaging-downloads)
    - [Metadata downloads](#metadata-downloads)
    - [Visibility downloads](#visibility-downloads)
    - [Beamformer downloads](#beamformer-downloads)
    - [Voltage downloads](#voltage-downloads)    
    - [Resubmitting jobs](#resubmitting-jobs)
  - [List MWA ASVO jobs](#list-mwa-asvo-jobs)
  - [List MWA ASVO jobs in JSON](#list-mwa-asvo-jobs-in-json)
  - [Filter MWA ASVO job listing](#filter-mwa-asvo-job-listing)
  - [Example: manual hash validation with Bash and jq](#example-manual-hash-validation-with-bash-and-jq)
  - [Download MWA ASVO jobs](#download-mwa-asvo-jobs)
- [Installation](#installation)
  - [Pre-compiled](#pre-compiled)
  - [Building from crates.io](#building-from-cratesio)
  - [Building from source](#building-from-source)
- [Docker](#docker)
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

For any job submission commands, if you want to check that your command works without actually submitting the
obsids, then you can use the `--dry-run` option. 

```bash
$ giant-squid submit-vis 1065880128 --dry-run
13:10:06 [WARN] Using 'acacia' for MWA ASVO delivery
13:10:06 [INFO] Would have submitted 1 obsids for visibility download
```

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
- You can also optionally pass `--delivery-format=tar` to instruct MWA ASVO to deliver a tar of the files, rather than all of the individual files.
- This option is only available to users who have a Pawsey account with MWA group access and your `Pawsey Group` has been set in your MWA ASVO profile by an MWA ASVO administrator.
  - Please contact support to request this.  
- NOTE: all Pawsey users in the specified Pawsey Group can access your job's files. If you prefer to keep your data private to only you, you should choose the `acacia` delivery option as only you have the download URL.

##### Delivery: Down Under Geosolutions (DUG) Filesystem

- You can request that your job's files be delivered to DUG's filesystem.
- You can also optionally pass `--delivery-format=tar` to instruct MWA ASVO to deliver a tar of the files, rather than all of the individual files.
- To submit a job with the DUG delivery option, specify `--delivery=dug` on any job submission command.
- `Voltage` jobs are not able to be delivered to DUG currently.
- This option is only open to users who have a Curtin University DUG account and your `DUG Group` has been set in your MWA ASVO profile by an MWA administrator.
  - Please contact support to request this.
- NOTE: all DUG users in the specified DUG Group can access your job's files. If you prefer to keep your data private to only you, you should choose the `acacia` delivery option as only you have the download URL.

##### Changing Your Default Delivery Option

- You can set the environment variable `GIANT_SQUID_DELIVERY` to `acacia`, `scratch` or `dug` if you don't want to keep specifying the delivery option on the command line.

#### Conversion downloads

Conversion jobs refer to jobs which convert raw visibilities into either CASA measurement set or UVFITS format while optionally RFI flagging, averaging, correcting and applying calibration solutions to the converted data.

Conversion jobs use the Birli software package to preprocess MWA raw visibilities. For more information about Birli please see: [Birli on GitHub](https://github.com/MWATelescope/Birli).

```text
Submit MWA ASVO preprocessing/conversion jobs

Usage: giant-squid submit-conv [OPTIONS] [OBSID]...

Arguments:
  [OBSID]...  The obsids to be submitted. Files containing obsids are also accepted

Options:
  -p, --parameters <PARAMETERS>
          The Birli parameters used. If any of the default parameters are not overwritten, then they remain. If the delivery option is specified here, it is ignored; delivery must be passed in as a command-line argument. Default: avg_freq_res=80, flag_edge_width=80, output=uvfits
  -d, --delivery <DELIVERY>
          Tell MWA ASVO where to deliver the data. The default is "acacia", which provides a download URL which you can download with giant-squid, wget, etc. Other options are: "dug" and "scratch", to deliver data directly to a target filesystem, but these are only available when your MWA ASVO profile has a "DUG Group" or "Pawsey Group" set. Please see README.md for more information on delivery options. The default can be overridden with the environment variable GIANT_SQUID_DELIVERY
  -f, --delivery-format <DELIVERY_FORMAT>
          Tell MWA ASVO to deliver the data in a particular format. Available value(s): `tar`. NOTE: this option does not apply if delivery = `acacia` which is always `tar`
  -w, --wait
          Do not exit giant-squid until the specified obsids are ready for download
  -n, --dry-run
          Don't actually submit; print information on what would've happened instead
  -r, --allow-resubmit
          Allow resubmit- if exact same job params already in your queue allow submission anyway. Default: allow resubmit is False / not present
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

The default conversion options can be found by running the help text:

```bash
giant-squid submit-conv --help
```

To change the default conversion options and/or specify more options, specify
comma-separated key-value pairs like so:

```bash
giant-squid submit-conv 1065880128 --parameters=avg_time_res=0.5,avg_freq_res=10
```

##### Key/Value Pairs for Conversion Job -p / --parameters

| key | Meaning | Values | Default |
|---|---|---|---|
|output | Output data format (CASA measurement set or UVFITS) | 'ms', 'uvfits'| 'ms'
|avg_time_res | Output time resolution in seconds (must be multiple of, or equal to, correlator time resolution) | 0.5 - 32.0 | omitted (will use correlator time resolution)
|avg_freq_res | Output frequency resolution in kHz (must be multiple of, or equal to, correlator frequency resolution). if omitted  (will use correlator frequency resolution) | 0.4 - 1280.0 | 80.0
|flag_edge_width | Width (in kHz) to flag at each coarse channel edge. Must be multiple of, or equal to, correlator frequency resolution. If omitted (will not flag edge channels) | 0.2 - 640.0 | 80.0
|centre | Phase centre to use | 'phase' (use observation phase centre), 'pointing' (use pointing centre), 'custom' (use custom phase centre) | 'phase'
|phase_centre_ra | If 'custom' phase centre, the right ascension  (in decimal degrees) of the new phase centre | 0-360 | omitted
|phase_centre_dec | If 'custom' phase centre, the declination (in decimal degrees) of the new phase centre | -90.0 - +90.0  | omitted
|apply_di_cal | Apply basic direction-independent calibration solution (if available)| true or omit it | omitted (will not apply calibration)
|no_rfi| Will disable radio frequency interference (RFI) flagging | true or omit it | ommitted (will perform RFI flagging)
|no_geometric_delay | Do not correct geometric delays (only applicable if not already applied by correlator)| true or omit it | omitted (will correct geometric delays)
|no_cable_delay | Do not correct cable length delays (only applicable if not already applied by correlator)| true or omit it | omitted (will correct cable length delays)
|no_digital_gains | Do not correct the digital gains|true or omit it | omitted (will correct for digital gains)
|no_flag_dc | Do not flag the DC channel | true or omit it| omitted (will flag DC channel)
|no_passband_gains | Do not correct the passband gains |true or omit it | omitted (will correct passband gains)

#### Imaging downloads

An "imaging download job" takes raw visibilities or an existing completed conversion job and produces an image. If an obsid is passed then the raw visibilities are converted to a CASA measurement set first, just like a regular [Conversion](#conversion-downloads) job.

The MWA ASVO imaging features uses the WSClean software by André Offringa to generated images from CASA measurement sets. For comprehensive documentation about WSClean, please see: [WSClean readthedocs](https://wsclean.readthedocs.io/).

```text
TODO
```

To submit an imaging download job for the obsid 1065880128, specify the obsid and any conversion parameters as well as imaging parameters:

```bash
giant-squid submit-image 1065880128 --parameters=avg_time_res=0.5,avg_freq_res=10,image_size=2048,multiscale=true
```

To submit an imaging download job for your existing conversion job 12345:

```bash
giant-squid submit-image 12345 --parameters=image_size=2048,multiscale=true
```

Some notes about submiting an imaging job based on an existing conversion job:
* conversion job parameters are invalid as your existing job has already been converted.
* only conversion jobs which output a CASA measurement set are able to be imaged.
* only conversion jobs where the data was delivered to Acacia or Scratch are able to be imaged.

##### Key/Value Pairs for Imaging Job -p / --parameters

In addition to the conversion job parameters [See: Conversion Downloads](#conversion-downloads), imaging jobs take the following parameters:

| key | Meaning | Values | Default |
|---|---|---|---|
|image_size | width and height in pixels of output image | One of 512, 1024, 2048, 3072, 4096 or 8192 | 3072
|pixel_scale | Number of arcsecs per pixel | 10.0 - 120.0 | 20.0
|weighting | Type of weighting to apply | One of natural, uniform, briggs | briggs
|robust | Robustness parameter- only used if `weighting=briggs` | -2.0 to 2.0 | -0.5 
|clean_iterations | Maximum number of clean iterations to perform | 0 to 1000000 | 100000
|clean_threshold | Absolute stopping clean thresholding in Jy. | 0.0 to 10.0 | 0.001
|auto_threshold | Relative clean threshold. Estimate noise level using a robust estimator and stop at sigma x stddev | 0.1 to 5.0 | 0.5
|nwlayers | Number of w-layers to use | 32 to 512 | 128
|multiscale | Clean on different scales. This is a new algorithm. This parameter invokes the optimized multiscale algorithm published by Offringa & Smirnov (2017). | true or false | true
|apply_primary_beam | Calculate and apply the primary beam and save images for the Jones components, with weighting identical to the weighting as used by the imager. | true or false | true

#### Metadata downloads

A "metadata download job" refers to a job which provides a tar containing a
metafits file and cotter flags for a single obsid.

```text
Submit MWA ASVO jobs to download MWA metadata- metafits (with PPDs for each tile) and RFI flags (if available)

Usage: giant-squid submit-meta [OPTIONS] [OBSID]...

Arguments:
  [OBSID]...  The obsids to be submitted. Files containing obsids are also accepted

Options:
  -d, --delivery <DELIVERY>
          Tell MWA ASVO where to deliver the data. The default is "acacia", which provides a download URL which you can download with giant-squid, wget, etc. Other options are: "dug" and "scratch", to deliver data directly to a target filesystem, but these are only available when your MWA ASVO profile has a "DUG Group" or "Pawsey Group" set. Please see README.md for more information on delivery options. The default can be overridden with the environment variable GIANT_SQUID_DELIVERY
  -f, --delivery-format <DELIVERY_FORMAT>
          Tell MWA ASVO to deliver the data in a particular format. Available value(s): `tar`. NOTE: this option does not apply if delivery = `acacia` which is always `tar`
  -w, --wait
          Do not exit giant-squid until the specified obsids are ready for download
  -n, --dry-run
          Don't actually submit; print information on what would've happened instead
  -r, --allow-resubmit
          Allow resubmit- if exact same job params already in your queue allow submission anyway. Default: allow resubmit is False / not present
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

Usage: giant-squid submit-vis [OPTIONS] [OBSID]...

Arguments:
  [OBSID]...  The obsids to be submitted. Files containing obsids are also accepted

Options:
  -d, --delivery <DELIVERY>
          Tell MWA ASVO where to deliver the data. The default is "acacia", which provides a download URL which you can download with giant-squid, wget, etc. Other options are: "dug" and "scratch", to deliver data directly to a target filesystem, but these are only available when your MWA ASVO profile has a "DUG Group" or "Pawsey Group" set. Please see README.md for more information on delivery options. The default can be overridden with the environment variable GIANT_SQUID_DELIVERY
  -f, --delivery-format <DELIVERY_FORMAT>
          Tell MWA ASVO to deliver the data in a particular format. Available value(s): `tar`. NOTE: this option does not apply if delivery = `acacia` which is always `tar`
  -w, --wait
          Do not exit giant-squid until the specified obsids are ready for download
  -n, --dry-run
          Don't actually submit; print information on what would've happened instead
  -r, --allow-resubmit
          Allow resubmit- if exact same job params already in your queue allow submission anyway. Default: allow resubmit is False / not present
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

Usage: giant-squid submit-bf [OPTIONS] [OBSID]...

Arguments:
  [OBSID]...  The obsids to be submitted. Files containing obsids are also accepted

Options:
  -d, --delivery <DELIVERY>
          Tell MWA ASVO where to deliver the data. The default is "acacia", which provides a download URL which you can download with giant-squid, wget, etc. Other options are: "dug" and "scratch", to deliver data directly to a target filesystem, but these are only available when your MWA ASVO profile has a "DUG Group" or "Pawsey Group" set. Please see README.md for more information on delivery options. The default can be overridden with the environment variable GIANT_SQUID_DELIVERY
  -f, --delivery-format <DELIVERY_FORMAT>
          Tell MWA ASVO to deliver the data in a particular format. Available value(s): `tar`. NOTE: this option does not apply if delivery = `acacia` which is always `tar`
  -w, --wait
          Do not exit giant-squid until the specified obsids are ready for download
  -n, --dry-run
          Don't actually submit; print information on what would've happened instead
  -r, --allow-resubmit
          Allow resubmit- if exact same job params already in your queue allow submission anyway. Default: allow resubmit is False / not present
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

Usage: giant-squid submit-volt [OPTIONS] --offset <OFFSET> --duration <DURATION> [OBSID]...

Arguments:
  [OBSID]...  The obsids to be submitted. Files containing obsids are also accepted

Options:
  -d, --delivery <DELIVERY>          Tell MWA ASVO where to deliver the data. The only valid value for a voltage job is "scratch", but this is only available when your MWA ASVO profile has the "mwavcs" "Pawsey Group" set. Please see README.md for more information on delivery options. The default can be overridden with the environment variable GIANT_SQUID_DELIVERY
  -o, --offset <OFFSET>              The offset in seconds from the start GPS time of the observation
  -u, --duration <DURATION>          The duration (in seconds) to download
  -f, --from-channel <FROM_CHANNEL>  The 'from' receiver channel number (0-255)
  -t, --to-channel <TO_CHANNEL>      The 'to' receiver channel number (0-255)
  -w, --wait                         Do not exit giant-squid until the specified obsids are ready for download
  -n, --dry-run                      Don't actually submit; print information on what would've happened instead
  -r, --allow-resubmit               Allow resubmit- if exact same job params already in your queue allow submission anyway. Default: allow resubmit is False / not present
  -v, --verbosity...                 The verbosity of the program. The default is to print high-level information
  -h, --help                         Print help
  ```

To submit a voltage download job for the obsid 1065880128:

```bash
giant-squid submit-volt --delivery scratch --offset 0 --duration 8 1065880128
```

Text files containing obsids may be used too.

For MWAX_VCS or MWAX_BUFFER voltage observations you can optionally pass `--from_channel` and `--to_channel` to restrict the job to
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

Usage: giant-squid list [OPTIONS] [JOBID_OR_OBSID]...

Arguments:
  [JOBID_OR_OBSID]...  job IDs or obsids to filter by. Files containing job IDs or obsids are also accepted

Options:
  -j, --json            Print the jobs as a simple JSON
  -v, --verbosity...    The verbosity of the program. The default is to print high-level information
      --states <STATE>  show only jobs matching the provided states, case insensitive. Options: queued, waitcal, staging, staged, retrieving, preprocessing, imaging, delivering, ready, error, expired, cancelled
      --types <TYPE>    filter job list by type, case insensitive with underscores. Options: conversion, download_visibilities, download_metadata, download_voltage or cancel_job
  -n, --no-colour       Disables colouring of output. Useful when you have a non-black terminal background for example
  -h, --help            Print help
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
{"325430":{"obsid":1090528304,"jobId":325430,"jobType":"DownloadVisibilities","jobState":"Ready","files":[{"fileName":"1090528304_vis.tar","fileSize":10762878689,"fileHash":"ca0e89e56cbeb05816dad853f5bab0b4075097da"}]},"325431":{"obsid":1090528432,"jobId":325431,"jobType":"DownloadVisibilities","jobState":"Ready","files":[{"fileName":"1090528432_vis.tar","fileSize":10762875021,"fileHash":"9d9c3c0f56a2bb4e851aa63cdfb79095b29c66c9"}]}}
```

`jobType` is allowed to be any of:

- `Conversion`
- `DownloadVisibilities`
- `DownloadMetadata`
- `DownloadVoltage`
- `CancelJob`
- `DownloadBeamformer`
- `Imaging`
- `Unknown`

`jobState` is allowed to be any of:

- `Queued`
- `WaitCal`
- `Staging`
- `Staged`
- `Downloading`
- `Preprocessing`
- `Imaging`
- `Delivering`
- `Ready`
- `Error: Text` (e.g. "Error: some error message")
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

Additionally, the `--states` and `--types` options can be used to further filter the output.

These both taks a comma-separated, case-insensitive list of values from the `jobType` and
`jobState` lists above. These can be provided in `TitleCase`, `UPPERCASE`, `lowercase`,
`kebab-case`, `snake_case`, or even `SPoNgeBOb-CAse`

example: show only jobs that match both of the following conditions:

- obsid is `1234567890` or `1234567891`
- jobType is `DownloadVisibilities`, `DownloadMetadata` or `CancelJob`
- jobState is `Preprocessing` or `Queued`

```bash
giant-squid list \
   --types download_visibilities,download-metadata,CANCELJOB \
   --states preprocessing, Queued \
   1234567890 1234567891
```

### Example: manual hash validation with Bash and jq

This example demonstrates how it is possible to stream the output of `giant-squid list --json` into
[`jq`](https://stedolan.github.io/jq/). This is the equivalent of what `giant-squid download` does,
but with the extra overhead of storing the tar to disk (`-k`).

```bash
set -eux
giant-squid list --json --types download_visibilities --states ready \
  | jq -r '.[]|[.jobId,.files[0].fileUrl//"",.files[0].fileSize//"",.files[0].fileHash//""]|@tsv' \
  | tee ready.tsv
while read -r jobid url size hash; do
   # note: it's a good idea to check you have enough disk space here using $size.
   wget $url -O ${jobid}.tar --progress=dot:giga --wait=60 --random-wait
   sha1=$(sha1sum ${jobid}.tar | cut -d' ' -f1)
   if [ "\$sha1" != "\$hash" ]; then
      echo "Download failed, hash mismatch. Expected $hash, got $sha1"
      exit 1
   fi
   tar -xf ${jobid}.tar
do < ready.tsv
```

### Download MWA ASVO jobs

Once an MWA ASVO job is "ready" the data is ready to be downloaded. If you set `--delivery=scratch` or `--delivery=dug` the data will be waiting for you on those filesystems and there is no 'downloading' to do.

```text
Download an MWA ASVO job

Usage: giant-squid download [OPTIONS] [JOBID_OR_OBSID]...

Arguments:
  [JOBID_OR_OBSID]...  The job IDs or obsids to be downloaded. Files containing job IDs or obsids are also accepted

Options:
  -d, --download-dir <DOWNLOAD_DIR>
          Which dir should downloads be written to [default: .]
  -k, --keep-tar
          Acacia delivery jobs only: Don't untar the contents of your download. NOTE: This option allows resuming downloads by rerunning giant-squid after an interruption. Giant-squid will resume where it left off [aliases: --keep-zip]
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

## Environment Variables

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