# Migrating from giant-squid 2.x to 3.0.0

This guide is for people who use the `giant-squid` command line. It lists
what changed from 2.x (the last is 2.5.1) to 3.0.0, and what to do about
each change. Most commands work as before; the changes that can alter your
results or break a script are in the checklist below.

Why the changes: giant-squid 3.0.0 uses version 2 of the MWA ASVO API. The
job options, their names and their defaults now come from that API, so
giant-squid, the API and its documentation use the same names everywhere.

## Quick checklist

1. If you submit **conversion or imaging jobs**, check the new defaults
   (output format, averaging, multiscale, w-layers): see
   [Conversion jobs](#conversion-jobs-submit-conv) and
   [Imaging jobs](#imaging-jobs-submit-image-and-submit-image-from-job).
2. If you use **`-p` / `--parameters`**, replace it with one option for each
   key: see [No more `--parameters`](#no-more--p----parameters).
3. If you run **`submit-image` with a job ID**, use `submit-image-from-job`:
   see [Imaging jobs](#imaging-jobs-submit-image-and-submit-image-from-job).
4. If a script reads **`list --json`** or **`wait --json`**, update it to
   the new key names, or use `--legacy-json` for one release: see
   [JSON output](#json-output-list---json-wait---json).
5. If a script reads **log lines** (for example "Submitted ... as MWA ASVO
   job ID ..."), read `--json` instead: see
   [Output: logs go to stderr](#output-logs-go-to-stderr).
6. If a script reads the **output of `cancel`**, or gives an **obsid to `wait`
   or `cancel`**, change it: see
   [Waiting and cancelling](#waiting-and-cancelling-wait-cancel).
7. If a script relies on `giant-squid list` (or `wait`, or `download`) seeing
   **jobs older than 30 days**, change it: see
   [Listing jobs](#listing-jobs-list).

Your API key, environment variables and download commands need no change.

## No more `-p` / `--parameters`

In 2.x, `submit-conv` and `submit-image` took their job options as
comma-separated `key=value` pairs:

```bash
# 2.x
giant-squid submit-conv 1065880128 --parameters=avg_time_res=0.5,avg_freq_res=10
```

In 3.0.0 each option is a command-line option of its own, with the API's
name. Write the key with `--` in front, and `-` in place of `_`:

```bash
# 3.0.0
giant-squid submit-conv 1065880128 --avg-time-res 0.5 --avg-freq-res 10
```

A value that is out of range is now rejected before anything is sent, with
a message that gives the range (for example
`--mgain: must be between 0.1 and 1 (got 1.5)`). Run
`giant-squid submit-conv --help` (or any other command) to see every option
and its default.

Keys whose names changed:

| 2.x key | 3.0.0 option | Note |
|---|---|---|
| `phase_centre_ra` | `--custom-centre-ra` | `--phase-centre-ra` still works |
| `phase_centre_dec` | `--custom-centre-dec` | `--phase-centre-dec` still works |
| `no_geometric_delay` | `--no-geometry-delay` | The API's name |
| `centre` (imaging) | `--centre` | `--phase-center` still works |

A flag that was `key=true` in 2.x is an option with no value in 3.0.0 (for
example `no_rfi=true` is `--no-rfi`). The imaging options that default to
true (`--apply-di-cal`, `--apply-primary-beam`, `--join-channels`) take an
optional value, so you can turn them off with `--apply-primary-beam=false`.

## Conversion jobs (`submit-conv`)

The defaults changed. In 2.x, giant-squid set its own
defaults for a conversion job. In 3.0.0 the defaults are the MWA ASVO API's:

| Option | 2.x default | 3.0.0 default |
|---|---|---|
| Output format | `uvfits` | `ms` (CASA measurement set) |
| Frequency averaging | 80 kHz | 40 kHz (`--avg-freq-res 40`) |
| Time averaging | none (correlator resolution) | 2 s (`--avg-time-res 2`) |
| Edge flagging | 80 kHz | 80 kHz (unchanged) |
| Delivery format (Scratch or DUG) | individual files (`tar` only if asked for) | `tar` (`--delivery-format files` for individual files) |

So a 2.x command with no options gives different files in 3.0.0. To get
the 2.x result, give the options:

```bash
# 3.0.0, the same output as `giant-squid submit-conv 1065880128` in 2.x
giant-squid submit-conv 1065880128 --output uvfits --avg-freq-res 80
```

There is no option for "no time averaging" in 3.0.0: give `--avg-time-res`
the correlator's time resolution of your observation to keep it.

The other conversion options (`--apply-di-cal`, `--centre`, `--no-rfi`,
`--no-cable-delay`, `--no-digital-gains`, `--no-flag-dc`,
`--no-geometry-delay`, `--no-passband-gains`) do what the 2.x keys did.

## Imaging jobs (`submit-image` and `submit-image-from-job`)

In 2.x, `submit-image` took obsids **or** the job IDs of completed
conversion jobs. In 3.0.0 these are two commands:

```bash
# 2.x
giant-squid submit-image 1065880128 --parameters=image_size=2048,multiscale=true
giant-squid submit-image 12345 --parameters=image_size=2048,multiscale=true

# 3.0.0
giant-squid submit-image 1065880128 --image-size 2048 --multiscale
giant-squid submit-image-from-job --source-job-id 12345 1065880128 --image-size 2048 --multiscale
```

`submit-image-from-job` (alias `sifj`) needs the conversion job's obsid as
well as its job ID. As in 2.x, the conversion job must have made a CASA
measurement set, delivered to Acacia or Scratch.

The imaging defaults are now the API's. Most are as in
2.x (image size 3072, pixel scale 20, `briggs` weighting with robust -0.5,
auto threshold 0.5, clean threshold 0.001, primary beam applied, output
`fits`), but two changed:

| Option | 2.x default | 3.0.0 default |
|---|---|---|
| Multiscale cleaning | on | off: give `--multiscale` to turn it on |
| w-layers | 128 | not set, so the MWA ASVO decides: give `--wstack-nwlayers 128` for the 2.x value (`--nwlayers` also works, but the API calls it deprecated) |

So to image as 2.x did by default:

```bash
# 3.0.0, the same imaging options as 2.x's defaults
giant-squid submit-image 1065880128 --multiscale --wstack-nwlayers 128
```

For an obsid, the conversion part of an imaging job also has the new
conversion defaults (see above). Run `giant-squid submit-image --help` to
see every default. `--pol` (`XX`, `YY` or `XXYY`, default `XXYY`) is new.

## Other submit commands

`submit-vis`, `submit-meta`, `submit-volt` and `submit-bf` take the same
options as in 2.x. The default delivery format is now `tar` for every job
except voltage jobs, whatever the delivery: in 2.x a Scratch or DUG delivery
gave individual files unless you asked for `tar`. Add `--delivery-format files`
to get the individual files. `submit-volt --offset` must now be from 0 to 5400, and a
negative value (`--offset -1`) is rejected with that range.

Every submit command has a new `--json` option, which prints the MWA ASVO's
reply for each job as one line of JSON, for example
`{"job_id":12345,"message":"...","status":"success"}`. Use it in scripts;
see the next section. The `status` text and the `message` describe the reply; the exit code of
`giant-squid` and the HTTP status of the MWA ASVO tell you whether a submission worked.

`--dry-run` now prints the request body that would be sent, as JSON, for
each obsid.

## Output: logs go to stderr

In 2.x, giant-squid wrote its log lines (information and
warnings) to standard output. In 3.0.0 all log lines go to standard error,
so that standard output has only the command's output: the job table,
`--json` output, and the `--json` replies of the submit commands. The
`--dry-run` reports are log lines, so they are on standard error too.

If a script read a log line to get a job ID, change it to read `--json`:

```bash
# 2.x: read the job ID from a log line
giant-squid submit-vis 1065880128 | grep "as MWA ASVO job ID"

# 3.0.0: read it from the JSON reply
giant-squid submit-vis --json 1065880128 | jq -r .job_id
```

To keep the log lines in a file, redirect standard error:
`giant-squid list 2> giant-squid.log`.

### Errors with `--json`

With `--json`, a command prints only JSON on standard output, and nothing on
standard error: no log lines and no progress bars. Each error is one line of
JSON in the form of the MWA ASVO's error response (`error_code`, `message`, and
`detail`, `suggestion`, `field_errors`, `request_id` when known), with the
`obs_id` or `job_id` it is about. A line with an `error_code` key is an error:

```bash
giant-squid submit-vis --json 1065880128 | jq -r 'select(.error_code) | .message'
```

`-v` cannot be used with `--json` (or with `--legacy-json`). With `--json`, `--dry-run` prints each
request as one line of JSON.

## Listing jobs (`list`)

The filter options have the API's names. The 2.x names still work:

| 2.x | 3.0.0 |
|---|---|
| `--states` | `--job-states` |
| `--types` | `--job-types` |

New options: `--date-from` and `--date-to` (a date such as `2026-09-01`,
which is midnight UTC, or a time such as `2026-09-01T12:00:00Z`) and
`--sort-by`. `--days` takes 1 to 30, the limits of the MWA ASVO API; other
values are refused before any request.

**`list` without `--days` shows the MWA ASVO's default window, not your full
history.** In 2.x, `giant-squid list` with no `--days` fetched every job you
have. In 3.0.0 it asks for the API's default, which is the past 30 days (the
most the API takes in `--days`). `giant-squid list --help` shows the default.
`wait` and `download` list jobs the same way, so they find the jobs of the past
30 days.

**The job states are the API's.** `--job-states` takes the MWA ASVO API's
states, and the table and the JSON show them as the API does:

| 2.x | 3.0.0 |
|---|---|
| `ready` (`Ready` in the JSON) | `completed` |
| `waitcal` (`WaitCal` in the JSON) | `waitcal` |
| `expired` | removed: the MWA ASVO has no such state |
| `error` (`{"Error": "<message>"}` in the JSON) | `error`; the message is in `error_text` |

The other states have the same names, in lower case in the JSON (for
example `queued`). `preparing` and `imaging` are new. The names are still
case insensitive. `giant-squid list --help` lists them.

**The job type names are the API's.** The MWA ASVO API gives each job type a
code and a name. `--job-types` takes the names, and the table shows them. The
JSON shows the code:

| 2.x name | 2.x JSON | 3.0.0 name | 3.0.0 JSON |
|---|---|---|---|
| `conversion` | `Conversion` | `conversion` | `0` |
| `download_visibilities` | `DownloadVisibilities` | `visibility` | `1` |
| `download_metadata` | `DownloadMetadata` | `metadata` | `2` |
| `download_voltages` | `DownloadVoltage` | `voltage` | `3` |
| `cancel_job` | `CancelJob` | `cancel` | `4` |
| `download_beamformer` | `DownloadBeamformer` | `beamformer` | `5` |
| `imaging` | `Imaging` | `imaging` | `6` |
| none | `Unknown` | none | `null` (the server gave no type) |

`--job-types` now refuses text that is not a job type, with an error that
names it. This includes the 2.x names. In 2.x any other text (a misspelt name,
for example) was accepted and matched no job, so the list was empty.

## Waiting and cancelling (`wait`, `cancel`)

**Obsids are refused.** `wait` and `cancel` take job IDs only. In 2.x an
obsid given to them was ignored without a word: `giant-squid cancel 31
1065880128` cancelled job 31 and said nothing about the obsid. In 3.0.0 an
obsid anywhere in the arguments (or in a file) stops the command with an error
that names the obsid, and nothing is sent:

```text
Expected only job IDs, but found these obsids: 1065880128. To find the job IDs of an obsid, use 'giant-squid list <obsid>'.
```

**The log lines of `cancel` changed.** A reply from the MWA ASVO does not
prove that a job was cancelled (it answers the cancellation of a job that is
already cancelled as normal, with a message that says so), so `cancel` no
longer says "Cancelled":

| 2.x | 3.0.0 |
|---|---|
| `Cancelled MWA ASVO job ID 31` | `Cancel request for job 31: <the MWA ASVO's message>` |
| `Cancelled 2 jobs.` | `Cancel requests: 2 sent, 0 failed.` |

If a script looks for "Cancelled", read the message instead, or use `list`
afterwards to check the state of the jobs. A request that the MWA ASVO
refuses with an error is logged as `Failed to cancel MWA ASVO job ID N:
<reason>` and counted in `failed`.

The JSON keys of `wait --json` changed as for `list`: see the next section.

## JSON output (`list --json`, `wait --json`)

The JSON keys changed to the API's names, and there are
more of them:

| 2.x key | 3.0.0 key |
|---|---|
| `obsid` | `obs_id` |
| `jobId` | `id` (the API's name in a job's details) |
| `jobType` | `job_type` |
| `jobState` | `job_state` |
| `files` | `product.files` (the file list is inside `product`) |
| `files[].jobType` (the delivery) | `product.files[].type` |
| `files[].fileUrl` | `product.files[].url` |
| `files[].filePath` | `product.files[].path` |
| `files[].fileSize` | `product.files[].size` |
| `files[].fileHash` | `product.files[].sha1` |
| `completed` | `completed` |

New keys: `created`, `started`, `modified`, `error_code`, `error_text`, `user_id`,
`first_name`, `last_name`, `job_params`, and `format` for each file. The
value of `job_state` is the API's (for example `completed`, see the table in
the previous section). The value of `job_type` is the API's code (for example
`1` for a visibility download). A file's `type` is the API's value
(`acacia`, `scratch` or `dug`; in 2.x `Acacia` and so on). As in the API, a
key that has no value is left out, for a job and for a file (in 2.x it was
`null`): for example `completed` for a job that is not finished. Use `//` in
`jq` for a key that can be missing, as the example below does.

To update a `jq` command, change the key names. The 2.x README example:

```bash
# 2.x
giant-squid list --json --types download_visibilities --states ready \
  | jq -r '.[]|[.jobId,.files[0].fileUrl//"",.files[0].fileSize//"",.files[0].fileHash//""]|@tsv'

# 3.0.0
giant-squid list --json --job-types visibility --job-states completed \
  | jq -r '.[]|[.id,.product.files[0].url//"",.product.files[0].size//"",.product.files[0].sha1//""]|@tsv'
```

For now, `--legacy-json` (on `list` and `wait`, in place of `--json`)
prints the 2.x format exactly, and warns on standard error. It is deprecated
and will be removed in a later release, so use it only while you update your
scripts.

## Downloading (`download`)

No change: `download` takes the same job IDs, obsids and options as in 2.x
(`--download-dir`, `--keep-tar`, `--no-resume`, `--concurrent-downloads`,
`--skip-hash`, `--dry-run`). New: `GIANT_SQUID_DOWNLOAD_RETRY_SECS` sets how
long a failing download is retried (default 900 s).

New: `-j`, `--json` prints the result of each download as one line of JSON
(`job_id`, `obs_id`, `status`, `message`), and `cancel --json` prints the MWA
ASVO's reply to each request, as the submit commands do. A failed download or
cancel request is an error line: see [Errors with `--json`](#errors-with---json).

New: a download without `--keep-tar` (stream untar) resumes. In 2.x, only a
`--keep-tar` download resumed. If a stream-untar download fails, run the same
command again with the same `--download-dir`. giant-squid does not download
again the files that are complete, and continues a partly written file. During
the download, the download directory contains a hidden resume file,
`.<tar file name>.giant-squid-resume.json`. giant-squid deletes it when the
download is complete. Do not delete it while a download is incomplete, if you
want the next run to be fast.

Changed: `--no-resume` downloads a partial file again from the start. In 2.x,
it skipped the partial file and left it as it was. `--skip-hash` skips the SHA-1
check only for a download that runs from start to end in one attempt. A resumed
download is always checked.

Changed: a stream-untar download does not write a tar entry whose path is
absolute, contains `..` or has no name. In 2.x, such an entry could be written
outside the download directory.

## Authentication and environment variables

No change to `MWA_ASVO_API_KEY`, `MWA_ASVO_HOST` (a full URL, as in 2.x),
`MWA_ASVO_API_TIMEOUT`, `GIANT_SQUID_DELIVERY`, `GIANT_SQUID_DELIVERY_FORMAT`
or `GIANT_SQUID_BUF_SIZE`.

New: giant-squid 3.0.0 logs in to the MWA ASVO with your API key and keeps
the session in `~/.mwa-asvo/tokens.json` (the same file as `mwa-cli`), so
that it does not log in again on every command. You can delete the file at
any time; giant-squid logs in again when it needs to. The MWA ASVO allows
only a few logins a minute, so a script that runs giant-squid very often
works best with `HOME` set, so that the file can be used.

## Commands at a glance

| Command | Alias | Change |
|---|---|---|
| `list` | `l` | New option names (old ones still work), new date options, new JSON keys, unknown `--job-types` refused |
| `download` | `d` | None |
| `submit-vis` | `sv` | `--json` added |
| `submit-meta` | `sm` | `--json` added |
| `submit-conv` | `sc` | `-p` replaced by options; **new defaults** |
| `submit-image` | `si` | `-p` replaced by options; obsids only; **new defaults** |
| `submit-image-from-job` | `sifj` | New: images a completed conversion job |
| `submit-volt` | `st` | `--json` added; `--offset` range checked |
| `submit-bf` | `sb` | `--json` added |
| `wait` | `w` | New JSON keys (`--legacy-json` for the old ones); obsids refused |
| `cancel` | `c` | New log lines; obsids refused |
