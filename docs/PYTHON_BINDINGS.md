# Plan: giant-squid as a Rust library and a Python library (PyO3)

## Handoff (read this first)

Written 2026-10-02, and updated at the end of that session. Branch `apiv2`
at `5053f08` (the commit of diff 27), plus diff 28 (below). Level 1 of the
thin-client refactor is finished, and Level 3 option A is under way. The
entries under "Status"
below are the detailed log; this section is the summary and the list of what
to do next.

**The work in progress is the "thin clients" refactor: start at "Open items",
item 1.**

### Where things are

- Phases 0 to 3 of the plan are done: the Rust library, the Python module
  (`mwa_giant_squid`), the `.pyi` stubs, `docs/PYTHON.md`, the installed
  Python `giant-squid` command (`mwa_giant_squid_cli/`), and the CI
  workflow `.github/workflows/python.yaml`.
- The 2026-10-01 list is finished, except the items under "Open items"
  below: the cancel log wording, the allowed values in `--help`, the README
  sections for `wait` and `cancel`, the CHANGELOG entry, the migration guide
  (its **(check this)** markers are gone), the crate `exclude` list, the
  wheels on the release, the stub drift check, and the live test run. The
  work since then, with its reasons, is in the entries dated 2026-10-02.
- The Python CI has run on GitHub and works. It showed two faults, both fixed: a test that compared log lines with
  their clock time, and a Ctrl-C that was lost when it arrived in a log
  call (a real bug of the module, not only of the test).
- Tests at the last run (after diff 28): 232 Rust unit tests (one is the
  `#[ignore]`d recording test) and 37 CLI tests; 346 pytest tests; 18 live tests
  (`tools/run_live_tests.sh`, by hand, against test-asvo), which all passed
  on 2026-10-02; clippy (default features and `python`), `ruff`, `ty`,
  stubtest and the stub drift check clean. The doctests cannot run in the
  sandbox (no `rustdoc`); CI runs them. `docs/TESTING.md` describes every
  layer and the tests that pin a decision.
- The docs are current as of that day: `README.md`, `CHANGELOG.md` (the
  3.0.0 date is still `2026-09-??`), `docs/V3_MIGRATION.md`,
  `docs/PYTHON.md`, `docs/TESTING.md` and this file.

### Decisions in force

- **The Rust and the Python command are thin clients of the Rust library**
  (decided 2026-10-02). They do not repeat validation, enums, enum-to-text
  conversion, logic or messages; those live in the library, and Python gets
  them from the module. Error messages come through from the API (and the
  library) unrewritten. If a change would rewrite an error, tell the user and
  let them decide. The review that led to this is the "thin clients" entry in
  Status. The user's decisions on the rewrites it found:
  - The library and both commands show the API's `detail` and `suggestion`
    (done, diff 23).
  - R1 (`expected one of: ...` for a bad schema enum value): keep.
  - R2 (`--image-size`): keep the `Invalid image_size:` wrapper, that is,
    show the library's full text (diff 26). This was my reading of "keep the
    wrapper"; the user has not confirmed it.
  - R3 (the per-obsid errors, and again in `N of M obsids failed`), R5 and R6
    (the guard in `list`, the usage-error layout of the Python command):
    keep as they are.
  - R4 (the Python job state and type parser): keep the `Invalid ...` form,
    with the wording of R1: `Invalid <what> '<text>': expected one of: ...`
    (diff 25).
  - R7 (the messages about environment variables): one phrasing, from the
    library (done, diff 24).
  - `job_params` of a listed job stays untyped for now.
- The library reads the environment only when a program calls
  `client_config_from_env`, `DownloadSettings::from_env` (Rust) or
  `AsvoClient.from_env`, `DownloadSettings.from_env` (Python). Nothing else
  reads it.
- The OpenAPI schema is the standard: defaults, names and limits come from
  it, and a wrong schema is fixed in the API, not in giant-squid.
- Only parameters of the API schema are sent, and only end-user endpoints
  are called. The library has no unused API calls or fields. The bodies are
  the generated types, and tests check every request field against the
  schema (so a conversion body has no `flags`, which a listed job's
  `job_params` shows but the request schema does not have).
- The processor and scheduler endpoints (`/v2/job_staged`,
  `/v2/scheduler/*`, `/v2/calibration_ready`) are not wrapped. Their types
  stay in `openapi.rs`. `staging_count` is never set (the generated types
  leave it out of the body, which is `null`, the API default), and it is
  not an option in Rust, the CLI or Python, because the API will remove it.
- `openapi.rs` is committed as `build.rs` writes it. Do not run
  `cargo fmt` or `cargo clippy --fix` on it: the `openapi-drift-check` job
  of `run-tests.yaml` regenerates the file and fails on any difference.
- Success or failure of a call is the HTTP status. The response `status`
  text ("success"/"failed") is descriptive, like `message`.
- A refused cancel of a job that is already cancelled is HTTP 200 with
  `status: failed`; any other refusal is a 4xx error. `cancel` logs
  `Cancel request for job N: <message>` and `Cancel requests: N sent, M
  failed.`, and never says a job was cancelled.
- `list` without `--days` uses the API default (30 days, read from the
  generated type), not `null` and not the full history. `wait` and
  `download` list jobs the same way.
- `wait` and `cancel` take job IDs only: an obsid is an error that names
  it, and nothing is sent.
- `list --job-types` refuses text that is not a job type, and accepts
  `download_voltage` as well as `download_voltages`.
- The Python package installs a `giant-squid` command with the same
  commands and options as the Rust one. The crates.io package leaves out
  every Python-only file (`exclude` in `Cargo.toml`); the sdist keeps the
  stub (`include` in `pyproject.toml`). The wheels are attached to each
  GitHub release by `releases.yaml`; nothing publishes to PyPI.
- A module that has tests is a folder with `mod.rs` (the code) and
  `tests.rs` (the tests).
- An astroquery module is out of scope for this project.
- One diff per step, each from a fresh clone of the latest `apiv2`. You
  commit and push; I never do. I do not edit an existing test to make it
  pass without asking.

### Open items, in the order I would take them

1. **Level 3, option A: the Python `giant-squid` command is the Rust CLI,
   run inside the Python module** (decided by the user: "stick with a thin
   python wrapper and see how we go with the challenges"). Level 1 is done
   (diffs 23 to 27, see Status). The plan, one diff per step:
   - **Diff 27 (done):** the test imports of `parse_job_ids_only` moved to the
     library and the `cli::params` re-export deleted; the text of
     `submit-image-from-job` for a job ID now points to `--source-job-id`.
     The two image messages stay in the Rust binary's code (now
     `src/cli/run.rs`): the Python copies go with the Python command. The
     Python copy of the `submit-image-from-job` text and its pin in
     `tests/python/test_cli.py` were left as they are, because diff 29
     deletes them.
   - **Diff 28 (done):** the whole command is in the library
     (`src/cli/run.rs`, `run_cli(args) -> i32`); `src/bin/giant-squid.rs` is
     a caller of it. The module has the private function
     `mwa_giant_squid._run_cli(args)`, and `mwa_giant_squid_cli/native.py`
     is the launcher (it puts SIGINT back to its default action, then
     `sys.exit(_run_cli(["giant-squid", *sys.argv[1:]]))`). It is installed as
     the temporary command `giant-squid-native`, so it can be compared with
     the Python `giant-squid`, which is unchanged. The `python` feature now
     includes `bin`. The module no longer installs the pyo3-log bridge at
     import: `connect_python_logging()` installs it when the first
     `AsvoClient` is made or `from_env` is called.
   - **Diff 29:** `[project.scripts] giant-squid` points to the launcher
     (and `giant-squid-native` is removed); delete `mwa_giant_squid_cli`
     except the launcher, and its pytest files (`test_cli.py`,
     `test_cli_units.py`, and the parts of other files that use the old
     command) (the user approved the deletion); keep and extend
     `tests/python/test_native_cli.py`; docs.
   - **User decisions for this step (2026-10-02):** the plan is right; the
     Python tests of the old command may be deleted in diff 29; Ctrl-C is
     the native Rust behaviour (SIGINT reset to the default action); the
     entry point is private; the pyo3-log bridge is installed on first use.
   - **Considered, not chosen for now:** ship the native binary in a second
     PyPI package (maturin `bindings = "bin"`, built from the same
     `Cargo.toml` in its own directory with its own `pyproject.toml`),
     with `pip install mwa-giant-squid[cli]` pulling it in as a dependency
     pinned to the same version, and `pip install mwa-giant-squid` giving the
     library only. It avoids the logger, Ctrl-C and GIL work and keeps the
     library wheel small; it costs a second package and a second wheel per
     platform in CI, and `mwa-giant-squid` alone would have no
     `giant-squid` command. It was not built or tested. On 2026-10-02 PyPI
     returned 404 for `giant-squid`, `giant_squid` and `mwa-giant-squid`
     (names free at that time).
