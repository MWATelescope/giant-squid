# giant-squid

<div class="bg-gray-dark" align="center" style="background-color:#24292e">
<br/>
<a href="https://docs.rs/crate/mwa_giant_squid"><img src="https://docs.rs/mwa_giant_squid/badge.svg" alt="docs"></a>
<img src="https://github.com/MWATelescope/giant-squid/workflows/Cross-platform%20tests/badge.svg" alt="Cross-platform%20tests">
</div>

An alternative MWA ASVO client. See the [MWA ASVO
page](https://asvo.mwatelescope.org/) for more info.

`giant-squid` was originally created as a library to do MWA ASVO related tasks
in the Haskell programming language (now available in Rust). However, it's not
just a library; the `giant-squid` executable acts as an alternative to the
[manta-ray-client](https://github.com/ICRAR/manta-ray-client) and may better
suit users for a few reasons:

1. By default, `giant-squid` _stream untars_ the downloads from ASVO. In other
   words, rather than downloading a potentially large (> 100 GiB!) tar file and
   then untarring it yourself (thereby occupying double the space of the
   original tar and performing a very expensive IO operation), it is possible to
   get the files without performing an untar.

2. `giant-squid` does not require a CSV file to submit jobs; this is instead
   handled by command line arguments.

3. For any commands that accept obsids or job IDs, it is possible use text files
   instead. These files are unpacked as if you had typed them out manually, and
   each entry of the text file(s) are checked for validity (all ints and all
   10-digits long); any exceptions are reported and the command fails.

4. One can ask `giant-squid` to print their ASVO queue as JSON; this makes
   parsing the state of your jobs in another programming language much simpler.

## Usage

### Print help text

```bash
giant-squid -h
```

This also applies to all of the subcommands, e.g.

```bash
giant-squid download -h
```

### Print the `giant-squid` version

```bash
giant-squid --version
giant-squid -V
```

(Useful if things are changing over time!)

### List ASVO jobs

```bash
giant-squid list
giant-squid l
```

### List ASVO jobs in JSON

the following commands are equivalent:

```bash
giant-squid list --json
giant-squid list -j
giant-squid l -j
```

Example output:

```bash
giant-squid list -j
{"325430":{"obsid":1090528304,"jobId":325430,"jobType":"DownloadVisibilities","jobState":"Ready","files":[{"fileName":"1090528304_vis.zip","fileSize":10762878689,"fileHash":"ca0e89e56cbeb05816dad853f5bab0b4075097da"}]},"325431":{"obsid":1090528432,"jobId":325431,"jobType":"DownloadVisibilities","jobState":"Ready","files":[{"fileName":"1090528432_vis.zip","fileSize":10762875021,"fileHash":"9d9c3c0f56a2bb4e851aa63cdfb79095b29c66c9"}]}}
```

`jobType` is allowed to be any of:

- `Conversion`
- `DownloadVisibilities`
- `DownloadMetadata`
- `DownloadVoltage`
- `CancelJob`

`jobState` is allowed to be any of:

- `Queued`
- `Processing`
- `Ready`
- `Error: Text` (e.g. "Error: some error message")
- `Expired`
- `Cancelled`

Example reading this in Python:

```bash
$ giant-squid list -j > /tmp/asvo.json
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

### Filter ASVO job listing

`giant-squid list` takes an optional list of identifiers that can be used to filter the job listing,
these identifiers can either be a list of jobIDs or a list of obsIDs, but not both.

Additionally, the `--states` and `--types` options can be used to further filter the output.

These both taks a comma-separated, case-insensitive list of values from the `jobType` and
`jobState` lists above. These can be provided in `TitleCase`, `UPPERCASE`, `lowercase`,
`kebab-case`, `snake_case`, or even `SPoNgeBOb-CAse`

example: show only jobs that match both of the following conditions:

- obsid is `1234567890` or `1234567891`
- jobType is `DownloadVisibilities`, `DownloadMetadata` or `CancelJob`
- jobState is `Processing` or `Queued`

```bash
giant-squid list \
   --types dOwNlOaD__vIsIbIlItIeS,download-metadata,CANCELJOB \
   --states pRoCeSsInG,__Q_u_e_u_e_D__ \
   1234567890 1234567891
```

### Example: manual hash validation with Bash and jq

This example demonstrates how it is possible to stream the output of `giant-squid list -j` into
[`jq`](https://stedolan.github.io/jq/). This is the equivalent of what `giant-squid download` does,
but with the extra overhead of storing the tar to disk (`-k`).

```bash
set -eux
giant-squid list -j --types download_visibilities --states ready \
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

### Download ASVO jobs

To download job ID 12345:

```bash
giant-squid download 12345
# or
giant-squid d 12345
```

To download obsid 1065880128:

```bash
giant-squid download 1065880128
# or
giant-squid d 1065880128
```

(`giant-squid` differentiates between job IDs and obsids by the length of the
number specified; 10-digit numbers are treated as obsids. If the ASVO ever
serves up more than a billion jobs, you have permission to be upset with me. The
same applies if this code is still being used in the year 2296.)

Text files containing job IDs or obsids may be used too.

By default, `giant-squid` will perform stream unzipping. Disable this with `-k`
(or `--keep-zip`).

The MWA ASVO provides a SHA-1 of its downloads. To verify the integrity of your
download, give a `--hash` to the `download` command. The additional computation
by this action appears to be negligible, but I made this option non-default as
it shouldn't be a problem anyway.

Jobs which were submitted with the /astro data delivery option behave differently
than jobs submitted with the acacia data delivery option. When attempting to download
an /astro job, if the path of the job (eg /astro/mwaops/asvo/12345) is reachable from
the current host, it will be moved to the current working directory. Otherwise, it will
be skipped.

### Submit ASVO jobs

#### Visibility downloads

A "visibility download job" refers to a job which provides a zip containing
gpubox files, a metafits file and cotter flags for a single obsid.

To submit a visibility download job for the obsid 1065880128:

```bash
giant-squid submit-vis 1065880128
# or
giant-squid sv 1065880128
```

Text files containing obsids may be used too.

If you want to check that your command works without actually submitting the
obsids, then you can use the `--dry-run` option (short version `-n`).

You can choose whether to have your files tarred up and uploaded to Pawsey's Acacia (default),
or you can request that the files be left on Pawsey's /astro filesystem. The second option requires
that your Pawsey group be set in your ASVO account, please contact an admin to request this. To submit
a job with the /astro option, set the environment variable GIANT_SQUID_DELIVERY=astro.

#### Conversion downloads

To submit a conversion job for obsid 1065880128:

```bash
giant-squid submit-conv 1065880128
# or
giant-squid sc 1065880128
```

Text files containing obsids may be used too.

The default conversion options can be found by running the help text:

```bash
giant-squid submit-conv -h
```

To change the default conversion options and/or specify more options, specify
comma-separated key-value pairs like so:

```bash
giant-squid submit-conv 1065880128 -p avg_time_res=0.5,avg_freq_res=10
```

If you want to check that your command works without actually submitting the
obsids, then you can use the `--dry-run` option (short version `-n`). More
messages (including what `giant-squid` uses for the conversion options) can be
accessed with `-v` (or `--verbose`). e.g.

```bash
$ giant-squid submit-conv 1065880128 -nv -p avg_time_res=0.5,avg_freq_res=10
20:40:24 [INFO] Would have submitted 1 obsids for conversion, using these parameters:
{"output": "uvfits", "job_type": "conversion", "flag_edge_width": "160", "avg_freq_res": "10", "avg_time_res": "0.5"}
```

You can choose whether to have your files tarred up and uploaded to Pawsey's Acacia (default),
or you can request that the files be left on Pawsey's /astro filesystem. The second option requires
that your Pawsey group be set in your ASVO account, please contact an admin to request this. To submit
a job with the /astro option, set the environment variable GIANT_SQUID_DELIVERY=astro.

#### Metadata downloads

A "metadata download job" refers to a job which provides a zip containing a
metafits file and cotter flags for a single obsid.

To submit a visibility download job for the obsid 1065880128:

```bash
giant-squid submit-meta 1065880128
# or
giant-squid sm 1065880128
```

Text files containing obsids may be used too.

If you want to check that your command works without actually submitting the
obsids, then you can use the `--dry-run` option (short version `-n`).

You can choose whether to have your files tarred up and uploaded to Pawsey's Acacia (default),
or you can request that the files be left on Pawsey's /astro filesystem. The second option requires
that your Pawsey group be set in your ASVO account, please contact an admin to request this. To submit
a job with the /astro option, set the environment variable GIANT_SQUID_DELIVERY=astro.

## Download performance

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

## Installation

### Pre-compiled

Have a look at the [GitHub releases
page](https://github.com/MWATelescope/giant-squid/releases).

### Building from crates.io

- Install [Rust](https://www.rust-lang.org/tools/install)

- Run `cargo install mwa_giant_squid`

  - The final executable will be at `~./cargo/bin/giant-squid`

  - This destination can be configured with the `CARGO_HOME` environment
    variable.

### Building from source

- Install [Rust](https://www.rust-lang.org/tools/install)

- Clone this repo and `cd` into it

  `git clone https://github.com/MWATelescope/giant-squid && cd giant-squid`

- Run `cargo install --path .`

  - The final executable will be at `~./cargo/bin/giant-squid`

  - This destination can be configured with the `CARGO_HOME` environment
    variable.

## Docker

You can run giant-squid using docker

```bash
docker run mwatelescope/giant-squid:latest -h
```

## Other

The Haskell code is still available on chj's
[GitLab](https://gitlab.com/chjordan/giant-squid). Switching to Rust means that
the code is more efficient and the code is easier to read (sorry Haskell. I love
you, but you're weird).
