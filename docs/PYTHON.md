# mwa-giant-squid: giant-squid for Python

`mwa-giant-squid` is a Python module for the [MWA ASVO](https://asvo.mwatelescope.org/). It is the
[giant-squid](https://github.com/MWATelescope/giant-squid) Rust library with Python bindings. You
can use it to list, submit, cancel and download MWA ASVO jobs from your own Python code.

The package also installs the `giant-squid` command. It is the Rust `giant-squid` program, run inside the
module (see [The giant-squid command](#the-giant-squid-command)).

- Install name: `mwa-giant-squid`. Import name: `mwa_giant_squid`.
- Python 3.10 or later.
- The package has no Python dependencies.
- The module has type information (`.pyi` stubs and `py.typed`).

The module has the same API as the Rust library, with the same names.

The API reference (every class, method and argument, with types) is at
<https://mwatelescope.github.io/giant-squid/>.

## Install

```bash
pip install mwa-giant-squid
```

`pip install` puts the `giant-squid` command on your `PATH` (in a virtual environment, in its `bin`
directory). If you also have the Rust `giant-squid`, the one that is first on `PATH` runs. Both read the
same environment variables and use the same session cache, so you can use either.

To build from source, you need Rust and [uv](https://docs.astral.sh/uv/):

```bash
git clone https://github.com/MWATelescope/giant-squid.git
cd giant-squid
uv sync
uv run giant-squid --version
```

## The giant-squid command

```bash
export MWA_ASVO_API_KEY="your-api-key-here"
giant-squid list
giant-squid submit-vis 1065880128
giant-squid wait 12345
giant-squid download 12345
giant-squid list --job-states completed --ids | giant-squid download  # IDs on standard input
```

The command is the Rust `giant-squid` program itself, not a copy of it. The module has the program's code
in it, and the `giant-squid` script that the install makes runs that code. So the sub-commands, short names,
options, defaults, environment variables, help, messages, colours and exit codes are those of the Rust
program. For what each one does, see the
[README](https://github.com/MWATelescope/giant-squid/blob/main/README.md) or run `giant-squid --help` and
`giant-squid <command> --help`. The commands are `list` (`l`), `download` (`d`), `submit-vis` (`sv`),
`submit-conv` (`sc`), `submit-image` (`si`), `submit-image-from-job` (`sifj`), `submit-meta` (`sm`),
`submit-volt` (`st`), `submit-bf` (`sb`), `submit-csv` (`scsv`), `wait` (`w`) and `cancel` (`c`). `download`,
`cancel` and `wait` read their IDs from standard input when you give them none (see the
[README](https://github.com/MWATelescope/giant-squid/blob/main/README.md#use-the-output-of-list-as-the-input-of-other-commands)).

The environment variables are `MWA_ASVO_API_KEY` (required), `MWA_ASVO_HOST`, `MWA_ASVO_API_TIMEOUT`,
`GIANT_SQUID_DELIVERY`, `GIANT_SQUID_DELIVERY_FORMAT`, `GIANT_SQUID_BUF_SIZE` and
`GIANT_SQUID_DOWNLOAD_RETRY_SECS`. The session is cached in `$HOME/.mwa-asvo/tokens.json`.

You can also run it with `python -m mwa_giant_squid_cli`.

What is particular to running the program from Python:

- Ctrl-C ends the process at once, as it ends the Rust program. The program puts the default action of
  Ctrl-C back when it starts, because Python's handler could not run while the Rust code runs.
- The output goes to the real standard output and standard error of the process, not to `sys.stdout` and
  `sys.stderr`. A program that runs the command in a thread of its own cannot capture it with Python's
  redirects (use a subprocess).
- The log lines are the Rust program's own, on standard error. They are not Python `logging` records.
- The function `mwa_giant_squid._run_cli` and the package `mwa_giant_squid_cli` are not part of the API of
  the module. Do not depend on them. If you want to write your own client in Python, use the module (the rest
  of this guide).

## Authentication

You need an MWA ASVO API key. To get one, log in to the
[MWA ASVO portal](https://asvo.mwatelescope.org/) and copy the key from your profile.

`AsvoClient(config)` logs in when it is created. The `AsvoClientConfig` is the same as the library's: it holds the
host, your API key, the API timeout and the token cache. `AsvoClientConfig` itself reads no environment variable.
To read them as the `giant-squid` command does, use `client_config_from_env()`.

```python
import mwa_giant_squid as gs

# Your program gives the key ...
client = gs.AsvoClient(gs.AsvoClientConfig("https://asvo.mwatelescope.org:443", api_key))

# ... or the environment does: MWA_ASVO_API_KEY (required), MWA_ASVO_HOST, MWA_ASVO_API_TIMEOUT and HOME.
client = gs.AsvoClient(gs.client_config_from_env())
```

`AsvoClientConfig(host, api_key, *, api_timeout=None, token_cache_path=None)` has these values. They are also
attributes, which you can change before you make the client.

| Argument | Meaning |
|---|---|
| `host` | The MWA ASVO URL. Use `http://` only for a local test server. All other hosts must use TLS. |
| `api_key` | Your API key. The `repr` of the config does not show it. |
| `api_timeout` | The time limit for one API request, in seconds (more than 0). `None` uses `DEFAULT_API_TIMEOUT`. |
| `token_cache_path` | A file in which to keep the session between runs. `None` keeps the session in memory only. A session is used only by a client with the same `host` and `api_key` as the one that made it; the file holds a hash of the key, never the key. |

The MWA ASVO allows only a few logins each minute. A program that runs often must set
`token_cache_path`. The `giant-squid` command uses `$HOME/.mwa-asvo/tokens.json`
(`default_token_cache_path(home)`). You can give the same path to share its session (for the same host and API key):

```python
from pathlib import Path

config = gs.AsvoClientConfig(host, api_key, token_cache_path=gs.default_token_cache_path(Path.home()))
client = gs.AsvoClient(config)
```

To use a different MWA ASVO server (for example, to test a new feature), give its URL as `host`.

## List jobs

`get_jobs` gets your jobs, as `giant-squid list` does. It takes a `JobsFilter`, which has lists of Job IDs, Obs IDs,
job types and job states, `days` or `date_from` and `date_to`, and `sort_by`. The server does what it can (one
state, one type, the days or the dates, the sort order), and the module
filters the rest. Every filter that is `None` does not filter, except `days` and `sort_by`: with `days=None` you
get the jobs from the MWA ASVO's default window (the schema's default for `days`, which is also the most `days`
can be), not your full history. The jobs are selected by `days` or by `date_from` and `date_to`, not by both:
`days` with a date raises `ValueError`, and with a date and no `days` the request has `"days": null`. A job older
than that is not returned, so `download_job` and `AsvoJobVec.all_ready` cannot find it either.

```python
from mwa_giant_squid import JobsFilter, JobState, JobType

# The jobs of the past 7 days (days: 1 to 30) that are completed.
jobs = client.get_jobs(JobsFilter(days=7, job_states=[JobState.Completed]))
for job in jobs:
    print(job.job_id, job.obs_id, job.job_type, job.job_state, job.created)

# Several Obs IDs, job types and job states.
jobs = client.get_jobs(
    JobsFilter(
        obs_ids=[1065880128, 1065880248],
        job_types=[JobType.Visibility],
        job_states=[JobState.Completed, JobState.Error],
    )
)
```

`get_jobs()` with no filter gets the jobs of the default window. `job_ids` and `obs_ids` cannot be given together:
that raises `ValueError`, as `JobsFilter.validate()` does. Each value of a `JobsFilter` is checked when you set it,
as the Rust type is: an Obs ID that is not valid raises `ObsIdError`, a Job ID of 0 or `days` outside 1 to 30
raises `ValueError`.

### Obs IDs

An Obs ID argument takes an `int` or an `ObsId`. `ObsId` is the library's checked Obs ID (10 digits). An `int` that
is not a valid Obs ID raises `ObsIdError`. `AsvoJob.obs_id`, `CsvJobFailure.obs_id` and the `obs_id` of an error are
`ObsId`. An `ObsId` is not an `int`, but it acts as one where a number is needed: it is equal to the `int` of the
same value, has its hash, is ordered with it, and has `__int__` and `__index__`. `json.dumps` does not know it:
use `int(obs_id)`.

```python
from mwa_giant_squid import ObsId, ObsIdError

obs_id = ObsId(1065880128)  # or ObsId.from_str("1065880128")
assert obs_id == 1065880128
print(int(obs_id), ObsId.MIN, ObsId.MAX)

try:
    ObsId(123)
except ObsIdError as e:
    print(e.kind, e.value)  # WrongNumDigits 123
```

The functions that read IDs on the command line are also in the module: `parse_obs_ids_only`,
`parse_job_ids_only`, `parse_many_job_ids_or_obs_ids` and `parse_job_ids_and_obs_ids_from_file` (each argument is an
ID or the name of a file of IDs), and `parse_utc_time` (RFC 3339 or a date, as a `datetime` in UTC). A problem is a
`ParseError`.

The result is an `AsvoJobVec`. It supports `len()`, indexing and iteration. Its `filter` method
filters a list that you already have, without a request. Its `json()` method gives the jobs as JSON, an object
keyed by Job ID, as `giant-squid list --json` prints.

`AsvoJob.detail()` is the job as the API gives it, a `JobDetailResponse` that has the attributes of the job. Its
`obs_id` is the `int` that the API gave, not a checked `ObsId`. `JobType.names()` and `JobState.names()` are the
names of the API's values, and `JobType.parse_name(text)` and `JobState.parse_name(text)` read a name as a user types
it (case, spaces, hyphens and underscores do not matter), or raise `AsvoError`.

Notes:

- `JobState` is the MWA ASVO API's job state. `str()` of a member is the API's value, for example
  `completed`. A job is ready for download when it is `Completed`. For an `Error` job, the message is in
  `AsvoJob.error_text`, and the server's error code (an integer, or `None`) is in `AsvoJob.error_code`. The
  MWA ASVO does not document the codes. `AsvoJobVec.all_ready` raises `AsvoError` of kind `JobFailed` for a
  failed job; its message includes the code, for example `has an error (code 7): the conversion failed`, and
  the exception has the attribute `error_code`.
- The download links of a completed job work for 7 days, and the job stays `Completed` in the API. `AsvoJob.expires` is when its links stop working (the `Expires` of its Acacia URLs, or `None`), `AsvoJob.is_expired` is whether they have, and `AsvoJob.display_state` is the state as `giant-squid list` shows it (`completed (ready)` or `completed (expired)` for a completed job). A download of an expired job, and `AsvoJobVec.all_ready` for one, raise `AsvoError` of kind `JobExpired`, with `job_id` and `expired`: resubmit the job.
- Times (`created`, `started`, `completed`, `modified`) are `datetime.datetime` objects with a time zone.
  The `date_from` and `date_to` arguments must have a time zone too, or the call raises `TypeError`.
- The files of a ready job are in `job.product.files` (a `JobProduct`). Each `JobFile` has `type` (a
  `Type`: `Acacia`, `Scratch` or `Dug`), `url`, `path`, `size`, `sha1` and `format`.

## Submit jobs

There are two submit methods, one for each MWA ASVO submit endpoint, and each takes a request body class:

| Method | Request body | `giant-squid` command |
|---|---|---|
| `submit_download_job(params)` | `DownloadJobParams(obs_id, ...)` | `submit-vis`, `submit-meta`, `submit-volt`, `submit-bf` |
| `submit_preprocess_job(params)` | `ConversionJobParams(obs_id, ...)` | `submit-conv` |
| `submit_preprocess_job(params)` | `ImagingJobFlow1Params(obs_id, ...)` | `submit-image` |
| `submit_preprocess_job(params)` | `ImagingJobFlow2Params(source_job_id, ...)` | `submit-image-from-job` (no Obs ID: the MWA ASVO takes it from the job) |
| `cancel_job(job_id)` | | `cancel` |

A `DownloadJobParams` is the body of a visibility, metadata, voltage or beamformer job. The body does not say which:
the MWA ASVO finds the kind of download from the observation. `only_get_metadata=True` asks for the metadata only.
`offset`, `duration`, `from_channel` and `to_channel` are for a voltage observation, and the MWA ASVO refuses them for
another kind. A voltage job is delivered to Scratch as files, which is not the default, so give
`delivery=DeliveryDestination.Scratch` and `delivery_format=DeliveryFormat.Files`. The MWA ASVO refuses any other
delivery for it.

Every keyword argument of a class has the MWA ASVO's default as its default value. The type stub
(`mwa_giant_squid.pyi`) shows it, for example `avg_freq_res: float | None = 40.0`, and so does `help()`. In
`help()` (and `inspect.signature`) an enum default is written as a quoted name, for example
`delivery='DeliveryDestination.Acacia'`, because Python accepts only literals there; the default is the enum member
`DeliveryDestination.Acacia`. Leave an argument out to use the default.

`None` is not the default. An argument that the MWA ASVO allows to be `null` (its type in the stub has `| None`)
accepts `None`, and `None` is sent as `null`. For example, `avg_freq_res=None` sends `"avg_freq_res": null`. The
MWA ASVO schema does not say what the MWA ASVO does with a `null` for each argument.

The keyword names are the MWA ASVO API field names, for example `avg_freq_res`. Where an argument has a
fixed set of values, the module has an enum (`DeliveryDestination`, `DeliveryFormat`, `OutputFormat`, `PhaseCentre`,
`ImageOutputMode`, `Polarization`, `ImageWeighting`, and `JobState`, `JobType` and `SortBy` for job lists). The
docstring of each class gives the limits of each number.

Each class is a plain class. Every argument is an attribute that can be read and changed, and the object can be
submitted again after a change. The class checks nothing when it is made or changed.

```python
from mwa_giant_squid import ConversionJobParams, DeliveryDestination, DeliveryFormat, OutputFormat

params = ConversionJobParams(
    1065880128,
    output=OutputFormat.Ms,
    avg_freq_res=80.0,
    delivery=DeliveryDestination.Acacia,
    delivery_format=DeliveryFormat.Tar,
)
params.avg_time_res = 4.0
reply = client.submit_preprocess_job(params)
print(reply.job_id, reply.status, reply.message)
```

The submit methods check the request body before they send a request. A value outside the MWA ASVO limits, or a
Job ID that is not valid, raises `ValueError` that names the argument, and an Obs ID that is not valid raises
`ObsIdError`. Nothing is sent. So does
`delivery_format=DeliveryFormat.Files` with `delivery=DeliveryDestination.Acacia` (the default): Acacia delivers a tar
only. The other deliveries take either format. `submit_preprocess_job` raises `TypeError` for an object that is not one
of the three preprocess classes.

Each submit method returns a `JobSubmittedResponse` with `job_id`, `message` and `status`. The `status`
(a `Status`: `Success` or `Failed`) and the `message` describe the reply and are for display. A call that fails
raises an exception, so do not use `status` to decide whether a call worked.

`cancel_job` returns a `JobCancelledResponse`, which has the same attributes. The MWA ASVO answers the cancellation of a job that is already cancelled
with a normal reply, not an error, so no exception is raised. The reason is in `message`. Any other refusal
is an error and raises `AsvoApiError`.

### Submit the jobs of a manta-ray-client CSV file

`submit_csv_file` submits the jobs of a [manta-ray-client](https://github.com/MWATelescope/manta-ray-client) CSV
file (the file that `mwa_client -c` reads). Each line of the file is one job. It is the same method as
`AsvoClient::submit_csv_file` of the Rust library, and the file format is the one that
[the README](https://github.com/MWATelescope/giant-squid#jobs-from-a-manta-ray-client-csv-file) describes.

```python
replies = client.submit_csv_file("jobs.csv")  # a str or a path
for reply in replies:
    print(reply.job_id, reply.message)
```

The whole file is read and checked before anything is submitted. Then the jobs are submitted in the order of the
file, and the list has the `JobSubmittedResponse` of each. A key that a line does not give has the default of the
MWA ASVO. The arguments `delivery`, `delivery_format` and `allow_resubmit` are the defaults of the lines that do not
give that key (`submit_csv_file` reads no environment variable, so `GIANT_SQUID_DELIVERY` is not used):

```python
from mwa_giant_squid import DeliveryDestination

replies = client.submit_csv_file("jobs.csv", delivery=DeliveryDestination.Scratch, allow_resubmit=True)
```

A key that a line gives twice, and a line that makes the same job as an earlier line, are logged as warnings (see
[Logging](#logging)).

If the file has a problem, the call raises an exception and submits nothing:

- `OSError` (for example `FileNotFoundError`): the file cannot be read.
- `ValueError`: the file has no job (it is empty, or has only blank lines and comments), or a line has a problem,
  for example a key that the job type does not have, or a value outside the limits of the MWA ASVO. The message
  lists every bad line with its line number.

A job that fails does not stop the others. When any job fails, the call raises `CsvSubmitError` after it has tried
every job. The jobs that did not fail were submitted: the exception has their replies, so you do not submit them
again.

```python
from mwa_giant_squid import CsvSubmitError

try:
    replies = client.submit_csv_file("jobs.csv")
except CsvSubmitError as e:
    for reply in e.submitted:  # the jobs that were submitted, in the order of the file
        print("submitted", reply.job_id)
    for failure in e.failures:  # the jobs that were not
        print("line", failure.line, "Obs ID", failure.obs_id, "failed:", failure.error)
```

Each `CsvJobFailure` has `line` (of the file), `obs_id` and `error`, the `AsvoApiError` of the request.

## Wait for jobs

The module has no wait function. Write the loop in your own code. This lets you choose the interval and
handle Ctrl-C and timeouts yourself. `AsvoJobVec.all_ready` makes no request. It checks a list that
you already have.

```python
import time

POLL_INTERVAL_S = 60

job_ids = [reply.job_id]
while not client.get_jobs().all_ready(job_ids):
    time.sleep(POLL_INTERVAL_S)
```

`all_ready` returns `False` while a job is in progress. It raises `AsvoError` if a job is missing, has
an error, was cancelled, or is completed but its download links have expired (kind `JobExpired`).

## Download

`download_job(job_id, options)` downloads the files of one ready job. `download_obs(obs_id, options)` does the same
for the one ready job of an Obs ID. Both return the `AsvoJob` that was downloaded, so `download_obs` gives the
Job ID of the Obs ID's job. `options` is a `DownloadOptions`.

```python
client.download_job(reply.job_id, gs.DownloadOptions("/data/mwa"))
```

The call gets the job list first. To download several jobs, get the list once and use `download_job_from(jobs,
job_id, options)` or `download_obs_from(jobs, obs_id, options)` for each: they find the job in your list and make
no request for it.

The directory must exist. The call blocks until the download ends. It releases the GIL, so other Python
threads run during the download. One `DownloadOptions` can serve a series of downloads: set `download_number`
between them. `DownloadSettings.from_env()` reads `GIANT_SQUID_BUF_SIZE` and `GIANT_SQUID_DOWNLOAD_RETRY_SECS`, as the
command does. Give its `buffer_size` and `retry_duration` to the options.

`DownloadOptions(download_dir, *, keep_tar=False, job_subdirectory=True, no_resume=False, hash=True,
progress=None, buffer_size=None, retry_duration=None, download_number=1, download_count=1)`. Each is also an
attribute.

| Argument | Meaning |
|---|---|
| `keep_tar` | Keep the tar file. By default the module unpacks it while it downloads. |
| `job_subdirectory` | Put the files in a directory of their own, `download_dir/<job_id>`, which is made if it is not there. Default: `True`, as for the `giant-squid` command. `False` puts the files in `download_dir` itself, as giant-squid 2.x did (use it for a partial download that an earlier version left there). It has no effect on a Scratch job, which is moved into `download_dir` in a folder of its own. |
| `no_resume` | Download the whole file again, even if part of it is on disk. |
| `hash` | Check the SHA-1 hash of the file. A resumed download, and a complete file that is already on disk, are always checked. Default: `True`. |
| `progress` | A function that receives `DownloadProgress` events. |
| `download_dir` | The directory for the files. It must exist. |
| `buffer_size` | How many bytes to hold in memory before they are written. Default: `DEFAULT_DOWNLOAD_BUFFER_SIZE`. At least `MIN_DOWNLOAD_BUFFER_SIZE`. |
| `retry_duration` | How long to retry a failing download, in seconds. `0` disables retries. Default: `DEFAULT_DOWNLOAD_RETRY_DURATION`. |
| `download_number`, `download_count` | The place of this download in a series. They set the `[1/2]` text in labels. |

### Resume and Ctrl-C

A partial file stays on disk. A new call resumes it. When you press Ctrl-C in the main thread, the
download stops at the next chunk (or at once, while it waits to retry) and the call raises
`KeyboardInterrupt`. This also holds when the signal arrives while the module is writing a log record: the
module takes the `KeyboardInterrupt` that the logging call raised and stops the download. In any other thread
Python does not run signal handlers, so a download in a worker thread cannot be stopped by Ctrl-C.

### Progress

The `progress` function receives one of three event classes. They are subclasses of `DownloadProgress`,
so `isinstance` and `match` work:

```python
from mwa_giant_squid import DownloadProgress

total = 0
done = 0


def on_progress(event: DownloadProgress) -> None:
    global total, done
    match event:
        case DownloadProgress.Started(total_bytes=total_bytes, position=position):
            total, done = total_bytes, position  # a second Started for a file means a restart
        case DownloadProgress.Advanced(bytes=n):
            done += n
            print(f"{done / total:.0%}", end="\r")
        case DownloadProgress.Finished():
            print()


client.download_job(job_id, gs.DownloadOptions("/data/mwa", progress=on_progress))
```

`Advanced` events are combined, so the function is called about 10 times each second at most. If
the function raises an exception, the download stops and the call raises that exception.

## Errors

| Exception | When |
|---|---|
| `AsvoApiError` | An API call failed. This includes a login that failed and an API call inside a download. For an error that the server describes, the message is the server's: its error code and message, then its detail, its suggestion, the field errors and the request ID, each on a line of its own. |
| `AsvoError` | A job check or download failed: the job is missing or not ready, a transfer failed, or a hash does not match. (A file that cannot be read or written is an `OSError`, not this.) |
| `ObsIdError` | A number or text is not a valid Obs ID. `ObsIdErrorWrongNumDigits` and `ObsIdErrorParse` are the subclasses for its two kinds. |
| `ParseError` | `parse_obs_ids_only` and the other `parse_*` functions could not read IDs or a time. It has a subclass for each kind (`ParseErrorNoObsIds`, `ParseErrorJobIdsGiven`, `ParseErrorInvalidTime` and so on). A file that cannot be read is an `OSError`. |
| `ValueError`, `TypeError`, `OverflowError` | An argument is not valid. The module raises these before it sends a request. |
| `OSError` | A file or directory cannot be read, written or made: the CSV file of `submit_csv_file`, or a file or directory of a download. Python gives the subclass (`FileNotFoundError`, `PermissionError`, `NotADirectoryError` and so on), with the operating system's error number, and the file name when the module knows it. |
| `CsvSubmitError` | `submit_csv_file` submitted the jobs of a file, and some failed. It has `submitted` (the replies of the jobs that were submitted) and `failures` (a `CsvJobFailure` for each job that was not). |
| `KeyboardInterrupt` | Ctrl-C stopped a download. |

`AsvoApiError`, `AsvoError`, `ObsIdError` and `ParseError` have a `kind` attribute: the name of the Rust error
variant. `ObsIdError` and `ParseError` also have a subclass for each kind (`ParseErrorNoObsIds`), so you can catch
one kind with `except`. The other attributes exist only for the kinds that use them (see the docstrings). For example:

```python
from mwa_giant_squid import AsvoApiError

try:
    client.cancel_job(1)
except AsvoApiError as e:
    if e.kind == "ApiError":
        print(e.error_code, e.message, e.field_errors, e.request_id)
    else:
        raise
```

## Logging

The module sends its log records to the Python `logging` module. The logger names start with
`mwa_giant_squid`. Records below `DEBUG` are not sent, because they can contain tokens.

The module connects to `logging` when you make the first `AsvoClient`, not when you import it. A Rust program can have only one log destination per
process, and the `giant-squid` command needs its own. A log record that something else makes before that point
is not sent anywhere. If another extension module has already set the Rust log destination in the process, the
records go there and `reset_logging()` does nothing.

```python
import logging

logging.basicConfig(level=logging.INFO)
logging.getLogger("mwa_giant_squid").setLevel(logging.DEBUG)
mwa_giant_squid.reset_logging()
```

For speed, the module remembers each logger and its level the first time it logs. If you change the
logging configuration after that, call `reset_logging()`.

A log handler that you add runs inside the module's calls, in the thread that made the call, so keep it fast.
A handler that raises a `KeyboardInterrupt` or `SystemExit` stops a download, like Ctrl-C. Any other exception
from a handler is cleared by the module, and does not break a download.

The `giant-squid` command does not use `logging`: it writes its own log lines to standard error.

## Threads

One `AsvoClient` can be shared by several threads. If the server rejects the session, only one thread
logs in again. The other threads use its new session.

## Limits

- There is no `async` API. Use `asyncio.to_thread` or a thread pool to run calls in the background.
- There are no wheels for free-threaded Python and no wheels for Windows.