2. **Levels 2 (not chosen) and the old options for Level 3.**
   - Level 2, shared orchestration in the library: `wait_until_ready` with a
     state-change callback and `should_stop` (this reverses decision 9, "no
     poll loops in the library"); a submit-many call that returns a result
     per obsid; a download-many call; the dry-run text as a function; the job
     table as a string. With option A it is not needed: there is one CLI.
   - The other Level 3 options were: (B) export a machine-readable
     description of the clap CLI and build the argparse parser from it; (C)
     keep both commands and add a CI test that fails when their options or
     help texts differ.
   - Differences between the two commands that option A removes: the
     download labels (`[n/N]` runs across job IDs and obsids in Python and
     restarts for obsids in Rust), one login per download in Rust against a
     shared login in Python, `-vv` (trace) in Rust only, and when the limits
     of `list` are checked (before the login in Rust, after it in Python).
3. **`dug` for conversion and imaging.** The API developer says the schema
   is wrong and will add `dug` to `ConversionJobParams` and both imaging
   bodies; the generated `Delivery` type already has it, so `submit-conv`,
   `submit-image` and `submit-image-from-job` accept `--delivery dug`. The
   README already says conversion and imaging jobs can go to DUG. When
   the new schema arrives: run `tools/generate_openapi.sh` (or commit the
   schema and run `cargo build --features regen-openapi`), commit
   `openapi.rs` as it is written, and check that the `schema_enum!` list in
   `src/cli/value_enums/mod.rs` and the limits tests still pass. Ask the
   API developer whether a conversion job that was delivered to DUG can be
   imaged from: the README (the imaging section) and `V3_MIGRATION.md` say
   that only a conversion job delivered to Acacia or Scratch can.
4. **Try a wheel** against the real MWA ASVO and on an older HPC system
   (yours).
5. **PyPI**: check that `mwa-giant-squid` is free, then publish (a release
   workflow with trusted publishing, or by hand).
6. **Release 3.0.0**: set the date in `CHANGELOG.md`; the Python version
   follows `Cargo.toml`; a `v*` tag starts `releases.yaml`, which builds the
   tarballs and the wheels, makes the GitHub release and publishes the
   crate (it needs the `CARGO_REGISTRY_TOKEN` secret). The Linux arm64
   tarball job in that file is commented out.
7. **After 3.0.0**: remove `--legacy-json` (`src/cli/legacy_json.rs`, the
   options of `list` and `wait`, the README and migration text, the tests).
8. **API developer's answers**, below.

### For the API developer

Raised, not fixed:

- `JobDetailResponse.id` and `QueuedJob.id` should be `job_id`;
  `CalibrationReadyCallback.asvo_job_id` should be `job_id` (raised
  2026-09-30).
- `UserUpdateProfileRequest` has `firstname`/`lastname`; the other types have
  `first_name`/`last_name`.

Not yet answered or raised:

- Which response model does cancel return: `JobCancelledResponse` or
  `JobSubmittedResponse`? Our code parses the second (the fields match).
- What do the `error_code` values mean? They are undocumented.
- Queued jobs have `started` set a few milliseconds after `created`, but the
  field is documented as "when the job began processing". Should it be null
  until the job starts, or is the description wrong?
- The `job_params` of a listed conversion job has `flags: []`, which is not
  a parameter of the request (to be reported). The library never sends it,
  and passes the server's `job_params` through unchanged.
- What does `days: null` do? Nothing in giant-squid sends `null` now. It
  matters only if the API returns jobs older than 30 days for `null`:
  `download` and `wait` see only the last 30 days now.

Answered (kept for the record):

- `staging_count` and `RestageRequest` are for the processors: the library
  never sends them and does not wrap the processor endpoints (2026-10-02).
- The HTTP 200 for a refused cancel is only for a job that is already
  cancelled (2026-10-02).

### Release checklist (yours)

- The Python workflow ran on GitHub, and works (done).
- Try a wheel against the real MWA ASVO and on an older HPC system.
- PyPI: check the name, then publish. Decide if `releases.yaml` should
  also publish (it attaches the wheels to the GitHub release only).
- Set the date in `CHANGELOG.md`, tag, and watch `releases.yaml`.
- The crates.io package has no Python-only files (done); check
  `cargo package --list` before the tag.
- Still undecided from earlier: nothing. (The astroquery module is out of
  scope, and the stub drift check exists.)

### Working notes for the next session

- Set-up in a fresh sandbox: `apt-get install -y rustc-1.91 cargo-1.91
  rustfmt-1.91 rust-1.91-clippy python3-dev`; link `rustc`, `cargo`,
  `rustfmt`, `cargo-fmt`, `cargo-clippy` and `clippy-driver` from
  `/usr/lib/rust-1.91/bin` into `~/bin`; `export PATH=~/bin:$PATH
  CARGO_HOME=/home/claude/.cargo`; `pip install uv`; then `uv sync
  --locked`. `rustdoc` is not on the path, so doctests cannot run here.
- A command is limited to 300 s: start long builds with `setsid nohup`
  and poll. A full Rust build is 3 to 5 minutes. The disk is small: use one
  `CARGO_TARGET_DIR` for all clones, `CARGO_INCREMENTAL=0`, and delete
  `target/debug/incremental` and old clones when the disk fills up.
- Checks before a diff: `cargo fmt` (then `git checkout
  src/asvo/apiv2/openapi.rs`), `cargo test --locked`, `cargo clippy --locked
  --all-targets` (also with `--features python`), `uv run ruff format
  --check .`, `uv run ruff check .`, `uv run ty check`, `uv run pytest`,
  `tools/run_stubtest.sh`, and `tools/generate_stubs.sh` (the `.pyi` must not
  change unless a signature or a docstring in `src/python/` did).
- The README option tables and help blocks were generated from the built
  binary and the schema with a throwaway script that is not in the
  repository. Rebuild the binary and redo the help blocks if an option
  changes.
- Make each diff with `git diff` from a fresh clone of the latest `apiv2`
  (`git diff --cached -M` when files are renamed), write a base64 copy, and
  check that `git apply` works on another fresh clone and gives the tree
  that was tested.
- `tools/run_live_tests.sh` runs the live tests against test-asvo; it needs
  `MWA_ASVO_API_KEY`. The server allows 5 logins a minute.

## Status

Entries before 2026-10-02 name things as they were then. Since the module
layout change of 2026-10-02, `foo.rs` with `foo/test.rs` is
`foo/mod.rs` with `foo/tests.rs`, and `src/asvo/test.rs` and
`src/cli/test.rs` are `tests.rs`. A job's files are `AsvoJob.product.files`.

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
  `submit-meta` uses it. One new library test.
- 2026-09-29: `submit_download_vis_job` now forces `download_type = vis`,
  as `submit_download_meta_job` forces `meta` (both call a private
  `submit_download_job`). The existing metadata test now calls
  `submit_download_meta_job` (approved); its assertions are unchanged. One
  new library test.
- 2026-09-29: step 0.10 done. `examples/list_jobs.rs` uses only the
  public library API. It reads `MWA_ASVO_API_KEY`, `MWA_ASVO_HOST` and
  `HOME` itself, builds an `AsvoClientConfig`, lists jobs and uses
  `AsvoJobVec::filter`. It builds with `--no-default-features`. **Phase 0
  is complete.**
- 2026-09-29: step 0.10b done. README section "Using giant-squid as a
  Rust library" (dependency line, a compile-checked example, and a link to
  `examples/list_jobs.rs`).
- 2026-09-29: Phase 1 done. `python` feature (`pyo3` 0.29 with
  `abi3-py310`, `pyo3-log` 0.13), `pyproject.toml` (maturin), the module
  in `src/python/mod.rs` (`__version__`, `reset_logging()`), a
  `release-python` profile that unwinds on panic, a hand-written
  `mwa_giant_squid.pyi`, and `tests/python/test_module.py`. See the
  Phase 1 section for the differences from the first plan.
- 2026-09-29: step 2.1 done. `AsvoClient` (constructor and `get_jobs`),
  `AsvoJob`, `AsvoJobVec` (`len`, indexing, iteration, `filter`,
  `all_ready`, `json`), `AsvoFilesArray`, the enums `AsvoJobType`,
  `AsvoJobState` and `Delivery`, and the exceptions `AsvoApiError` and
  `AsvoError`. `editable-profile = "dev"`. 16 pytest tests against a local
  mock server (`pytest-httpserver`), including the log bridge.
- 2026-09-30: step 2.2 done. `AsvoClient` has `submit_download_vis_job`,
  `submit_download_meta_job`, `submit_conversion_job`,
  `submit_imaging_job`, `submit_image_from_job`, `submit_voltage_job` and
  `submit_beamformer_job`. Each returns `JobSubmittedResponse` (`job_id`,
  `message`, and `status` as the text "success" or "failed"). New enums:
  `DeliveryFormat`, `Output`, `Centre`, `OutputMode`, `Weighting` and
  `Polarization`; the existing `Delivery` is also the type of the
  `delivery` argument. `str()` of a member is the API value. A `None`
  argument is left out of the request builder, so the schema default
  applies. The Python layer adds no default. The request bodies are built
  in `src/python/params.rs`, one argument struct per job type; step 2.3
  reuses them for the `*_params` functions. The arguments are the ones the
  CLI has, under their OpenAPI names. Not arguments: beamformer `mode`,
  voltage `delivery_format`, and conversion `no_cable_delay` and `no_rfi`.
  `submit_voltage_job` sets `channel_range` when a channel bound is given,
  as the CLI does. `pol` is a `Polarization` for `submit_imaging_job` and a
  free-form `str` for `submit_image_from_job`, as in the schema. A bad
  obsid, a zero `source_job_id` or `nmiter`, or an `image_size` that the
  API does not accept raises `ValueError` before any request. (The range
  checks that this entry first left to the server moved into the library
  in step 2.2b, below.) 33 new pytest tests (the mock checks each request
  body), 49 in total.
