# Plan: giant-squid as a Rust library and a Python library (PyO3)

## Status

- 2026-09-24: plan reviewed and all questions answered. No code written
  yet. Written against `apiv2` at commit `e517d6c`.
- Next step: Phase 0, step 0.1 (`AsvoClientConfig`). Start from a fresh
  clone of `apiv2`, one diff per step, and update this section after each.

## Goal

Three layers, and each talks only to the layer below it:

```text
Rust CLI (src/bin/giant-squid.rs)  ─┐
                                    ├─>  Rust library (mwa_giant_squid)  ─>  MWA ASVO API
Python caller or Python CLI        ─┘    (Python calls it through the PyO3 module)
```

- The **Rust library** is a public API that a Rust program can use directly.
  It reads no environment variables and prints nothing.
- The **Rust CLI** reads environment variables and flags, builds explicit
  configuration, calls the library, and does all printing.
- The **Python module** (`pip install mwa-giant-squid`, `import
  mwa_giant_squid`) wraps the same library, with the same names.

```python
import mwa_giant_squid

client = mwa_giant_squid.AsvoClient(api_key="...", host="https://asvo.mwatelescope.org:443")
resp = client.submit_download_vis_job(1234567890, allow_resubmit=True)
for job in client.get_jobs(days=7):
    print(job.jobid, job.obsid, job.jtype, job.state)
client.cancel_job(resp.job_id)
```

## Decisions (from review)

1. Names are the same as in Rust, where Python practice allows it.
2. The Python library takes constructor arguments and reads no environment
   variables. So does the Rust library (see Phase 0).
3. Downloads report progress through a callback. The library has no UI.
4. No free-threaded wheels for now.
5. PyPI name `mwa-giant-squid`, import name `mwa_giant_squid`.
6. Same crate, behind a `python` Cargo feature (as mwalib).
7. The Python package installs no command of its own for now. An example
   CLI in `examples/python/` shows how to build one.
8. The in-process tests may change how they build the client (from an
   `AsvoClientConfig` instead of environment variables). Their assertions
   stay the same.

Also kept from the first draft: optional job arguments default to `None`,
meaning "use the OpenAPI schema default", so neither layer adds defaults of
its own; Python >= 3.10 with one `abi3` wheel per platform; maturin;
`pyo3-stub-gen` for `.pyi` stubs (as mwalib); pyo3 0.29.

## Naming in Python

Rust names are kept for types, methods, fields and enum variants
(`AsvoClient`, `get_jobs`, `submit_download_vis_job`, `AsvoJob.jobid`,
`AsvoJobState.Ready`). Rust method and field names are already snake_case,
and CamelCase enum variants are valid Python.

Exceptions: Python practice is that an exception name ends in `Error`. The
two Rust error enums already do, so Python gets two exception classes with
the same names, `AsvoApiError` and `AsvoError`. Each has a `kind` attribute
with the Rust variant name (for example `"ApiError"`, `"MissingAuthKey"`)
plus that variant's fields (`error_code`, `message`, `detail`,
`suggestion`, `code`). This keeps every name identical to Rust.

## Phase 0: make the Rust library a clean public API (Rust only)

Each step is its own diff; the CLI keeps its current behaviour.

| Step | Change | Why |
|---|---|---|
| 0.1 | New `AsvoClientConfig { host, api_key, api_timeout, token_cache_path: Option<PathBuf> }` and `AsvoClient::new(config)`. The host is stored in the client, not read from `MWA_ASVO_HOST` on each request. The CLI reads `MWA_ASVO_HOST`, `MWA_ASVO_API_KEY`, `MWA_ASVO_API_TIMEOUT` and `HOME` and builds the config | The library reads no environment |
| 0.2 | `DownloadOptions` gets `buffer_size` and `retry_duration`. The CLI reads `GIANT_SQUID_BUF_SIZE` and `GIANT_SQUID_DOWNLOAD_RETRY_SECS` | Same |
| 0.3 | `DownloadOptions.progress_bar` becomes an optional progress callback (`Option<&dyn Fn(DownloadProgress)>`). The CLI adapts it to its `indicatif` bars; `indicatif` becomes a CLI-only dependency | No UI in the library. Also, today the library does not build without the `bin` feature because `src/asvo/mod.rs` imports `indicatif` |
| 0.4 | `AsvoJobVec::list` (the table) and the table-style helpers move to the CLI; `prettytable` becomes CLI-only | The library prints nothing |
| 0.5 | `download_jobid` and `download_obsid` return `Result<(), AsvoError>`, not `anyhow::Result` | A library returns typed errors; Python maps them to exceptions |
| 0.6 | `AsvoClient` uses a `Mutex` instead of a `RefCell` | PyO3 requires a `#[pyclass]` to be `Sync`, and network calls run with the GIL released (`py.detach`) |
| 0.7 | Move `wait_loop` into the library as `AsvoClient::wait_for_jobs(jobids, poll_interval)`; the 60 s interval becomes a named default | Shared by both CLIs |
| 0.8 | Move the `list` filters into the library (`AsvoJobVec::filter(jobids, obsids, jtypes, states)`) | Same |
| 0.9 | Add `AsvoClient::submit_download_meta_job` (wraps `submit_download_vis_job` with `download_type = meta`) | The `submit-meta` command has no library method today; this gives Rust and Python the same name |
| 0.10 | A library-level example (`examples/list_jobs.rs`) that uses only the public API | Shows the library works without the CLI |

