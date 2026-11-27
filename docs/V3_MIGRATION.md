# Migrating from giant-squid 2.x to 3.0.0

This guide is for people who use the `giant-squid` command line. It lists
what changed from 2.x (the last is 2.5.1) to 3.0.0, and what to do about
each change. Most commands work as before; the changes that can alter your
results or break a script are in the checklist below.

Why the changes: giant-squid 3.0.0 uses version 2 of the MWA ASVO API. The
job options, their names and their defaults now come from that API, so
giant-squid, the API and its documentation use the same names everywhere.

Every command except the file transfer of `download` uses version 2 of the
MWA ASVO API (2.8.3, OpenAPI 3.1.0). A download job (visibility, metadata,
voltage or beamformer) goes to `/api/v2/jobs/download`, a conversion or
imaging job to `/api/v2/jobs/preprocess`, and an image from a conversion job
to `/api/v2/jobs/image_from_job`. `giant-squid --version` prints
`giant-squid 3.0.0` and the versions of the API and of the OpenAPI
specification (for example `MWA ASVO API 2.8.3`, `OpenAPI 3.1.0`). In 2.x it
printed the name of the crate (`mwa_giant_squid`).

The changes to the Rust library and the Python module are in
[V3_LIBRARY.md](V3_LIBRARY.md).

## Quick checklist