- 2026-09-30: step 2.2b done (approved: move the common validation into
  the library). New module `src/asvo/apiv2/validate.rs` holds the limits
  of the imaging request bodies once: a `Bounds` type, one named constant
  per limited field (`MGAIN`, `NMITER`, ...) and `IMAGE_SIZES`.
  `validate_imaging_params` and `validate_image_from_job_params` check a
  request body, and `image_size`, `nmiter` and `source_job_id` give the
  typed values. `AsvoClient::submit_imaging_job` and `submit_image_from_job`
  call them, so an out-of-range body is never sent, whoever the caller is.
  A new variant `AsvoApiError::InvalidParameter { name, message }` carries
  the fault. The CLI's `clap` value parsers now use the same constants (the
  private `parse_f64_range` and `parse_i64_range` are replaced by
  `parse_f64_bounds` and `parse_i64_bounds`), and the Python builders call
  the same checks and raise `ValueError`; `InvalidParameter` also maps to
  `ValueError` if it reaches the exception conversion. All three show one
  message, for example `Invalid mgain: must be between 0.1 and 1 (got
  1.5)`. Each constant is hand-written, and a unit test compares every
  one with `openapi-schema.json`, so a regenerated schema with different
  limits fails the tests until the constant is changed. One limit is not in
  the schema: `clean_iterations` has no minimum there, and the CLI's
  minimum of 0 is kept (the test names this exemption). Small changes in
  behaviour: `NaN` is now rejected, and the message for a one-sided limit
  reads `must be at most 100`, not a range that ends at the largest float.
  Not checked, as before, although the schema has limits for them:
  conversion jobs (`avg_freq_res`, `avg_time_res`, `flag_edge_width`,
  `custom_centre_ra`, `custom_centre_dec`), `wstack_nwlayers` (32 to 512)
  and the voltage `offset` (0 to 5400). 16 new library tests (including
  the schema comparison), 4 new CLI and client tests, 28 new pytest tests
  (77 in total).
- 2026-09-30: step 2.2c done (approved: add the remaining schema limits
  to the library). New checks: `validate_conversion_params` (the same five
  limits as the imaging body: `avg_freq_res`, `avg_time_res`,
  `flag_edge_width`, `custom_centre_ra`, `custom_centre_dec`),
  `validate_voltage_params` (`offset`, 0 to 5400, constant
  `VOLTAGE_OFFSET`), and `wstack_nwlayers` (32 to 512, constant
  `WSTACK_NWLAYERS`) in the imaging body. `submit_conversion_job` and
  `submit_voltage_job` now check their body before sending, as the imaging
  submits do; the CLI (`submit-conv`, `submit-volt`, `submit-image`) and
  Python use the same constants. The image-from-job body is not checked for
  `wstack_nwlayers`, because the schema gives it no limit there; a test
  fails when the schema adds one (a point to raise with the API developer).
  A new test finds every `minimum` and `maximum` in the six job bodies of
  the schema and fails for any that is neither checked here nor enforced by
  the Rust type (`obs_id` by `Obsid::validate`, which is stricter;
  `source_job_id` by `NonZeroU64`; `duration` by `u64`; channels by `u8`).
  A negative `--offset` given as a separate value (`--offset -1`) is still
  refused by clap as an unknown argument, because `submit-volt` does not
  allow negative numbers; `--offset=-1` reaches the check. 6 new library
  tests (22 in total), 6 new CLI and client tests, 11 new pytest tests
  (88 in total).
- 2026-09-30: step 2.3 done. `AsvoClient.cancel_job(job_id)` returns
  `JobSubmittedResponse`; the argument is `job_id`, as in Rust (the table
  below said `jobid`; corrected). Module function
  `parse_many_jobids_or_obsids(strings)` returns `(jobids, obsids)`; text
  in a file that is not an integer raises `ValueError`, and a file that
  cannot be read raises the matching `OSError` (for example
  `FileNotFoundError`). Seven builders, named after the submit methods
  without `submit_` (`download_vis_job_params`, `download_meta_job_params`,
  `conversion_job_params`, `imaging_job_params`, `image_from_job_params`,
  `voltage_job_params`, `beamformer_job_params`), in
  `src/python/functions.rs`. Each returns the request body as a `dict` and
  makes no request. It uses the argument struct of its submit method, so it
  has the same defaults and checks; the download builders also set
  `download_type`, which the client sets when it submits. pytest tests
  check that each builder's `dict` is the body its submit method sends, and
  that each builder has the same signature as its method (pyo3 exposes the
  real signatures to `inspect`), because the two argument lists are written
  out twice in Rust. The `.pyi` stubs of the builders are copied from the
  submit stubs. 31 new pytest tests (119 in total). **Phase 2 steps 2.1 to
  2.3 are done.**
- 2026-09-30: step 2.3b done (approved). `submit-volt` has
  `allow_negative_numbers`, so `--offset -1` reaches the range check and
  gives its message, as the other submit commands do for negative values.
  `ParseError::IO` is now a struct variant with the `file` (a `PathBuf`)
  and the `source` IO error, and its message names the file; the CLI shows
  the path too (`/tmp/ids.txt: No such file or directory`). In Python,
  `parse_many_jobids_or_obsids` raises `OSError(errno, strerror, filename)`,
  which Python makes the subclass for the errno (for example
  `FileNotFoundError`) with `filename` set, as Python's own file functions
  do. One new library test, one new CLI test, one new pytest test (120 in
  total).
- 2026-09-30: step 2.4 done. Library: `DownloadOptions` has a new field
  `should_stop: Option<&dyn Fn() -> bool>`. The download asks it before
  each chunk and, in steps of at most 100 ms, while it waits to retry; when
  it returns `true` the download ends with the new `AsvoError::Interrupted`,
  which is never retried. The retry loop is now `retry_unless_stopped`
  (the `backoff::retry` rules, but with the wait cut into steps), so a stop
  does not wait for a back-off interval of up to a minute. A partial file
  stays on disk for a later resume. The library still reads no signal; the
  CLI passes `None`, so Ctrl-C ends the CLI process as before. The one
  line `should_stop: None` was added to the struct literals in the CLI, the
  README example and the `options()` helper of `src/asvo/test.rs` (no
  assertion changed). Python: `AsvoClient.download_jobid(jobid,
  download_dir, *, keep_tar=False, no_resume=False, hash=True,
  progress=None, buffer_size=None, retry_duration=None, download_number=1,
  download_count=1)` and `download_obsid(obsid, ...)` with the same
  keywords (`obsid` is the Rust name; the table below said `obs_id`;
  corrected). They run with the GIL released (`src/python/download.rs`).
  The library's `should_stop` hook runs `check_signals` at most every
  100 ms, so Ctrl-C raises `KeyboardInterrupt` at the next chunk when the
  call is on the main thread. The progress callback gets
  `DownloadProgress.Started`, `.Advanced` and `.Finished` (a pyo3 complex
  enum: each variant is a subclass, so `isinstance` and `match` work).
  `Advanced` events are combined and sent at most every 100 ms, with the
  byte total unchanged, because the library reports every chunk. An
  exception from the callback stops the download at the next check and is
  raised to the caller; it takes priority over the download's own error.
  `buffer_size`, `retry_duration` (seconds), `download_number` and
  `download_count` are the other `DownloadOptions` fields; `None` uses the
  library defaults. 3 new library tests (a stop, no stop, and a stop during
  a retry wait after one real retry); 16 new pytest tests (136 in total),
  including SIGINT sent from a timer thread during a slow download and
  during a retry wait. **Phase 2 is complete.**
- 2026-09-30: step 2.5 done (decision 10, approved). Names follow the
  OpenAPI schema everywhere, and types follow Rust naming (`Id`, not
  `ID`). Rust and Python: `AsvoJobID` is now `AsvoJobId`, `Obsid` is
  `ObsId` (module `obs_id`, error `ObsIdError`); `AsvoJob` fields are
  `job_id`, `obs_id`, `job_type` and `job_state`; `DownloadProgress`
  `Started.job_id`; `AsvoError` fields and Python attributes are `job_id`,
  `obs_id` and `job_state` (before, some kinds had `jobid` and others
  `job_id`); variants `NoObsId`, `NoJobReadyForObsId` and `TooManyObsIds`
  (the Python `kind` strings change with them). `download_jobid` and
  `download_obsid` are now `download_job(job_id, ...)` and
  `download_obs(obs_id, ...)`; `parse_many_jobids_or_obsids` is
  `parse_many_job_ids_or_obs_ids`, and `parse_jobids_and_obsids_from_file`
  is `parse_job_ids_and_obs_ids_from_file`; `AsvoJobVec::filter` takes
  `job_ids`, `obs_ids`, `job_types` and `job_states`, and `all_ready` takes
  `job_ids`. CLI: `--custom-centre-ra` and `--custom-centre-dec`
  (`submit-conv` and `submit-image`), `--centre` (`submit-image`),
  `--job-states` and `--job-types` (`list`); the old names
  (`--phase-centre-ra`, `--phase-centre-dec`, `--custom-ra`, `--custom-dec`,
  `--phase-center`, `--states`, `--types`) are hidden aliases, and the
  existing CLI tests, which use them, pass unchanged. Argument placeholders
  are `JOB_ID_OR_OBS_ID`, `OBS_ID` and `JOB_ID`. Not changed: the `list`
  and `wait` `--json` keys (`obsid`, `jobId`, `jobType`, `jobState`,
  `fileUrl`, ...), pinned by a new test until that is decided, and prose
  such as log messages and the table header "Obsid". The renames were made
  with a token-aware script, so strings and comments changed only where
  they name code. Tests changed only by the renames, no assertion changed;
  5 new CLI tests and 1 new library test.