**Test impact (approved 2026-09-24).** The in-process tests in
`src/asvo/test.rs` and `src/asvo/apiv2/client/test.rs` build the client
through environment variables (`TestEnv` sets `MWA_ASVO_HOST`, `HOME`,
and others). After 0.1 and 0.2 they must build an `AsvoClientConfig`
instead. Their assertions do not change. This also removes the need for
the process-wide `ENV_LOCK`, so those tests can run in parallel. The CLI
tests in `tests/cli.rs` and `tests/live.rs` do not change.

## Phase 1: Python module skeleton

- `Cargo.toml`: `crate-type = ["rlib", "cdylib"]`; `python` feature
  (`pyo3` with `extension-module` and `abi3-py310`, `pyo3-log`);
  `python-stubgen` feature (as mwalib).
- `pyproject.toml`: maturin backend, `features = ["python"]`,
  `module-name = "mwa_giant_squid"`, version from `Cargo.toml`,
  `requires-python = ">=3.10"`.
- `src/python/` holds all binding code, behind `#[cfg(feature = "python")]`.
- Rust `log` output goes to Python `logging` (logger `mwa_giant_squid`).

## Phase 2: Python API

`AsvoClient(host, api_key, *, api_timeout=None, token_cache_path=None)`.
With `token_cache_path=None` the session is kept in memory only. A script
that runs often should pass a path, because the server allows only a few
logins a minute.

| Python (same name as Rust) | CLI command |
|---|---|
| `get_jobs(days=None) -> AsvoJobVec` | `list` |
| `submit_download_vis_job(obs_id, *, delivery=None, delivery_format=None, allow_resubmit=None)` | `submit-vis` |
| `submit_download_meta_job(obs_id, ...)` | `submit-meta` |
| `submit_conversion_job(obs_id, *, ...)` | `submit-conv` |
| `submit_imaging_job(obs_id, *, ...)` | `submit-image` |
| `submit_image_from_job(obs_id, source_job_id, *, ...)` | `submit-image-from-job` |
| `submit_voltage_job(obs_id, offset, duration, *, ...)` | `submit-volt` |
| `submit_beamformer_job(obs_id, *, ...)` | `submit-bf` |
| `cancel_job(jobid) -> JobSubmittedResponse` | `cancel` |
| `wait_for_jobs(jobids, poll_interval=None)` | `wait` |
| `download_jobid(jobid, download_dir, *, keep_tar=False, no_resume=False, hash=True, progress=None)` | `download` |
| `download_obsid(obs_id, ...)` | `download` |

Keyword names are the OpenAPI field names (for example `avg_freq_res`), so
they match the schema and the Rust builder methods.

Module functions: `parse_many_jobids_or_obsids`, and one `*_params`
builder per job type that returns the request body as a `dict` (for a
caller's own dry run).

Types: `AsvoJob`, `AsvoJobVec` (iterable, with `filter` and `json`),
`AsvoFilesArray`, `JobSubmittedResponse`, and the enums `AsvoJobType`,
`AsvoJobState`, `Delivery`, `DeliveryFormat`, `Output`, and others.
`AsvoJobState::Error(String)` carries data, so in Python it is
`AsvoJobState.Error` and the message is in `AsvoJob.error_text`.

Long calls: `wait_for_jobs` sleeps in short slices and checks for Ctrl-C;
downloads check between chunks. Both release the GIL.

## Phase 3: stubs, docs, tests, CI

- `.pyi` stubs from `pyo3-stub-gen`, docstrings, and `docs/PYTHON.md`.
- pytest suite against a local mock server (`pytest-httpserver`), never a
  real server. `examples/python/` holds a small Python CLI (`list`,
  `submit-vis`, `cancel`) that shows how to build one on the module, and
  CI runs it as a smoke test.
- CI builds wheels for the `run-tests.yaml` platforms (Linux
  x86_64/aarch64, macOS x86_64/arm64) plus an sdist, and runs pytest;
  `ruff check`, `ruff format` (line length 120) and `ty check` on the
  Python code. Publishing to PyPI stays your step.

## Out of scope for now

An `async` API, free-threaded wheels, Windows wheels, and an installed
Python command.
