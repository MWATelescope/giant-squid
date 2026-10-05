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
```

The command is the Rust `giant-squid` program itself, not a copy of it. The module has the program's code
in it, and the `giant-squid` script that the install makes runs that code. So the sub-commands, short names,
options, defaults, environment variables, help, messages, colours and exit codes are those of the Rust
program. For what each one does, see the
[README](https://github.com/MWATelescope/giant-squid/blob/main/README.md) or run `giant-squid --help` and
`giant-squid <command> --help`. The commands are `list` (`l`), `download` (`d`), `submit-vis` (`sv`),
`submit-conv` (`sc`), `submit-image` (`si`), `submit-image-from-job` (`sifj`), `submit-meta` (`sm`),
`submit-volt` (`st`), `submit-bf` (`sb`), `wait` (`w`) and `cancel` (`c`).

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

The module reads no environment variable. Your program reads the key and gives it to the client:

```python
import os

import mwa_giant_squid

client = mwa_giant_squid.AsvoClient(
    "https://asvo.mwatelescope.org:443",
    os.environ["MWA_ASVO_API_KEY"],
)
```

`AsvoClient(host, api_key, *, api_timeout=None, token_cache_path=None)` logs in when it is created.

| Argument | Meaning |
|---|---|
| `host` | The MWA ASVO URL. Use `http://` only for a local test server. All other hosts must use TLS. |
| `api_key` | Your API key. |
| `api_timeout` | The time limit for one API request, in seconds. `None` uses the library's `DEFAULT_API_TIMEOUT`. |
| `token_cache_path` | A file in which to keep the session between runs. `None` keeps the session in memory only. |

The MWA ASVO allows only a few logins each minute. A program that runs often must set
`token_cache_path`. The `giant-squid` command and `manta-ray-client` use `$HOME/.mwa-asvo/tokens.json`.
You can give the same path to share their session:

```python
from pathlib import Path

client = mwa_giant_squid.AsvoClient(
    host,
    api_key,
    token_cache_path=Path.home() / ".mwa-asvo" / "tokens.json",
)
```

To use a different MWA ASVO server (for example, to test a new feature), give its URL as `host`.

## List jobs

