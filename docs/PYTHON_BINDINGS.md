# Plan: giant-squid as a Rust library and a Python library (PyO3)

## Status

- 2026-09-24: plan reviewed and all questions answered. No code written
  yet. Written against `apiv2` at commit `e517d6c`.
- 2026-09-29: step 0.1 done. `AsvoClientConfig` and
  `AsvoClient::new(config)`; the library reads no `MWA_ASVO_*` or `HOME`
  variable. The CLI builds the config in `src/cli/config.rs`. The unit
  tests build the config in `src/test_config.rs`.
- 2026-09-29: step 0.2 done. `DownloadOptions` has `buffer_size` (bytes)
  and `retry_duration`; the CLI reads `GIANT_SQUID_BUF_SIZE` and
  `GIANT_SQUID_DOWNLOAD_RETRY_SECS` in `src/cli/config.rs`. The library
  reads no environment variable. `ENV_LOCK` is removed, so the unit tests
  run in parallel.
- 2026-09-29: `AsvoError::Parse` removed (no producer after 0.2).
- 2026-09-29: step 0.3 done. `DownloadOptions.progress` is an
  `Option<&dyn Fn(DownloadProgress)>`; `DownloadProgress` has `Started`,
  `Advanced` and `Finished` events. The CLI shows them on its `indicatif`
  bars (`update_progress_bar` in the binary). `indicatif` is used only by
  the binary. `cargo check --no-default-features` now fails only on
  `anyhow` (step 0.5).
- 2026-09-29: step 0.4 done. The job table (`print_jobs_table`) and the
  table style helpers are in `src/cli/table.rs`; `prettytable-rs` is a
  `bin`-only dependency. The unused `AsvoJobType::prettytable_colour` is
  removed. Two new CLI tests cover the table output.
- 2026-09-29: step 0.5 done. `download_jobid` and `download_obsid`
  return `Result<(), AsvoError>`. The new variant `AsvoError::AsvoApi`
  wraps an `AsvoApiError` from the job list request. The library now
  builds with `--no-default-features` (no `anyhow`, `indicatif` or
  `prettytable-rs`).
- 2026-09-29: step 0.6 done. `AsvoClient` holds its HTTP client in a
  `Mutex`, so it is `Send + Sync`. The lock is held only to clone or swap
  the client, not during a request. A re-login after a rejected token is
  serialised (`login_lock` plus a generation counter), so when several
  threads have the same token rejected, only the first logs in again and
  the others reuse its token. Three new tests: a compile-time
  `Send + Sync` check, one client used from four threads at once, and
  eight threads sharing a rejected token cause exactly one login.
- 2026-09-29: step 0.7 done (revised: decision 9). The library has
  `AsvoJobVec::all_ready(jobids)`, a single check with no request and no
  sleep. The poll loop, its 60 s interval and the state-change logs stay
  in the CLI (`wait_loop` in the binary). New `AsvoError` variants
  `JobFailed`, `JobExpired` and `JobCancelled` keep the old messages.
  Seven new library tests and two new CLI tests.
- 2026-09-29: step 0.8 done. `AsvoJobVec::filter(jobids, obsids,
  jtypes, states)` keeps jobs that match every non-empty filter (states
  compare by kind, so any `Error(..)` matches). `list` and `wait` use it.
  The CLI still rejects job IDs and obsids together, now before it
  connects. Six new library tests.
- 2026-09-29: step 0.9 done. `AsvoClient::submit_download_meta_job(params)`
  sets `download_type = meta` and calls `submit_download_vis_job`.
  `submit-meta` uses it. `submit_download_vis_job` is unchanged (it does
  not force `vis`). One new library test.
- Next step: Phase 0, step 0.10 (library-level example
  `examples/list_jobs.rs`). Start from a fresh clone of `apiv2`, one diff
  per step, and update this section after each.

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
9. Library functions do not implement poll loops (added 2026-09-29).
   The library gives single checks; the caller does the loop and the
   sleep. This keeps Ctrl-C, timeouts and progress output in the
   caller's hands.

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
| 0.7 | Add `AsvoJobVec::all_ready(jobids)`: one check of a job list, with typed errors for a missing, failed, expired or cancelled job. The poll loop stays in the CLI (decision 9) | Shared by both CLIs |
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
| `get_jobs()` then `AsvoJobVec.all_ready(jobids)`, in a loop the caller writes | `wait` |
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

Long calls: downloads check for Ctrl-C between chunks and release the
GIL. There is no library wait call; the caller's own loop (for example
with `time.sleep`) handles Ctrl-C as normal Python code.

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