1. If you submit **conversion or imaging jobs**, check the new defaults
   (output format, averaging, multiscale, w-layers): see
   [Conversion jobs](#conversion-jobs-submit-preprocess) and
   [Imaging jobs](#imaging-jobs-submit-image-and-submit-image-from-job).
2. If you use **`-p` / `--parameters`**, replace it with one option for each
   key: see [No more `--parameters`](#no-more--p----parameters).
3. If you run **`submit-image` with a Job ID**, use `submit-image-from-job`:
   see [Imaging jobs](#imaging-jobs-submit-image-and-submit-image-from-job).
4. If a script reads **`list --json`** or **`wait --json`**, update it to
   the new key names, or use `--legacy-json` for one release: see
   [JSON output](#json-output-list---json-wait---json).
5. If a script reads **log lines** (for example "Submitted ... as MWA ASVO
   Job ID ..."), read `--json` instead: see
   [Output: logs go to stderr](#output-logs-go-to-stderr).
6. If a script reads the **output of `cancel`**, or gives an **Obs ID to `wait`
   or `cancel`**, change it: see
   [Waiting and cancelling](#waiting-and-cancelling-wait-cancel).
7. If a script relies on `giant-squid list` (or `wait`, or `download`) seeing
   **jobs older than 30 days**, change it: see
   [Listing jobs](#listing-jobs-list).

8. If a script uses **`-f` or `-t` with `submit-volt`**, or **`-n` with `list`
   or `wait`**, change it: see [Short flags](#short-flags).
9. If a script expects the files of **`download`** in the download directory
   itself, change it: see [Downloading](#downloading-download).
10. If a script gives **`submit-volt`** `-d`/`--delivery` or
    `-f`/`--delivery-format`, remove them: see
    [Other submit commands](#other-submit-commands).
11. If a script uses **`submit-image`** or **`submit-image-from-job`**, add
    `-o`/`--output-mode` (it is required now), and change
    **`--clean-threshold`** or **`--nwlayers`** to `--abs-threshold` and
    `--wstack-nwlayers`: see
    [Imaging jobs](#imaging-jobs-submit-image-and-submit-image-from-job).
12. If a script uses **`submit-vis`, `submit-meta`, `submit-volt`, `submit-bf`
    or `submit-conv`**, plan to change it to `submit-download` or
    `submit-preprocess`. The old commands still work, but they are deprecated
    and log a warning: see [Deprecated submit commands](#deprecated-submit-commands).
13. If a script reads the **exit code** of a submit command, `download` or
    `cancel`, check it: they now try every ID and exit with code 1 if any
    failed. See [Messages, errors and exit codes](#messages-errors-and-exit-codes).

Your API key and environment variables need no change.

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
giant-squid submit-preprocess 1065880128 --avg-time-res 0.5 --avg-freq-res 10
```

A value that is out of range is now rejected before anything is sent, with
a message that gives the range (for example
`--mgain: must be between 0.1 and 1 (got 1.5)`). Run
`giant-squid submit-preprocess --help` (or any other command) to see every option
and its default.

Keys whose names changed:

| 2.x key | 3.0.0 option | Note |
|---|---|---|
| `phase_centre_ra` | `--custom-centre-ra` | `--phase-centre-ra` still works |
| `phase_centre_dec` | `--custom-centre-dec` | `--phase-centre-dec` still works |
| `centre` (imaging) | `--centre` | `--phase-center` still works |

A flag that was `key=true` in 2.x is an option with no value in 3.0.0 (for
example `no_rfi=true` is `--no-rfi`). The imaging options that default to
true (`--apply-di-cal`, `--apply-primary-beam`, `--join-channels`) take an
optional value, so you can turn them off with `--apply-primary-beam=false`.
`--apply-di-cal` of `submit-preprocess` takes the same optional value (its default is
false). The aliases `--phase-centre-ra`, `--phase-centre-dec`, `--custom-ra`,
`--custom-dec` and `--phase-center` work on both `submit-preprocess` and
`submit-image`.

### Option values and help

An option for a field that the MWA ASVO allows to be `null` takes the word
`null` (`--avg-freq-res null`, `--no-rfi=null`, `--allow-resubmit=null`):
`--avg-freq-res`, `--avg-time-res`, `--flag-edge-width`, `--abs-threshold`,
`--no-digital-gains`, `--no-flag-dc`, `--no-geometric-delay`,
`--no-passband-gains`, `--no-cable-delay`, `--no-rfi` and
`-r`/`--allow-resubmit`. The last seven also take `true` and `false`, and the
bare flag (`--no-rfi`, `-r`) is still `true`.

An option that takes a named value (`--delivery`, `--centre`,
`--polarization`, `--job-states`, `--job-types` and so on) reads it in the
same way: the case, spaces, hyphens and underscores do not matter (for
example `--delivery ACACIA`, `--job-states WAIT-CAL`). The help
(`[possible values: ...]`) and the message for a bad value list the values in
the schema's order.

A negative number now works in the `--option value` form, for example
`submit-volt --offset -1` (it is then refused with its range). In 2.x, clap
read `-1` as an unknown flag (`unexpected argument '-1'`), and only the
`--option=value` form worked.

The help text of each job option is the MWA ASVO API's (the schema's
`x-cmd-help`). An option whose value has limits shows them after the text,
for example `[range: 0 to 7200]`, and `--help` shows the default of each
option. `--help` lists the allowed values of `--delivery`,
`--delivery-format`, `--output`, `--centre`, `--output-mode`,
`--polarization` and `--weighting`. The help of `list --job-states` and
`--job-types` lists the names that the options take (in 2.x it listed
`retrieving`, which is not a state). The help and clap's messages are in
colour on a terminal (as cargo's are), not only bold. `NO_COLOR` turns the
colours off.

giant-squid checks a job only against the limits of the API schema (numbers,
image sizes). The MWA ASVO checks the rules that are not in the schema, and
refuses a job that breaks one with a validation error: `delivery_format`
`files` with `delivery` `acacia`; `custom_centre_ra` and `custom_centre_dec`
with a `centre` other than `custom`; and `only_metadata` (true) with a
voltage option.

## Conversion jobs (`submit-preprocess`)

A conversion job is now submitted with `submit-preprocess` (short name
`sp`). It has the same options as `submit-conv`, which still works but is
deprecated (see [Deprecated submit commands](#deprecated-submit-commands)).

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
giant-squid submit-preprocess 1065880128 --output uvfits --avg-freq-res 80
```

For no time averaging, give `--avg-time-res null` (as for "no frequency
averaging", `--avg-freq-res null`).

The other conversion options (`--apply-di-cal`, `--centre`, `--no-rfi`,
`--no-cable-delay`, `--no-digital-gains`, `--no-flag-dc`,
`--no-geometric-delay`, `--no-passband-gains`) do what the 2.x keys did.

## Imaging jobs (`submit-image` and `submit-image-from-job`)

In 2.x, `submit-image` took Obs IDs **or** the Job IDs of completed
conversion jobs. In 3.0.0 these are two commands:

```bash
# 2.x
giant-squid submit-image 1065880128 --parameters=image_size=2048,multiscale=true
giant-squid submit-image 12345 --parameters=image_size=2048,multiscale=true

# 3.0.0
giant-squid submit-image 1065880128 -o fits --image-size 2048 --multiscale
giant-squid submit-image-from-job --source-job-id 12345 -o fits --image-size 2048 --multiscale
```

`submit-image-from-job` (alias `sifj`) needs only the conversion job's Job ID,
as `--source-job-id`: it takes no Obs ID, because the MWA ASVO takes the Obs ID
from the conversion job. As in 2.x, the conversion job must have made a CASA
measurement set, delivered to Acacia or Scratch.

The imaging defaults are now the API's. Some are as in 2.x (`briggs`
weighting with robust -0.5, auto threshold 0.5, clean threshold 0.001,
primary beam applied), but these changed:

| Option | 2.x default | 3.0.0 default |
|---|---|---|
| Output mode | `fits` | none: `-o`/`--output-mode` is required (`fits`, `all_fits` or `all_files`) |
| Image size | 3072 | not set, so the MWA ASVO calculates it: give `--image-size 3072` for the 2.x value |
| Pixel scale | 20 | not set, so the MWA ASVO calculates it: give `--pixel-scale 20` for the 2.x value |
| Multiscale cleaning | on | off: give `--multiscale` to turn it on |
| w-layers | 128 | not set, so the MWA ASVO decides: give `--wstack-nwlayers 128` for the 2.x value |

The MWA ASVO API no longer has `clean_threshold` and `nwlayers`, so
`--clean-threshold` and `--nwlayers` are gone (also as keys of a
`submit-csv` imaging line). Use `--abs-threshold` and `--wstack-nwlayers`.

So to image as 2.x did by default:

```bash
# 3.0.0, the same imaging options as 2.x's defaults
giant-squid submit-image 1065880128 -o fits --image-size 3072 --pixel-scale 20 \
    --multiscale --wstack-nwlayers 128
```

For an Obs ID, the conversion part of an imaging job also has the new
conversion defaults (see above). Run `giant-squid submit-image --help` to
see every default. `--polarization` (`xx`, `yy`, `xy`, `yx`, `xxyy`, `i`
or `iquv`, default `i`) is new. In a `submit-csv` imaging line, give
`output_mode`.

## Other submit commands

**`submit-download` replaces `submit-vis`, `submit-meta`, `submit-volt` and
`submit-bf`.** The MWA ASVO API has one endpoint for every download job, and
it finds the kind of data from the observation, so one command submits them
all: see the table in [Deprecated submit commands](#deprecated-submit-commands).

The voltage options (`--offset`, `--duration`, `--from-channel`,
`--to-channel`) are for a voltage observation: the MWA ASVO refuses them for
an observation that has no voltages. `--offset` and `--duration` must be from
0 to 7200 seconds, and a
negative value (`--offset -1`) is rejected with that range. Without
`--only-metadata`, the request has no `only_metadata` key (the API gives it
no default).

The default delivery format is now `tar` for every job, whatever the
delivery: in 2.x a Scratch or DUG delivery gave individual files unless you
asked for `tar`. Add `--delivery-format files` to get the individual files.

**`submit-volt` has no delivery options.** `-d`/`--delivery` and
`-f`/`--delivery-format` are gone from `submit-volt` (clap refuses them), and
`GIANT_SQUID_DELIVERY` and `GIANT_SQUID_DELIVERY_FORMAT` do not apply to it:
it sends the MWA ASVO's default delivery (`acacia`, `tar`), and the MWA ASVO
decides where a voltage job is delivered. `submit-download` takes the
delivery options for a voltage job too. A voltage line of a manta-ray-client
CSV file gives `delivery` and `delivery_format` as the other download lines
do, and the defaults of the file apply to it.
A beamformer job has no `mode`.

**New: `submit-csv`** (short name `scsv`) submits the jobs of a
[manta-ray-client](https://github.com/MWATelescope/manta-ray-client) CSV file
(`mwa_client -c file.csv`): conversion, download (visibility and metadata),
beamformer, voltage and imaging jobs, one per line. It reads and checks the
whole file before it submits anything, and a submission that fails does not
stop the others. A key that the job type does not have is an error (also the
beamformer `mode`). A `channel_range` is checked but not sent. A key that a
line gives twice, and a line that makes the same job as an earlier line, are
warnings. A line that gives no delivery or delivery format uses
`GIANT_SQUID_DELIVERY` and `GIANT_SQUID_DELIVERY_FORMAT`, if they are set,
and else the schema default. A voltage line is no different. A file with no
job is an error. See the
[README](../README.md#jobs-from-a-manta-ray-client-csv-file).

### Deprecated submit commands

The submit commands of 2.x still work in 3.0.0, with the same options and
short names, but they will be removed in a later release. `giant-squid
--help` does not list them with the other commands (the end of the help
lists them), and each one logs a warning on standard error when it runs, for
example:

```text
[WARN] submit-vis is deprecated and will be removed in a later release. Use submit-download.
```

With `--json` there is no warning, because `--json` logs nothing.

| Deprecated command | Short name | Use instead |
|---|---|---|
| `submit-vis` | `sv` | `submit-download` |
| `submit-meta` | `sm` | `submit-download --only-metadata` |
| `submit-volt` | `st` | `submit-download` with `--offset` and `--duration` |
| `submit-bf` | `sb` | `submit-download` |
| `submit-conv` | `sc` | `submit-preprocess` |

Every submit command has a new `--json` option, which prints the MWA ASVO's
reply for each job as one line of JSON, for example
`{"status":"success","job_id":12345,"message":"..."}`. Use it in scripts;
see the next section. The `status` text and the `message` describe the reply; the exit code of
`giant-squid` and the HTTP status of the MWA ASVO tell you whether a submission worked.

`--dry-run` now prints the request body that would be sent, as JSON, for
each Obs ID.

## Output: logs go to stderr

In 2.x, giant-squid wrote its log lines (information and
warnings) to standard output. In 3.0.0 all log lines go to standard error,
so that standard output has only the command's output: the job table,
`--json` output, and the `--json` replies of the submit commands. The
`--dry-run` reports are log lines, so they are on standard error too.

If a script read a log line to get a Job ID, change it to read `--json`:

```bash
# 2.x: read the job ID from a log line
giant-squid submit-vis 1065880128 | grep "as MWA ASVO job ID"

# 3.0.0: read it from the JSON reply
giant-squid submit-download --json 1065880128 | jq -r .job_id
```

To keep the log lines in a file, redirect standard error:
`giant-squid list 2> giant-squid.log`.

`-v` shows all the API requests and responses, and `-vv` shows their full
payload. The logs show no credential: see
[Authentication and environment variables](#authentication-and-environment-variables).

### Messages, errors and exit codes

Messages, logs, `--help` and the docs write "Job ID" and "Obs ID" (2.x wrote
"job ID", "jobid", "obsid" and "Obsid"). A script that searches the log
lines for these words must change. JSON keys and argument names do not
change (`job_id`, `obs_id`).

An error that the MWA ASVO sends is shown as it sent it: its error code,
message, `detail`, `suggestion`, field errors and request ID (also for a
failed login). A failed job shows the MWA ASVO's `error_code` as well as its
error text.

The submit commands try every Obs ID, and log the error of each failed
submission when it happens. If any failed, the command ends with
`Error: M of N job submissions failed` and exit code 1. In 2.x it stopped at
the first failure. `download` ends with `Error: M of N downloads failed` and
exit code 1. In 2.x it exited with 0. For `cancel`, see
[Waiting and cancelling](#waiting-and-cancelling-wait-cancel).

A command whose standard output is a pipe that closes early (for example
`giant-squid list | head -1`) ends without an error. In 2.x it panicked with
`failed printing to stdout: Broken pipe` and exited with 101.

### Errors with `--json`

With `--json`, a command prints only JSON on standard output, and nothing on
standard error: no log lines and no progress bars. Each error is one line of
JSON in the form of the MWA ASVO's error response (`error_code`, `message`, and
`detail`, `suggestion`, `field_errors`, `request_id` when known), with the
`obs_id` or `job_id` it is about. A line with an `error_code` key is an error:

```bash
giant-squid submit-download --json 1065880128 | jq -r 'select(.error_code) | .message'
```

An error from the MWA ASVO is printed as it was sent. Other errors have a
giant-squid code, for example `HTTP_503`, `NETWORK_ERROR`,
`INVALID_ARGUMENT`, `INVALID_ENVIRONMENT` or `JOB_EXPIRED`. A bad command
line with `--json` is one such line too.

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
`--sort-by` (`id`, `created` or `completed`). `--days` takes 1 to 30, the
limits of the MWA ASVO API; other values are refused before any request.
`--days` cannot be used with `--date-from` or `--date-to`. When no job
matches, the MWA ASVO's `JOB_NOT_FOUND` reply is an empty list.

New: `--fields a,b,c` adds a column to the job table for each named field of
the job or of its parameters (for example `avg_time_res` or `error_text`).
`--help` lists the fields. A job that does not have a field has an empty
cell. The parameters of a listed job are read as the schema type of its job
type (conversion, imaging, ...), so a key that the server did not send shows
the schema default. New: `--ids` prints one Job ID on each line, for the
input of other commands (see [Job IDs and Obs IDs](#job-ids-and-obs-ids)).

The table shows the completed date and time. Its layout and colours are as
in 2.x. With `NO_COLOR` set (and not empty), `list` and `wait` print the
table without colours, as `--no-colour` does. The table of `list` and `wait`
shows a completed job as `completed (ready)` or `completed (expired)`: see
[Expired download links](#expired-download-links).

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
| `expired` | removed: the MWA ASVO has no such state. A completed job whose download links have expired shows as `completed (expired)` in the table (see the README, "Expired download links") |
| `error` (`{"Error": "<message>"}` in the JSON) | `error`; the message is in `error_text` |

The other states have the same names, in lower case in the JSON (for
example `queued`). `preparing` and `imaging` are new. The names are still
case insensitive. `giant-squid list --help` lists them.

**The job type names are the API's.** `--job-types` takes the names of the
MWA ASVO API's `JobType`, the table shows them, and so does the JSON:

| 2.x name | 2.x JSON | 3.0.0 name and JSON |
|---|---|---|
| `conversion` | `Conversion` | `conversion` |
| `download_visibilities` | `DownloadVisibilities` | `visibility` |
| `download_metadata` | `DownloadMetadata` | `metadata` |
| `download_voltages` | `DownloadVoltage` | `voltage` |
| `cancel_job` | `CancelJob` | `cancel` |
| `download_beamformer` | `DownloadBeamformer` | `beamformer` |
| `imaging` | `Imaging` | `imaging` |
| none | `Unknown` | removed: every job has a type |

`--job-types` now refuses text that is not a job type, with an error that
names it. This includes the 2.x names. In 2.x any other text (a misspelt name,
for example) was accepted and matched no job, so the list was empty. A job
type that the schema does not list makes the listing fail.

## Waiting and cancelling (`wait`, `cancel`)

**Obs IDs are refused.** `wait` and `cancel` take Job IDs only. In 2.x an
Obs ID given to them was ignored without a word: `giant-squid cancel 31
1065880128` cancelled job 31 and said nothing about the Obs ID. In 3.0.0 an
Obs ID anywhere in the arguments (or in a file) stops the command with an error
that names the Obs ID, and nothing is sent:

```text
Expected only Job IDs, but found these Obs IDs: 1065880128. To find the Job IDs of an Obs ID, use 'giant-squid list <OBS_ID>'.
```

**The log lines of `cancel` changed.** A reply from the MWA ASVO does not
prove that a job was cancelled (it answers the cancellation of a job that is
already cancelled as normal, with a message that says so), so `cancel` no
longer says "Cancelled":

| 2.x | 3.0.0 |
|---|---|
| `Cancelled MWA ASVO job ID 31` | `Cancel request for Job ID 31: <the MWA ASVO's message>` |
| `Cancelled 2 jobs.` | `Cancel requests: 2 sent, 0 failed.` |

If a script looks for "Cancelled", read the message instead, or use `list`
afterwards to check the state of the jobs. A request that the MWA ASVO
refuses with an error is logged as `Failed to cancel MWA ASVO Job ID N:
<reason>` and counted in `failed`.

**The exit code of `cancel` changed.** In 2.x, a request that the MWA ASVO
refused was skipped without a word, and the exit code was 0. In 3.0.0,
`cancel` still sends every request, but the exit code is 1 if the MWA ASVO
refused any of them with an HTTP error. The cancellation of a job that is
already cancelled is not a failure (exit code 0), because the MWA ASVO
answers it as normal.

**`wait` tries again after a failed check.** In 2.x, one failed request for the
job list ended a long `wait`. In 3.0.0, a network error, or a reply with an HTTP
status of 429 or 5xx, is logged as a warning and the check is tried again after
a short pause that grows. `wait` gives up after 15 minutes of such failures
without one success. Any other failure ends `wait` at once.

The JSON keys of `wait --json` changed as for `list`: see the next section.

## JSON output (`list --json`, `wait --json`)

The JSON keys changed to the API's names, and there are
more of them:

| 2.x key | 3.0.0 key |
|---|---|
| `obsid` | `obs_id` |
| `jobId` | `job_id` (the API's name in a job's details) |
| `jobType` | `job_type` |
| `jobState` | `job_state` |
| `files` | `product.files` (the file list is inside `product`) |
| `files[].jobType` (the delivery) | `product.files[].type` |
| `files[].fileUrl` | `product.files[].url` |
| `files[].filePath` | `product.files[].path` |
| `files[].fileSize` | `product.files[].size` |
| `files[].fileHash` | `product.files[].sha1` |

New keys: `completed`, `created`, `started`, `modified`, `error_code`, `error_text`, `user_id`,
`first_name`, `last_name`, `job_params`, and `format` for each file. The
value of `job_state` is the API's (for example `completed`, see the table in
the previous section). The value of `job_type` is the API's name (for example
`visibility` for a visibility download). A file's `type` is the API's value
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
  | jq -r '.[]|[.job_id,.product.files[0].url//"",.product.files[0].size//"",.product.files[0].sha1//""]|@tsv'
```

For now, `--legacy-json` (on `list` and `wait`, in place of `--json`)
prints the 2.x format exactly, and warns on standard error. It is deprecated
and will be removed in a later release, so use it only while you update your
scripts.

## Downloading (`download`)

`download` takes the same Job IDs, Obs IDs and options as in 2.x
(`--download-dir`, `--keep-tar`, `--no-resume`, `--concurrent-downloads`,
`--skip-hash`, `--dry-run`). New: `GIANT_SQUID_DOWNLOAD_RETRY_SECS` sets how
long a failing download is retried (default 900 s), from the first failure.
An attempt that downloads some bytes before it fails starts that time again,
and an attempt that ends in a hash mismatch does not. `download` now retries
a network error during the transfer too (2.x retried only a failed request),
with or without `--keep-tar`.

Changed: each job is downloaded into a directory of its own below
`--download-dir`, named for the Job ID. `giant-squid download -d /data 12345`
writes to `/data/12345`, and not to `/data`, so the files of two jobs of the
same observation do not meet. To get the layout of 2.x, add `--no-job-subdir`.
A script that reads the files must look in `<download directory>/<job_id>`. A
partial download that 2.x left in the download directory is not found there:
use `--no-job-subdir` to carry on with it. A Scratch job is not affected.

New: `-j`, `--json` prints the result of each download as one line of JSON
(`status`, `job_id`, `obs_id`, `message`), and `cancel --json` prints the
MWA ASVO's reply to each request, as the submit commands do. A failed download or
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
it skipped the partial file and left it as it was. A complete `--keep-tar`
file that matches the SHA-1 is still not downloaded again, and a retry in the
same run still continues its own partial file. `--skip-hash` skips the SHA-1
check only for a download that runs from start to end in one attempt. A resumed
download is always checked, with or without `--keep-tar`: after a failed
attempt in the same run, or from the files of an earlier run. If the check
fails, the whole archive is downloaded again. With `--keep-tar`, a complete
tar file that is already on disk is checked, as in 2.x. The help of
`--skip-hash` says this.

Changed: `download` of a Scratch job that this host can reach moves its files
into the download directory (`-d`, `--download-dir`), as for an Acacia job.
In 2.x, they went to the current directory. The move does not replace
anything: if something is already at the target, the download fails
(`DESTINATION_EXISTS`; `SymlinkInDownloadDir` for a symbolic link) and
nothing changes. A directory at the target, when the Scratch folder is gone,
is the result of an earlier download, and a second download succeeds. If the
download directory is where the files already are, nothing is moved. A move
to another file system is a copy, then a removal (in 2.x it failed). The copy
is made next to the job folder in the download directory, and renamed to it
when it is complete, so a copy that stops leaves nothing in the way of the
next run.

Changed: `download` of a DUG job, or of a Scratch job whose path this host
cannot reach, fails (`FILES_NOT_REACHABLE`) with a non-zero exit code. In
2.x, it logged an error and reported success.

Changed: `download` fails when its files of IDs hold no Job ID or Obs ID
(2.x did nothing and exited with 0), and when `-d`/`--download-dir` is not a
directory (2.x accepted it).

### Expired download links

The download links of a job work for 7 days. The job stays `completed` in
the API. `giant-squid` reads the end of that time from the `Expires` of the
signed URL. The table of `list` and `wait` shows a completed job as
`completed (ready)` or `completed (expired)`. The JSON and `--job-states`
keep the API's `completed`.

`download` of a job whose links have expired, or that the file server
answers with `AccessDenied` (HTTP 403), fails with the error `JOB_EXPIRED`:
submit the job again. The command skips such a job with a warning, and does
not count it as a failure. `download` of an Obs ID skips a completed job
whose links have expired when another one works. `wait` fails for an expired
job, as for a cancelled job. See the
[README](../README.md#expired-download-links).

### Fixes to `download`

- An ID that is given more than once is used once, at its first place. In
  2.x, `cancel 5 5` sent two requests. `download` of a Job ID and of the Obs
  ID of the same job downloads the job once, and logs that it does. In 2.x the
  job could be downloaded twice at the same time into the same directory,
  which could corrupt its files.
- `download` starts several downloads at once in the order of the IDs, so the
  numbers in the log (`[1/20]`, `[2/20]`, ...) are in order. In 2.x each
  thread took a block of the list, and the log said `[1/20]`, `[11/20]`,
  `[6/20]`. Job IDs and Obs IDs together are numbered from 1 to the total
  (`[3/4]`); in 2.x the Obs ID downloads started again from 1.
- `download` logs in and gets the job list once for all its downloads. In
  2.x, each download logged in on its own (a long list could hit the
  server's limit on logins) and got the job list again.
- `download` no longer panics on a job file with no SHA-1 hash (`NO_HASH`), or
  on a download URL or Scratch path that names no file. The time and speed in
  "Completed download of ..." are for that file, not for the job so far.
- `download --keep-tar` of a complete tar file that cannot be read for the
  hash check fails with the read error. In 2.x, the error counted as a hash
  mismatch, and the file was emptied and downloaded again.
- `download --keep-tar` resume: 2.x sent the range request with a wrong
  header, so the server sent the whole file and giant-squid appended it to
  the partial file. A complete file was downloaded again, and the SHA-1 of a
  resumed download covered only the new bytes. All three are fixed. If a
  server ignores the range request, giant-squid downloads the file again from
  the start.

### Safety of the download directory

A stream-untar download (without `--keep-tar`) does not write a tar entry
whose path is absolute (for example `/home/user/.bashrc`), contains `..` (for
example `../../.ssh/authorized_keys`) or has no name. It shows a warning for
each such entry and unpacks the others. In 2.x, such an entry was written
where its path pointed, so a bad archive could create or overwrite a file
anywhere that the user can write. It also unpacks only files and
directories: a symbolic link, a hard link or a special file in the archive is
skipped with a warning. In 2.x it was unpacked, so a link could point outside
the download directory. The SHA-1 check still covers the whole archive.

`download` does not write through a symbolic link that is already in the
download directory. A stream-untar entry whose file, or a directory on the
way to it, is a symbolic link fails the download with `SymlinkInDownloadDir`
(the entries before it are already written). A `--keep-tar` download whose
tar file is a symbolic link (even one that points nowhere) fails in the same
way, and nothing is fetched. The download directory itself may be a symbolic
link. On Unix, the refusal is part of the call that opens the file
(`O_NOFOLLOW`) and of the calls that walk down the directories of a tar
entry (`openat`), so a link that another process puts in the place after the
check is refused too. In 2.x the download wrote wherever the link pointed.
See the [README](../README.md#symbolic-links-in-the-download-directory).

A Scratch job is moved only when its path is a folder named for its Job ID,
as the MWA ASVO makes it. Any other path fails with `INVALID_JOB` and is not
moved, so that a bad reply cannot move another folder. A Scratch job folder
with a link that points outside the folder is moved as before (the link is
moved as a link), with a warning for each such link.

## Authentication and environment variables

No change to `MWA_ASVO_API_KEY`, `MWA_ASVO_HOST` (a full URL, as in 2.x),
`GIANT_SQUID_DELIVERY`, `GIANT_SQUID_DELIVERY_FORMAT` or
`GIANT_SQUID_BUF_SIZE`. `MWA_ASVO_API_TIMEOUT` is the same, except that a
value that is not a whole number of seconds is now an error (2.x warned and
used the default), as it already was for `GIANT_SQUID_BUF_SIZE`. A value of
`0` is also an error, for both variables. A value of the new
`GIANT_SQUID_DOWNLOAD_RETRY_SECS` that is not a whole number is an error too.
With `--json`, these errors are `INVALID_ENVIRONMENT`.

New: giant-squid 3.0.0 logs in to the MWA ASVO's v2 (JWT-based) login API
with your API key, instead of HTTP Basic Auth. It keeps the session in
`~/.mwa-asvo/tokens.json` (the same place as `mwa-cli`), so that it does not
log in again on every command, and refreshes the session when it expires. The file also holds the host
and a hash of the API key (never the key itself), because a session is used
only for the host and the API key that it was made for. A file without them,
such as one written by `mwa-cli`, is not used: giant-squid logs in again and
replaces it. You can delete the file at any time; giant-squid logs in again
when it needs to. The MWA ASVO allows
only a few logins a minute, so a script that runs giant-squid very often
works best with `HOME` set, so that the file can be used.

The logs show no credential, also at trace level (`-vv`). The login and
refresh requests and replies, which hold the API key and the tokens, are not
logged. The other request and reply bodies are logged with the URLs in them
redacted, and so is the URL of a file of a job (a signed Acacia URL):
giant-squid shows the host, the path, the names of the query parameters and
the first four characters of a long value. A URL whose scheme is in capitals
(`HTTPS://`) is redacted too. An error of a file download does not show the
signed URL either.

The login, refresh and API requests follow a redirect only to the same host
and port, so that a 307 or 308 redirect does not send the API key to another
host. The download of a file still follows every redirect. If `MWA_ASVO_HOST`
is an `http://` URL, giant-squid warns that the API key and the session are
sent in clear text.

## Job IDs and Obs IDs

The smallest Obs ID is the API's minimum, 1056643216. A number with 10 digits
is still read as an Obs ID, so a 10-digit number below the minimum is an
error, not a Job ID. `wait` and `cancel` take Job IDs only: see
[Waiting and cancelling](#waiting-and-cancelling-wait-cancel).

New: `download`, `cancel` and `wait` read their IDs from standard input when
an argument is `-`, and only then. The input is the output of the new option
`list --ids` (one Job ID on each line) or of `list --json`, or IDs that
spaces or lines separate. The commands do not filter what they read, and an
empty list is not an error. For example:

```bash
giant-squid list --job-states completed --ids | giant-squid download -
```

See the [README](../README.md#use-the-output-of-list-as-the-input-of-other-commands).

`download`, `cancel` and `wait` use an ID that is given more than once a
single time: see [Fixes to `download`](#fixes-to-download).

A file of IDs that starts with a UTF-8 byte order mark (as Notepad and
PowerShell can save it) is read. In 2.x its first ID was refused as text that
is not a number. Standard input and CSV files accept the mark too.

## Short flags

Three short flags are gone, so that a letter means the same thing in every
command where it is used for these two options:

| 2.x | 3.0.0 |
|---|---|
| `submit-volt -f N` | `submit-download --from-channel N` |
| `submit-volt -t N` | `submit-download --to-channel N` |
| `list -n`, `wait -n` | `list --no-colour`, `wait --no-colour` |

`-f` is now only `--delivery-format`, and `-n` is now only `--dry-run`. The
other short flags did not change. `-d`, `-o` and `-r` still mean more than
one thing, as in 2.x, so use the long names in scripts.

## Docker

The Docker image is built from a Docker hardened image, in a two-stage build.