`get_jobs` asks the server for your jobs. The server does the filtering. Every filter that is `None`
does not filter, except `days` and `sort_by`: with `days=None` you get the jobs from the MWA ASVO's default
window (the schema's default for `days`, which is also the most `days` can be), not your
full history. A job older than that is not returned, so `download_job` and `AsvoJobVec.all_ready` cannot find
it either.

```python
from mwa_giant_squid import JobState

jobs = client.get_jobs(days=7, job_state=JobState.Completed)  # days: 1 to 30
for job in jobs:
    print(job.job_id, job.obs_id, job.job_type, job.job_state, job.created)
```

`list_jobs` works as `giant-squid list` does. It accepts lists of job IDs, obsids, job types and job
states. The server does what it can, and the module filters the rest:

```python
from mwa_giant_squid import JobState, JobType

jobs = client.list_jobs(
    obs_ids=[1065880128, 1065880248],
    job_types=[JobType.Visibility],
    job_states=[JobState.Completed, JobState.Error],
)
```

The result is an `AsvoJobVec`. It supports `len()`, indexing and iteration. Its `filter` method
filters a list that you already have, without a request.

Notes:

- `JobState` is the MWA ASVO API's job state. `str()` of a member is the API's value, for example
  `completed`. A job is ready for download when it is `Completed`. For an `Error` job, the message is in
  `AsvoJob.error_text`, and the server's error code (an integer, or `None`) is in `AsvoJob.error_code`. The
  MWA ASVO does not document the codes. `AsvoJobVec.all_ready` raises `AsvoError` of kind `JobFailed` for a
  failed job; its message includes the code, for example `has an error (code 7): the conversion failed`, and
  the exception has the attribute `error_code`.
- Times (`created`, `started`, `completed`, `modified`) are `datetime.datetime` objects with a time zone.
  The `date_from` and `date_to` arguments must have a time zone too, or the call raises `TypeError`.
- The files of a ready job are in `job.product.files` (a `JobProduct`). Each `JobFile` has `type` (a
  `Type`: `Acacia`, `Scratch` or `Dug`), `url`, `path`, `size`, `sha1` and `format`.

## Submit jobs

Each job type has one method. Every keyword argument that is `None` uses the MWA ASVO default.

| Method | `giant-squid` command |
|---|---|
| `submit_download_vis_job(obs_id, ...)` | `submit-vis` |
| `submit_download_meta_job(obs_id, ...)` | `submit-meta` |
| `submit_conversion_job(obs_id, ...)` | `submit-conv` |
| `submit_imaging_job(obs_id, ...)` | `submit-image` |
| `submit_image_from_job(obs_id, source_job_id, ...)` | `submit-image-from-job` |
| `submit_voltage_job(obs_id, offset, duration, ...)` | `submit-volt` |
| `submit_beamformer_job(obs_id, ...)` | `submit-bf` |
| `cancel_job(job_id)` | `cancel` |

The keyword names are the MWA ASVO API field names, for example `avg_freq_res`. Where an argument has a
fixed set of values, the module has an enum (`Delivery`, `DeliveryFormat`, `Output`, `Centre`,
`OutputMode`, `Polarization`, `Weighting`). The docstring of each method gives the limits of each number.

```python
from mwa_giant_squid import Delivery, DeliveryFormat, Output

reply = client.submit_conversion_job(
    1065880128,
    output=Output.Ms,
    avg_freq_res=80.0,
    avg_time_res=4.0,
    delivery=Delivery.Acacia,
    delivery_format=DeliveryFormat.Tar,
)
print(reply.job_id, reply.status, reply.message)
```

The methods check the arguments before they send a request. A value outside the MWA ASVO limits raises
`ValueError` (or `OverflowError`) that names the argument. Nothing is sent.

Each submit method returns a `JobSubmittedResponse` with `job_id`, `message` and `status`. The `status`
(a `Status`: `Success` or `Failed`) and the `message` describe the reply and are for display. A call that fails
raises an exception, so do not use `status` to decide whether a call worked.

`cancel_job` returns a `JobCancelledResponse`, which has the same attributes. The MWA ASVO answers the cancellation of a job that is already cancelled
with a normal reply, not an error, so no exception is raised. The reason is in `message`. Any other refusal
is an error and raises `AsvoApiError`.

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
an error or was cancelled.

## Download

`download_job` downloads the files of one ready job. `download_obs` does the same for the one ready job
of an obsid. Both return the `AsvoJob` that was downloaded, so `download_obs` gives the job ID of the
obsid's job.

```python
client.download_job(reply.job_id, "/data/mwa")
```

The directory must exist. The call blocks until the download ends. It releases the GIL, so other Python
threads run during the download.

| Argument | Meaning |
|---|---|
| `keep_tar` | Keep the tar file. By default the module unpacks it while it downloads. |
| `no_resume` | Download the whole file again, even if part of it is on disk. |
| `hash` | Check the SHA-1 hash of the file. A resumed download, and a complete file that is already on disk, are always checked. Default: `True`. |
| `progress` | A function that receives `DownloadProgress` events. |
| `buffer_size` | How many bytes to hold in memory before they are written. Default: the library's `DEFAULT_DOWNLOAD_BUFFER_SIZE`. |
| `retry_duration` | How long to retry a failing download, in seconds. `0` disables retries. Default: the library's `DEFAULT_DOWNLOAD_RETRY_DURATION`. |
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


client.download_job(job_id, "/data/mwa", progress=on_progress)
```

`Advanced` events are combined, so the function is called about 10 times each second at most. If
the function raises an exception, the download stops and the call raises that exception.

## Errors

| Exception | When |
|---|---|
| `AsvoApiError` | An API call failed. This includes a login that failed and an API call inside a download. For an error that the server describes, the message is the server's: its error code and message, then its detail, its suggestion, the field errors and the request ID, each on a line of its own. |
| `AsvoError` | A job check or download failed: the job is missing or not ready, a transfer failed, or a hash does not match. |
| `ValueError`, `TypeError`, `OverflowError` | An argument is not valid. The module raises these before it sends a request. |
| `OSError` | A file cannot be read or written. |
| `KeyboardInterrupt` | Ctrl-C stopped a download. |

Both `AsvoApiError` and `AsvoError` have a `kind` attribute: the name of the Rust error variant. The
other attributes exist only for the kinds that use them (see the docstrings). For example:

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