- 2026-09-30: step 2.6 done (approved: option C, and `AsvoJobId` is a
  `u64`). `list --json`, `wait --json` and `AsvoJobVec::json` (Rust and
  Python) print the OpenAPI names: `obs_id`, `job_id`, `job_type`,
  `job_state`, `files`, `completed`, and `type`, `url`, `path`, `size`,
  `sha1` for each file. This also fixes the old key `jobType` for a file's
  delivery type. The values do not change (`DownloadVisibilities`,
  `Ready`, `{"Error": "..."}`). `list` and `wait` have `--legacy-json`,
  which prints the old format byte for byte (a golden test, whose text was
  captured from the old code) and a deprecation warning on stderr; it
  conflicts with `--json`. It is only in the CLI (`src/cli/legacy_json.rs`)
  and is to be removed, with the module, in the release after 3.0.0. The
  warning uses `eprintln!`, not the logger, because the CLI logger
  (`SimpleLogger`) writes every record to stdout, where a line would break
  the JSON for a script. `AsvoJobId` is a `u64`, as the schema's `job_id`
  is; the CLI no longer converts submitted job IDs to `u32` (the "doesn't
  fit" warnings are gone), and `check_file_sha1_hash` takes an
  `AsvoJobId`. The six test assertions on the old `--json` keys now check
  the new keys, as the approved change requires; no other assertion
  changed. The README JSON section is rewritten (it showed a `fileName`
  key that did not exist, and an `"Error: text"` state format that was
  wrong), and its `jq` recipe is fixed (`do` for `done`, and escaped
  `\$sha1` and `\$hash` that compared literal text). Also fixed: a missing
  `)]` in `src/cli/mod.rs` (`submit-meta`) in the pushed 2.5 commit, which
  stopped the crate from building. 3 new CLI unit tests, 1 new CLI test;
  the 2.5 key-pinning test now pins the new keys.
- 2026-09-30: step 2.7 done (approved). The CLI logs to stderr: both
  logger setups (`init_logger`, and `init_logger_with_progressbar_support`
  for `download`) use `simplelog`'s `WriteLogger` over `std::io::stderr()`,
  which writes each record in the same format as before. `SimpleLogger`
  sent every level except `Error` to stdout, so a warning (for example
  "non-default host") was mixed into the `--json` output. Now stdout has
  only a command's output: the job table, the `--json` jobs, and the
  `submit-* --json` responses. The `--legacy-json` warning is a normal
  `warn!` again (step 2.6 used `eprintln!` to avoid stdout). The
  `--dry-run` reports are log records, so they are on stderr now too. One
  new CLI test: the whole of `list --json` stdout parses as one JSON
  document, and the non-default-host warning is on stderr; it fails with
  the old logger.
- 2026-09-30: step 2.8 done: the code follows schema v1.11 (commit
  `38f7de0`). Changes in the schema and what they needed:
  `JobDetailResponse.product` is typed (`JobProduct`, `JobFile`) and
  `JobsByUserResponse.jobs` is `Vec<JobDetailResponse>`, so `get_jobs`
  reads each page as `RawJobsPage` (untyped jobs) and still runs
  `normalize_job_value` on each job before it parses it, and
  `product_to_files` maps the typed `JobFile`. `normalize_job_value` has a
  third, defensive fix: a `product` without `files` (for example `{}`) is
  treated as no product, because the typed `JobProduct` requires `files`
  and one such job would otherwise fail the whole listing. `JobFile` has a
  new field `format`, now in `AsvoFilesArray`, the Python `AsvoFilesArray`
  and the `--json` output (not in `--legacy-json`, which is unchanged).
  `download_type` is the shared `DownloadType` enum and is no longer an
  `Option`. Image-from-job (flow 2): `pol` is the `Polarization` enum
  (default `XXYY`; it was the string `"XX,YY"`), in the CLI (validated, as
  for `submit-image`) and in Python (`Polarization`, not `str`);
  `clean_threshold` has a default (0.001), so the CLI takes it from the
  schema, as for flow 1 (before, an unset value was sent as `null`);
  `wstack_nwlayers` has the limits 32 to 512, so it is a shared imaging
  limit and the CLI checks it. Tests: the two library tests and the CLI and
  Python tests that asserted the old flow 2 schema (no `wstack_nwlayers`
  limit, a string `pol`, no `clean_threshold`) are replaced or updated to
  the new schema; the JSON key tests have the new `format` key. 2 new
  client tests, 2 new pytest tests (138 in total). A new audit script
  compares each job body's schema fields with the CLI flags and the Python
  keyword arguments: every name matches, and the only fields with neither
  are conversion `no_cable_delay` and `no_rfi`, voltage `delivery_format`
  and beamformer `mode`, as decided before.
- 2026-09-30: step 2.9 done (approved). Added the schema fields that
  were missing. Conversion: `no_cable_delay` and `no_rfi` (CLI flags
  `--no-cable-delay`, `--no-rfi`; Python keywords). `AsvoJob` has every
  field of `JobDetailResponse`: `created`, `started`, `modified`,
  `error_text`, `user_id`, `first_name`, `last_name` and `job_params` (an
  untyped map, as in the schema), in Rust, Python (as properties;
  `job_params` is a `dict`) and the `--json` output (`--legacy-json` is
  unchanged). Python `AsvoJob.error_text` is now the server's
  `error_text`, not derived from the state; for an `Error` job it is the
  same message. `AsvoClient::get_jobs(&JobsFilter)` takes every
  `JobsByUserRequest` filter: `days`, `job_state`, `job_type`,
  `date_from`, `date_to` and `sort_by` (`limit` and `offset` stay
  internal: the client pages through all results). `job_state` and
  `job_type` are the library's `AsvoJobState` and `AsvoJobType`, converted
  to the API's values (`Ready` is `completed`; a type is its number);
  `Expired` and `Unknown`, which the API cannot filter by, are an
  `InvalidParameter` error before any request. The request is built with
  the generated builder, so unset fields take the schema defaults, except
  `days`, which is still sent as `null` when unset (the null-means-all
  assumption is still to be confirmed). Python: `get_jobs(days=None, *,
  job_state, job_type, date_from, date_to, sort_by)`; a datetime without
  a time zone is a `TypeError`. The CLI `list` still filters by several
  states and types on the client, and has no date filters. `AsvoApiError::
  ApiError` has `field_errors` (a `Vec` of the schema's `FieldError`) and
  `request_id`, also in its message (one line per field error, then the
  request ID; the message is unchanged when there are none) and as Python
  attributes (`field_errors` is a list of `{"field", "message"}` dicts).
  This makes `AsvoApiError` (and `AsvoError`) 144 bytes, over clippy's
  `result_large_err` limit of 128; a new `clippy.toml` sets the limit to
  160 with the reason, rather than boxing the fields. `AsvoJobVec` derives
  `Debug`. `tools/generate_openapi.sh` runs `cargo fmt` after it
  regenerates `openapi.rs`. Tests changed only where the new fields
  required it (struct literals, the JSON key and golden tests); one test
  comment changed. 6 new client tests, 1 new CLI test, 9 new pytest tests
  (147 in total).
- 2026-09-30: step 2.10 done (approved). Listing logic is in the library:
  `JobQuery` has the lists the API cannot filter by (several job IDs,
  obsids, job types and job states) and the server-side filters, and
  `AsvoClient::list_jobs(&JobQuery)` validates it (job IDs and obsids
  together are an `InvalidParameter`, before any request), sends a single
  supported type or state to the server, and applies the lists to the
  result (so `Expired` works too). The CLI's `list` and `wait` use it, and
  `list` has `--date-from`, `--date-to` (RFC 3339, or a date at midnight
  UTC) and `--sort-by`. Python: `AsvoClient.list_jobs(job_ids, obs_ids,
  job_types, job_states, *, days, date_from, date_to, sort_by)`. The files
  of a job are nested as in the API: `AsvoJob.product:
  Option<AsvoJobProduct>` with `files`, in Rust, Python
  (`job.product.files`; `AsvoJob.files` is gone) and `--json`
  (`"product": {"files": [...]}`); `--legacy-json` is unchanged. The job
  table no longer panics on a job with an empty file list. New
  `docs/V3_MIGRATION.md`, a guide for CLI users from 2.x (based on the
  2.5.1 README and source): its example commands were checked with
  `--dry-run`, and its `jq` recipes give the same output from the old and
  new JSON. New `tools/coverage.sh` (see docs/TESTING.md, "Code
  coverage"): Rust tests, then the Python tests against an instrumented
  extension module, with `cargo-llvm-cov` as in CI; reports in
  `coverage/` (git-ignored). A fixture makes the Ctrl-C tests install
  Python's SIGINT handler, because a process started in the background by
  a non-interactive shell starts with SIGINT ignored. 6 new library
  tests, 3 new CLI tests, 3 new pytest tests (150 in total).
- 2026-09-30: step 2.11 done (approved: option B). chrono is gone:
  it is soft-deprecated (chrono issue #1768), and the RustSec
  "unmaintained" advisory waits only for jiff 1.0. The crate uses
  `jiff::Timestamp` (jiff 0.2, without its time zone database features;
  every MWA ASVO time is UTC), and re-exports `mwa_giant_squid::jiff`. The
  generated `openapi.rs` has `Timestamp` for every `date-time` field: one
  `with_conversion` in `build.rs` (typify matches it without the schema's
  descriptions); the regenerated file differs from the old one only in
  that type. PyO3 uses its `jiff-02` feature, which converts as its chrono
  feature did (a UTC-aware `datetime` out; a naive one refused), so the
  Python API and its tests are unchanged. `cargo tree -i chrono` finds
  nothing for any feature set, and `Cargo.lock` has no chrono. The
  `--json` and `--legacy-json` times are unchanged (the byte-for-byte
  golden test passes as it was), and so is the token cache shared with
  mwa-cli: new tests show that jiff reads `Z`, `+00:00`, other offsets and
  fractional seconds, and writes RFC 3339 UTC with `Z`, as chrono did.
  `list --date-from` and `--date-to` still refuse a time without an offset
  (a test now covers this). Tests that compared chrono's `to_rfc3339()`
  text now compare the instants. jiff 0.2 is in the public Rust API, so
  jiff 1.0 will be a giant-squid major version for Rust library users (not
  for CLI or Python users). 3 new library tests, 1 new CLI test assertion.
- 2026-10-01: step 3.1 done. `mwa_giant_squid.pyi` is generated by
  pyo3-stub-gen 0.23.1 (feature `jiff-02`; default features off, plus
  `infer_signature` for the default values). New feature `python-stubgen`
  and binary `stub_gen`; `tools/generate_stubs.sh` runs it, then `ruff
  check --fix` and `ruff format` on the stub (pyo3-stub-gen writes
  `typing.Optional[X]`). The annotations are
  `cfg_attr(feature = "python-stubgen", gen_stub_...)`, as in mwalib, so
  the wheel and the published crate do not depend on pyo3-stub-gen or
  chrono. Override attributes (`#[gen_stub(...)]`) cannot be used with
  this pattern (see the Phase 3 notes), so three newtypes in
  `src/python/typed.rs` give the stub types: `JobIterator`
  (`typing.Iterator[AsvoJob]`), `JsonDict` (`dict[str, Any]`, for
  `AsvoJob.job_params` and the seven `*_params` builders) and
  `ProgressCallback` (`Callable[[DownloadProgress], object]`). At run time
  each is the object it wraps. The exception attributes are a manual
  `PyMethodsInfo` in `src/python/error.rs`. The docstrings that were only in
  the hand-written stub are now in the Rust doc comments, which are also
  the run-time `__doc__`; `download_job` and `download_obs` named their
  first argument `jobid` and `obsid` there (corrected). Lost:
  the `__version__` docstring (`module_variable!` has no doc). The
  generated stub differs from the hand-written one only in these ways:
  `Sequence[...]` for list arguments (pyo3 takes any sequence), `__new__`
  for `__init__`, `pathlib.Path` added to the path arguments, and
  `@typing.final` on the classes. mypy `stubtest` finds 41 errors (77 with
  the hand-written stub): 40 enum members, because a pyo3 enum is not an
  `enum.Enum` at run time, and `AsvoJobVec.__getitem__`, whose `index` is
  positional-only at run time (pyo3 does not permit `signature` on a magic
  method). Both are for a stubtest allowlist in the CI step. Clippy reports
  `incompatible_msrv` for the `TypeId::of` function pointers in the
  pyo3-stub-gen statics; it is allowed in `src/python`, for
  `python-stubgen` builds only. No test changed.
- 2026-10-01: fixes after step 3.1. `tools/generate_stubs.sh` failed
  with a uv-managed Python ("libpython3.14.so.1.0: cannot open shared
  object file"): `stub_gen` embeds Python and is linked against the
  `libpython` of the Python that pyo3 builds for, and uv keeps that library
  in its own directory, which the dynamic loader does not search. The
  script now sets `PYO3_PYTHON` to `uv python find` and puts that Python's
  `LIBDIR` on `LD_LIBRARY_PATH` (Linux only). Run the script, not `cargo
  run --bin stub_gen` alone. `cargo clippy --no-default-features
  --all-targets` failed in `src/asvo/apiv2/client/test.rs`, which uses
  `clap` and `crate::cli`: the imports, the `vis_params_from_cli` helper,
  `TARGET_ENV` and the eight tests that use them (seven that build a body
  from a command line, and the ignored recording test) now have
  `#[cfg(feature = "bin")]`. No test body or assertion changed; with the
  default features the same tests run. The `incompatible_msrv` allow
  (step 3.1) is a false positive: the `python-stubgen` library and
  `stub_gen` build with Rust 1.89 (older than 1.91, where `TypeId::of`
  became const-callable), and that `stub_gen` writes the same stub. No 1.88
  toolchain was available for the test.
- 2026-10-01: `stub_gen` runs on its own. `build.rs` (feature
  `python-stubgen` only) adds the library directory of pyo3's Python to
  the rpath of `stub_gen` (`cargo:rustc-link-arg-bin`), using
  `pyo3-build-config` (new optional build-dependency, same version as
  pyo3). So `cargo build --no-default-features --features python-stubgen`
  followed by `target/debug/stub_gen` works with a uv-managed Python, and
  `tools/generate_stubs.sh` no longer sets `LD_LIBRARY_PATH` (it still sets
  `PYO3_PYTHON`). `stub_gen` also sets `CARGO_MANIFEST_DIR` when it is not
  set: pyo3-stub-gen reads it when it runs, and only `cargo run` sets it.
  Why mwalib did not need this: its `stub_gen` build links without
  libpython (pyo3 has `extension-module`, and nothing in the binary needs
  a Python symbol). giant-squid's does need them (`Py_True`, `PyDict_Next`
  and others), so `pyo3/extension-module` must not be used here: the link
  fails with undefined symbols. Tested with Rust 1.89 and a uv-managed
  Python 3.14.4: `NEEDED libpython3.14.so.1.0` with a `RUNPATH` to uv's
  `lib` directory; run from another directory it writes the same
  `mwa_giant_squid.pyi` as the committed one (after ruff).
- 2026-10-01: step 3.2 done. `docs/PYTHON.md` is the user guide for the
  Python module: install, authentication (the caller reads the
  environment; `token_cache_path` and the shared `tokens.json`), listing,
  the submit methods and the `*_params` dry-run functions, the caller's
  wait loop, downloads (progress with `match`, resume, Ctrl-C), errors,
  logging, threads and limits. `readme` in `pyproject.toml` is now
  `docs/PYTHON.md`, so the PyPI page describes the Python module. The
  links in the guide are absolute, because PyPI does not resolve relative
  links. Each code example was run against the pytest mock server (in a
  scratch test that is not committed); the guide's `ApiError` example
  also shows that `field_errors` and `request_id` exist only for that
  `kind`. No code and no test changed. The guide says `pip install
  mwa-giant-squid`, which works only after the first PyPI release.
- 2026-10-01: step 3.3 done (revised: the example CLI is now the installed
  `giant-squid` command; decision 7 changed). New package
  `mwa_giant_squid_cli/` at the repository root, listed in `pyproject.toml`
  as `python-packages` and as `[project.scripts] giant-squid`. maturin
  puts it in the wheel and the sdist next to the unchanged native module
  `mwa_giant_squid` (no `python-source`; the module keeps its name, its
  stub and `py.typed`). Checked: a wheel built with maturin installs the
  command in a clean venv. The program has the Rust command's 11
  sub-commands and short names (`l`, `d`, `sv`, `sc`, `si`, `sifj`, `sm`,
  `st`, `sb`, `w`, `c`), options, short options, aliases, environment
  variables (`MWA_ASVO_*`, `GIANT_SQUID_DELIVERY`,
  `GIANT_SQUID_DELIVERY_FORMAT`, `GIANT_SQUID_BUF_SIZE`,
  `GIANT_SQUID_DOWNLOAD_RETRY_SECS`), log lines (`HH:MM:SS [INFO] ...` on
  stderr), job table, `--json` and `--dry-run` output, and exit codes. The
  option defaults are taken from the module's `*_params` functions, as the
  Rust command takes them from the schema. A list of the options of each
  command was compared with `giant-squid <command> --help` of the Rust
  binary, and the output of about 35 invocations (stdout, stderr without the
  time, exit code) was compared with the Rust binary running against the
  same mock server. The differences are: no `--legacy-json` (deprecated);
  the boolean options `--apply-di-cal`, `--apply-primary-beam` and
  `--join-channels` take a value only as `--flag=false`, as in Rust (a
  small step turns a bare flag into `--flag=true`, because argparse would
  otherwise take the next argument); `-vv` is the same as `-v`; the
  range errors have the module's text but the same exit code, 2; the
  downloads share one login and one client; a download with
  `--concurrent-downloads` above 1 runs in daemon threads, and Ctrl-C
  ends the program with exit code 130 (a single download runs in the main
  thread and the module stops it at the next chunk); the progress bars are
  drawn with ANSI codes and need no package (shown on a terminal only);
  `--job-states`/`--job-types` take the names `downloading` and
  `preparing` that the Rust parser takes (its help text still lists
  `retrieving`, which it does not parse: a bug in the Rust help). The
  download numbers `[n/N]` run on across job IDs and obsids (the Rust
  command restarts at 1 for the obsids). 113 new pytest tests (81 in
  `tests/python/test_cli.py`, 32 in `test_cli_units.py`; the suite is 263).
  They run `main()` in the process against the mock server, with a few
  subprocess tests (`python -m`, and Ctrl-C during two concurrent
  downloads). The suite also passes on Python 3.10 with the built wheel,
  except one existing test (below). The earlier step 3.3 diff (an
  `examples/python/asvo_cli.py` example and its test) was not committed
  and is replaced by this one. `docs/PYTHON.md` has
  a section on the command and the README has a pip section. The package
  has no Python dependencies. Not changed: the Rust code and the existing
  tests. Found, not fixed: `tests/python/test_client.py::
  test_get_jobs_sends_every_filter_to_the_server` fails on Python 3.10,
  because `datetime.fromisoformat` there does not read a trailing `Z`
  (Python 3.11 and later do). The CI matrix (step 3.4) will run 3.10.
- 2026-10-01: step 3.4 done. `.github/workflows/python.yaml` (one file, four
  jobs; it runs on the same triggers as `run-tests.yaml`). `check`: `uv sync
  --locked`, `ruff check`, `ruff format --check`, `ty check` and stubtest.
  `test`: `uv run pytest` on Python 3.10 and 3.14 for each of the four
  `run-tests.yaml` platforms (ubuntu x86_64 and aarch64, macOS Intel and
  Apple silicon), and on 3.11, 3.12 and 3.13 on Linux x86_64 (the wheel is
  abi3, so it is the same module for each). `wheels`: `PyO3/maturin-action`
  builds a release wheel on each of the four platforms (Linux in a manylinux
  container, `manylinux: auto`), installs it in a new environment without
  the source and runs `giant-squid --version`, then uploads it as an
  artifact. `sdist`: builds the sdist and uploads it. Nothing is published:
  PyPI stays your step. Stubtest: `tools/run_stubtest.sh` runs
  `mypy.stubtest mwa_giant_squid` with mypy pinned (2.3.1) and
  `tools/stubtest_allowlist.txt`. With mypy 2.3.1 there are 44 findings, not
  41: the 40 enum members and `AsvoJobVec.__getitem__` as before, plus
  three that this mypy adds or that were not counted: the `mwa_giant_squid.
  mwa_giant_squid` submodule that has no stub of its own, `__all__` (the
  run-time `__all__` has `__version__`, the generated one does not), and
  `DownloadProgress` (`@disjoint_base`, which mypy asks for since 1.19 and
  pyo3-stub-gen does not write). Each entry in the allowlist has the reason
  as a comment, and stubtest fails on an entry that matches nothing. The one
  existing test that failed on Python 3.10 (`test_get_jobs_sends_every_
  filter_to_the_server`) is fixed: the test reads the `Z` of an RFC 3339 time
  with a small helper `parse_rfc3339`, and no assertion changed. Checked
  here: the whole suite (263 tests) passes on Python 3.10, 3.11, 3.12, 3.13
  and 3.14; `ruff`, `ty` and stubtest pass; `uv sync --locked` works with
  the committed `uv.lock`; `actionlint` 1.7.12 finds nothing in the workflow.
  Not checked, because it needs GitHub: the workflow run itself, the
  manylinux build (`aws-lc-sys` in the container), the macOS and aarch64
  runners, and the action versions (`astral-sh/setup-uv@v10.0.1`,
  `PyO3/maturin-action@v1`, `actions/upload-artifact@v4`). The crate on
  crates.io still includes `mwa_giant_squid_cli/` and `tests/python/`
  (`Cargo.toml` `exclude` has only `.github/*`); add them to `exclude` if
  you do not want that.
- 2026-10-01: API change 1 of 4 (new `openapi.rs` and `openapi-schema.json`,
  commit `ed3c063`; they are the same schema, and the build is broken until
  this step). The new schema: `JobsByUserRequest.days` is `NonZeroU64`
  (minimum 1, maximum 30; the API developer fixed the earlier
  `exclusiveMinimum: 1`), and every job body has an optional
  `staging_count` (processor only), `JobDetailResponse` has `error_code`,
  the conversion `delivery_format` default is `tar` (was `files`), and
  there is a new `RestageRequest` (processor only). Decision: the library
  has no unused API calls, so `staging_count` and `RestageRequest` are
  not used and not exposed (the generated types keep them; nothing sets
  `staging_count`, and it is not sent when `None`). This step: `days`.
  `validate::DAYS` (1 to 30) and `validate::days(i64)`. `get_jobs` and
  `JobQuery::validate` refuse a `days` outside the limits with
  `InvalidParameter` before any request (so `list_jobs` and Python
  `ValueError` do too). `JobsFilter.days` and `JobQuery.days` stay `i64`,
  like the other validated numbers. The CLI's `--days` uses
  `parse_i64_bounds(validate::DAYS)`, so clap refuses it at parse time. The
  Python command reports it as a usage error after the login (the module
  has the check, and the program does not repeat the limits). Tests: one
  client test, two validate tests (one compares `DAYS` with the schema's
  `JobsByUserRequest.days`), one CLI test, three pytest tests and one
  Python command test (7 new pytest tests; 270 in total; 180 Rust unit
  tests). `tests/live.rs` has `live_list_without_days_probe`, which checks
  the open question below as far as an account's jobs allow and prints a
  verdict. The stub docstrings are regenerated and stubtest passes. Still
  not confirmed: that `days: null` (what `list` sends without `--days`)
  means no limit, not 30. Left for the other steps: `error_code` (step 2,
  with the failed-job message), the `delivery_format` default in the docs
  (step 3), and the status notes (step 4). Still open with the API
  developer: `JobDetailResponse.id`, `QueuedJob.id` and
  `CalibrationReadyCallback.asvo_job_id` (should be `job_id`), `firstname`
  and `lastname`, the cancel response model, and what `error_code` means.
- 2026-10-01: API change 2 of 4 (`error_code`, schema 1.12.2). `AsvoJob`
  has `error_code: Option<i64>` (before `error_text`), from
  `JobDetailResponse.error_code`. The library passes it on and does not
  interpret it: the schema does not document its values. It is in the
  `list --json` and `wait --json` output (key `error_code`, null when the
  server gives none), in the Python `AsvoJob.error_code`, and in
  `AsvoError::JobFailed { error_code }`, whose message reads `has an error
  (code N): <error_text>` when there is a code, and as before when there is
  none (decided without the user's answer on the exact wording: it is one
  small function, `error_code_suffix` in `src/asvo/error.rs`, to change).
  `AsvoJobState::Error(String)`, the job table and the `wait` log lines
  show the error text only, as before. The Python `AsvoError` has the
  attribute `error_code` for kind `JobFailed` (`int | None`). A job JSON
  without an `error_code` key reads as `None` (serde). The `--legacy-json`
  output is unchanged. Tests: one types test (the message, with and
  without a code), one client test extended (every field reaches
  `AsvoJob`), one asserted in the errored-job test, the key list and the
  golden JSON in the existing tests, one CLI test (`wait` shows the code),
  pytest assertions in the existing client and CLI tests. No existing
  assertion changed except the key lists and the golden JSON string, which
  gain `error_code`. The stub is regenerated and stubtest passes. Not done,
  by decision: nothing uses `staging_count` or `RestageRequest`. Still open
  with the API developer: the meaning of `error_code`, and the `status` of
  a cancel (a refused cancel returns 200 with `status: failed`).
- 2026-10-01: API changes 3 and 4 (`status`, README, docs). Decisions from
  the API developer: the response `status` ("success" or "failed") is
  descriptive text, like `message`, and callers must use the HTTP status,
  which the client already did. It stays exposed, documented as for
  display only: the Python `JobSubmittedResponse.status` docstring, the
  Rust `AsvoClient::cancel_job` doc, `docs/PYTHON.md` and
  `docs/V3_MIGRATION.md` say so. Cancel: the API developer says a refused
  cancel (for example, of a job that is already cancelled) stays HTTP 200
  with `status: failed` and a message such as "Unable to cancel job N".
  So the client cannot tell such a refusal from success, except by the
  message; `cancel` logs `Cancelled MWA ASVO job ID N (<message>)` and
  `Cancelled 1 jobs.` for it, as before. The Python `cancel_job` and the
  Rust `cancel_job` docs now say this. `live_cancel` is changed to match:
  its second cancel expects the server's message (the new constant
  `CANCEL_REFUSED_MESSAGE`) and not a structured error. README: the
  "Submit MWA ASVO jobs" sections are rewritten (they were the 2.x text,
  with the `-p/--parameters` help and key/value tables). Each submit
  command has its real 3.0 `--help` block, a table of the options that
  every submit command has, and an options table for `submit-conv` and
  `submit-image` (the meaning, the allowed values from the schema, and the
  default from the real help). There is a new section for
  `submit-image-from-job`. The delivery notes say that the default
  delivery format is `tar` (use `--delivery-format files` for individual
  files), that `GIANT_SQUID_DELIVERY_FORMAT` sets it, and that only
  visibility, metadata and beamformer jobs can go to DUG. The dry-run
  example and the `download` help block are the real output. The tables
  and help blocks were generated from the built binary and the schema
  with a throwaway script (not committed), so they match this commit. In
  `V3_MIGRATION.md` the conversion table gains the delivery format row
  (inferred from the 2.x README, which said 2.x gave individual files for
  Scratch and DUG unless asked for `tar`). Not done: README sections for
  `wait` and `cancel` (there are none). Found, not fixed: the schema offers
  only `acacia` and `scratch` for conversion and imaging jobs, but the
  generated `Delivery` type has `dug` too, so `submit-conv`, `submit-image`
  and `submit-image-from-job` accept `--delivery dug` and the server would
  refuse it. The `--delivery` help does not list the values (nor do
  `--output`, `--centre`, `--pol` or `--weighting`).
- 2026-10-02: CI and packaging. A `stubs` job in `python.yaml` runs
  `tools/generate_stubs.sh` and fails if `mwa_giant_squid.pyi` differs.
  `releases.yaml` builds the four wheels and attaches them to the GitHub
  release. The crates.io package leaves out every Python-only file
  (`pyproject.toml`, `uv.lock`, the stub, the Python docs and tools, the
  Python command and its tests); `src/python` and `src/bin/stub_gen.rs`
  stay, because the manifest needs them. maturin builds the sdist from the
  crate's file list, so leaving the stub out of the crate also left it out
  of the sdist, and a wheel built from that sdist had no stub and no
  `py.typed`. `pyproject.toml` adds the stub to the sdist (`include`,
  `format = "sdist"`); a wheel built from that sdist has both. The comments of `tools/generate_stubs.sh` now say what it
  does (it sets no `PYO3_PYTHON`; it runs from the repository root).
- 2026-10-02: `cancel` logs `Cancel request for job N: <message>` and
  `Cancel requests: N sent, M failed.` in both commands. The assertions that
  checked the old text changed (approved). A refused cancel of a job that is
  already cancelled is HTTP 200 with `status: failed`; any other refusal is
  a 4xx error (API developer).
- 2026-10-02: `--help` lists the allowed values of `--delivery`,
  `--delivery-format`, `--output`, `--centre`, `--output-mode`, `--pol` and
  `--weighting`, from a clap value parser for the schema enums
  (`src/cli/value_enums/`); the `schema_enum!` macro fails to compile when
  the schema gets a value that the list lacks. The Python command lists the
  same values. The help of `list --job-states` and `--job-types` named
  `retrieving` (not a state) and left out two types; both fixed, with tests
  that each name parses.
- 2026-10-02: `AsvoJobType::from_str` returns `InvalidJobType` for text that
  is not a job type (it returned `Unknown`), and accepts `download_voltage`
  (approved).
- 2026-10-02: README sections for `wait` and `cancel`; the CHANGELOG entry
  of 3.0.0; `V3_MIGRATION.md` is final (the **(check this)** markers are
  gone, and it has the cancel, `wait` and `--job-types` changes).
- 2026-10-02: `wait` and `cancel` refuse an obsid with
  `Expected only job IDs, but found these obsids: ...` and send nothing
  (`parse_job_ids_only`; the Python command has the same). 2.x ignored the
  obsid without a word. One assertion of `test_wait_needs_a_job_id` changed
  with it.
- 2026-10-02: `openapi-drift-check` was failing for a reason that had
  nothing to do with the schema: it ran `cargo clippy --fix` and `cargo
  fmt` after the regeneration, and rustfmt rewrites about 1,500 lines of the
  file that `build.rs` (prettyplease) writes. It only regenerates and
  compares now, and `tools/generate_openapi.sh` no longer formats. Its name
  contains the job id, so it can be found in the Actions page.
- 2026-10-02: `test_the_short_command_names_work` compared stderr with the
  clock time of its last line, so it failed when two runs fell in different
  seconds (the macOS Intel runners). It compares `log_lines()` now
  (approved).
- 2026-10-02: `list` without `--days` sends the schema default for `days`
  (30), not `null`; the CLI shows `[default: 30]`, read from the generated
  type. Two tests that pinned `null` changed (approved). `wait` and
  `download` list jobs the same way.
- 2026-10-02: module layout. Seven modules (`helpers`, `obs_id`,
  `asvo/token_store`, `asvo/types`, `asvo/apiv2/client`,
  `asvo/apiv2/validate`, `cli/value_enums`) moved from `foo.rs` to
  `foo/mod.rs`, and every `test.rs` is `tests.rs` (`mod tests;`). Pure
  renames; test ids read `...::tests::...`.
- 2026-10-02: a lost Ctrl-C. On the aarch64 Python 3.10 runner,
  `test_ctrl_c_stops_the_wait_before_a_retry` got `AsvoError: HTTP error
  500`, the error that a download returns when its whole retry window has
  run out. Python runs a pending SIGINT handler at the next bytecode of the
  main thread, and the Rust code makes Python calls of its own: every log
  record goes through `logging` (pyo3-log). When the signal landed in one
  of those calls, the `KeyboardInterrupt` left the log call, pyo3-log left
  it as the thread's current exception, the signal was spent, and
  `check_signals` found nothing. `take_stray_error` in `src/python/download.rs`
  now takes such an exception (a `BaseException` stops the download and is
  raised; an ordinary `Exception` is cleared). The slow runner made the
  window big because the CLI tests had left their log handler on the root
  logger, writing to a closed stream, so every record printed a traceback;
  `main()` now removes its handler. Two new tests, one of which was red
  before the fix.
- 2026-10-02: `staging_count`, the processor endpoints and `flags`.
  Nothing in the library called the endpoints or set `staging_count`; the
  decision is now recorded in the module docs of `src/asvo/apiv2/mod.rs`
  and pinned by tests: a recording client test (the endpoints, and no
  `staging_count` on the wire), a CLI test (no option, no key in a default
  body) and Python tests (no argument, no key). Another pair of tests
  checks that every field of every request body is a property of that
  endpoint's schema, which is what keeps `flags` out.
- 2026-10-02: live tests. All 19 passed against test-asvo. The probe
  `live_list_without_days_probe` was removed (18 remain): it asked what
  `days: null` does, and nothing sends `null` now.
- 2026-10-02: documentation. `docs/TESTING.md` has the Python test layer, the
  list of tests that pin a decision, and corrected paths and counts;
  `docs/PYTHON.md` has the Ctrl-C and logging notes and the `days` default;
  the README says conversion and imaging jobs can go to DUG (ahead of the
  schema fix); this file's handoff is rewritten.
- 2026-10-02 (thin clients): review of the two commands. Together the Rust
  command (about 2,590 lines) and the Python command (about 2,040) share 37
  identical message strings, and Python copied library constants by hand
  (the 7 endpoint paths, the default host, the token cache path, the wait
  times). It also copied the job state and type names, the enum text
  conversion, `parse_utc_time`, the environment variable handling, the wait
  loop, the submit-each-obsid loop, the dry-run text and the jobs table. No
  command rewrote the body of an API error; the library captured the API's
  `detail` and `suggestion` and never showed them. The rewrites found are the
  R1 to R7 of the decisions above. Levels 1 to 3 of the plan are in Open
  items.
- 2026-10-02 (diff 23): the text of an API error is the server's: the error
  code and message, then `Detail:`, `Suggestion:`, the field errors and the
  request ID, each on a line of its own, and only the parts the server gave.
  Both commands and `str(AsvoApiError)` show it. Two Rust assertions that pinned
  the old text changed (approved by the decision above).
- 2026-10-02 (diff 24): the environment, once, in the library. New module
  `src/asvo/env/`: the `ENV_*` names, `client_config_from_env`,
  `DownloadSettings::from_env`, and the error
  `AsvoError::InvalidEnvironment { name, value, problem }`. New library
  constants `DEFAULT_CONCURRENT_DOWNLOADS`, `WAIT_POLL_INTERVAL` and
  `WAIT_INITIAL_DELAY`, and the endpoint constants at the crate root. The
  Python module has `AsvoClient.from_env()`, `DownloadSettings.from_env()` and
  the constants (the endpoint paths, the two delivery variable names, the wait
  times in seconds, the concurrent downloads default). `cli/config.rs` and
  `mwa_giant_squid_cli/config.py` are deleted. R7: one phrasing. Behaviour that
  changed: a bad `GIANT_SQUID_DOWNLOAD_RETRY_SECS` is a warning in both
  commands (the Rust command ignored it), and a negative
  `GIANT_SQUID_BUF_SIZE` is refused in both with the same message. Two Python
  tests changed only to read the wait times from the module. The library reads
  the environment only when asked.
- 2026-10-02 (diff 25): the names of the job states and job types, once, in
  the library (`src/asvo/types/mod.rs`): `AsvoJobState::names()` and
  `AsvoJobType::names()`, and `FromStr` reads the same tables (`download_voltage`
  stays accepted and is not listed). The help of `list --job-states` and
  `--job-types` in the Rust command is built from `names()`; the Python module
  has `names()` and `parse(text)` on both enums and `AsvoJob.state_text`
  (`Error: <message>`). Deleted from `mwa_giant_squid_cli`: `JOB_STATE_NAMES`,
  `JOB_TYPE_NAMES` and `state_text`; `name_list_parser` now takes the enum and
  calls the module. R4: `Invalid job state 'x': expected one of: ...`. Two
  existing pytest tests (`test_names_ignore_case_and_punctuation`,
  `test_every_job_state_and_type_can_be_named`) were rewired to the new API
  with the same assertions (needs the user's approval). The help now ends
  `imaging, cancel_job` (no "or"). `enum_parser` (the schema enums in the
  Python command, `choose from`) is not changed: open question for the user.
- 2026-10-02 (diff 26): the ID guards and the time parser, once, in the
  library (`src/helpers/mod.rs`). New: `parse_obs_ids_only`,
  `parse_job_ids_only`, `parse_utc_time`, the `ParseError` variants
  `JobIdsGiven`, `ObsIdsGiven`, `NoObsIds`, `NoJobIds` and `InvalidTime`, and
  the constant `OBS_ID_HINT`. The Rust binary has one helper (`obs_ids_only`)
  in place of seven copies; Python has the same three functions in the module
  (a parse `ValueError` has the attribute `kind`) and
  `JobSubmittedResponse.json()`. Deleted from `mwa_giant_squid_cli`: the
  copied time parser, `job_ids_only`, `OBS_ID_HINT`, `DATE_ONLY_FORMAT` and the
  hand-built submit JSON. The typo is fixed: `found these job IDs: [..]`
  (the list keeps its `[..]` form). R2: `--image-size` shows the library's
  full text (`Invalid image_size: ...`). Python `parse_utc_time` now uses
  jiff's rules, not `datetime`'s. One existing pytest assertion changed
  (`test_cli.py`, the typo text, approved by "fix in next diff").
- 2026-10-02 (diff 27): in `src/cli/tests.rs` the five tests of
  `parse_job_ids_only` import it from `crate` (approved by the user), and the
  re-export in `cli::params` is deleted. `IMAGE_FROM_JOB_JOB_IDS_MESSAGE` in
  the Rust binary now reads `The arguments must be obsids, not job IDs. Give
  the conversion job with --source-job-id.` (the old text could be read as
  saying that the command takes no job ID, but it needs one as
  `--source-job-id`). New CLI test:
  `a_job_id_argument_of_submit_image_from_job_points_to_source_job_id`. The
  Python command is not changed. The user chose option A for Level 3.
- 2026-10-02 (diff 28): the command moves from `src/bin/giant-squid.rs` to
  `src/cli/run.rs` (`pub fn run_cli`, which parses with `Args::try_parse_from`,
  prints clap's text and returns its code for a bad argument, prints
  `Error: {error:?}` and returns 1 for an error, and flushes standard output
  and error before it returns). The two logger set-up functions keep an
  installed logger and only change the level, instead of `unwrap()`, so a
  second run in one process does not panic (the second run's progress bars
  are not wired to the logger of the first). The module has `_run_cli`
  (the GIL is released while it runs); `python` now includes `bin`;
  `pyproject.toml` has the temporary script `giant-squid-native`. Tests: 2 new
  Rust unit tests (exit codes 0 and 2; the exit code 1 is covered by the CLI
  tests, which run the program) and 7 new pytest tests (346 in total) in
  `tests/python/test_native_cli.py` (help, version, usage error, command
  error, logger, `list` against the mock, SIGINT). No existing test was
  changed.
- Next step: see "Handoff (read this first)" at the top of this file.

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
    print(job.job_id, job.obs_id, job.job_type, job.job_state)
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
7. The Python package installs a `giant-squid` command (changed
   2026-10-01; before, it installed none). It is written in Python on the
   module, with the same commands, options, defaults and output as the
   Rust command (step 3.3).
8. The in-process tests may change how they build the client (from an
   `AsvoClientConfig` instead of environment variables). Their assertions
   stay the same.
9. Library functions do not implement poll loops (added 2026-09-29).
   The library gives single checks; the caller does the loop and the
   sleep. This keeps Ctrl-C, timeouts and progress output in the
   caller's hands.
10. Names follow the OpenAPI schema (added 2026-09-30): `job_id`, not
    `jobid`; `obs_id`, not `obsid`; `job_type` and `job_state`. This
    applies to the Rust library, the Python module, the CLI flags and the
    JSON output. Rust type names follow Rust naming (`AsvoJobId`, `ObsId`).
    Where the schema is itself inconsistent (for example `id` for a job ID
    in `JobDetailResponse`), the majority name is used and the
    inconsistency is raised with the API developer.

Also kept from the first draft: optional job arguments default to `None`,
meaning "use the OpenAPI schema default", so neither layer adds defaults of
its own; Python >= 3.10 with one `abi3` wheel per platform; maturin;
`pyo3-stub-gen` for `.pyi` stubs (as mwalib); pyo3 0.29.

## Naming in Python

Rust names are kept for types, methods, fields and enum variants
(`AsvoClient`, `get_jobs`, `submit_download_vis_job`, `AsvoJob.job_id`,
`AsvoJobState.Ready`). Rust method and field names are already snake_case,
and CamelCase enum variants are valid Python.

Exceptions: Python practice is that an exception name ends in `Error`. The
two Rust error enums already do, so Python gets two exception classes with
the same names, `AsvoApiError` and `AsvoError`. Each has a `kind` attribute
with the Rust variant name (for example `"ApiError"`, `"MissingAuthKey"`)
plus that variant's fields (`error_code`, `message`, `detail`,
`suggestion`, `code`). This keeps every name identical to Rust.

A job argument that is outside what the schema allows is not an API failure:
it is found before any request. In Rust it is `AsvoApiError::InvalidParameter`;
in Python it is a `ValueError`, as Python practice suggests for a bad argument.

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

## Phase 1: Python module skeleton (done)

- `Cargo.toml`: `python` feature (`pyo3` 0.29 with `abi3-py310`, and
  `pyo3-log` 0.13). A `release-python` profile inherits `release` but sets
  `panic = "unwind"`: pyo3 turns a Rust panic into a Python
  `PanicException`, but the `release` profile's `panic = 'abort'` would stop
  the whole Python process.
- `pyproject.toml`: maturin backend, `module-name = "mwa_giant_squid"`,
  `no-default-features = true` and `features = ["python"]` (so the CLI is
  not in the wheel), `profile = "release-python"`, version from
  `Cargo.toml`, `requires-python = ">=3.10"`. Dev tools (`pytest`, `ruff`,
  `ty`) are in the `dev` dependency group: `uv sync`, then `uv run pytest`.
- `src/python/` holds all binding code, behind `#[cfg(feature = "python")]`.
- Rust `log` output goes to Python `logging` through `pyo3-log`. Logger
  names are the Rust module paths with `.` for `::`, so `mwa_giant_squid`
  is their parent. Records below `DEBUG` (Rust `trace`, which includes
  request and response bodies with tokens) are not sent to Python.
  `reset_logging()` clears `pyo3-log`'s cache of logger levels after the
  caller changes its logging configuration.
- `mwa_giant_squid.pyi` (hand-written for now) gives `ty` and editors the
  module's types; maturin puts it in the wheel with `py.typed`.

Differences from the first plan, found while checking the current
documentation:

- No `crate-type = ["rlib", "cdylib"]`. maturin passes
  `--crate-type cdylib` itself, so a normal `cargo build` makes no shared
  library.
- No `extension-module` feature. It is deprecated in pyo3 0.29; maturin
  1.9.4 and later set `PYO3_BUILD_EXTENSION_MODULE` instead. As a result,
  `cargo test --features python` links and runs.
- The `python-stubgen` feature moves to Phase 3, where stubs are generated.

## Phase 2: Python API

`AsvoClient(host, api_key, *, api_timeout=None, token_cache_path=None)`.
With `token_cache_path=None` the session is kept in memory only. A script
that runs often should pass a path, because the server allows only a few
logins a minute.

| Python (same name as Rust) | CLI command |
|---|---|
| `get_jobs(days=None, *, job_state=None, job_type=None, date_from=None, date_to=None, sort_by=None) -> AsvoJobVec` | `list` |
| `submit_download_vis_job(obs_id, *, delivery=None, delivery_format=None, allow_resubmit=None)` | `submit-vis` |
| `submit_download_meta_job(obs_id, ...)` | `submit-meta` |
| `submit_conversion_job(obs_id, *, ...)` | `submit-conv` |
| `submit_imaging_job(obs_id, *, ...)` | `submit-image` |
| `submit_image_from_job(obs_id, source_job_id, *, ...)` | `submit-image-from-job` |
| `submit_voltage_job(obs_id, offset, duration, *, ...)` | `submit-volt` |
| `submit_beamformer_job(obs_id, *, ...)` | `submit-bf` |
| `cancel_job(job_id) -> JobSubmittedResponse` | `cancel` |
| `get_jobs()` then `AsvoJobVec.all_ready(jobids)`, in a loop the caller writes | `wait` |
| `download_job(job_id, download_dir, *, keep_tar=False, no_resume=False, hash=True, progress=None, ...)` | `download` |
| `download_obs(obs_id, ...)` | `download` |

Keyword names are the OpenAPI field names (for example `avg_freq_res`), so
they match the schema and the Rust builder methods.

Module functions: `parse_many_job_ids_or_obs_ids`, and one `*_params`
builder per job type that returns the request body as a `dict` (for a
caller's own dry run). A builder's name is its submit method's name without
`submit_`, plus `_params` (for example `imaging_job_params`).

Types: `AsvoJob`, `AsvoJobVec` (iterable, with `filter` and `json`),
`AsvoFilesArray`, `JobSubmittedResponse`, and the enums `AsvoJobType`,
`AsvoJobState`, `Delivery`, `DeliveryFormat`, `Output`, and others.
`AsvoJobState::Error(String)` carries data, so in Python it is
`AsvoJobState.Error` and the message is in `AsvoJob.error_text`.

Long calls: downloads check for Ctrl-C between chunks and release the
GIL. There is no library wait call; the caller's own loop (for example
with `time.sleep`) handles Ctrl-C as normal Python code.

Exceptions: `AsvoError::AsvoApi` (an API failure inside a download) is
raised as `AsvoApiError`, so a caller catches every API failure in one
way. The tuple variants' values are named attributes: `job_id` or `obs_id`.

Steps:

| Step | Content |
|---|---|
| 2.1 | `AsvoClient(...)`, `get_jobs`, the job types and enums, the two exceptions (done) |
| 2.2 | The seven submit methods, `JobSubmittedResponse`, and the enums the job arguments need (done) |
| 2.3 | `cancel_job`, `parse_many_jobids_or_obsids`, and the `*_params` builders (done) |
| 2.4 | `download_jobid`, `download_obsid` and the progress callback, with Ctrl-C between chunks (done) |

## Phase 3: stubs, docs, tests, CI

- `.pyi` stubs from `pyo3-stub-gen` and docstrings (step 3.1, done), and
  `docs/PYTHON.md` (step 3.2, done).
- pytest suite against a local mock server (`pytest-httpserver`), never a
  real server. The `giant-squid` command (`mwa_giant_squid_cli/`) is
  tested in the same suite (step 3.3, done), so CI needs no separate smoke
  test.
- CI (step 3.4, done: `.github/workflows/python.yaml`) builds wheels for the
  `run-tests.yaml` platforms (Linux x86_64/aarch64, macOS x86_64/arm64) plus
  an sdist, and runs pytest;
  `ruff check`, `ruff format` (line length 120) and `ty check` on the
  Python code, and a `stubs` job that fails if the stub is out of date.
  Publishing to PyPI stays your step. `releases.yaml` builds the same four
  wheels and attaches them to the GitHub release.

Notes for Phase 3, found during Phases 1 and 2:

- The dates and times are `jiff::Timestamp` (step 2.11). pyo3-stub-gen
  0.23.1 (PR #496) has the optional feature `jiff-02`, which maps
  `Timestamp` to `datetime.datetime`, so they need nothing more. Even with
  it, pyo3-stub-gen depends on chrono (not optional), so `python-stubgen`
  builds pull chrono in; the published crate and wheel do not.
- `#[gen_stub(...)]` override attributes do not work with the `cfg_attr`
  annotations: inside `cfg_attr(...)` the `#[gen_stub_pymethods]` macro
  does not see them, and as plain attributes they do not compile without
  the feature ("cannot find attribute `gen_stub`"). To give an argument
  or a return value a type that pyo3-stub-gen does not know, use a newtype
  with a `PyStubType` (see `src/python/typed.rs`).
- Build the Linux wheels in a manylinux container (for example
  `maturin-action` with `manylinux: auto`). A wheel built on a developer
  machine gets that machine's glibc tag (`manylinux_2_38` on the build
  machine used here), which older systems, such as many HPC systems,
  cannot install.
- The wheel links `reqwest`, `rustls` and `aws-lc-rs` (about 2.4 MB).
  `aws-lc-sys` compiles C code, so each wheel build environment needs a C
  toolchain. The standard maturin images have one.
- `readme` in `pyproject.toml` is `docs/PYTHON.md`, so that the PyPI
  page describes the Python module and not the CLI (step 3.2, done).

## Out of scope for now

An `async` API, free-threaded wheels, Windows wheels, and an astroquery
module (decided 2026-10-02: out of scope for this project).
