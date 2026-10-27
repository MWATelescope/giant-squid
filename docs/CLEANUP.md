# giant-squid `apiv2` review: handoff for the next session

## Start here

1. Make a fresh clone of `MWATelescope/giant-squid`, branch `apiv2`.
2. Check that the head is `5b8de97` "Python: request body classes and two submit methods", or a later commit by Greg.
3. Run the full check (see "How to check") before you change anything. Expected result:
   - 520 Rust unit tests, 59 CLI tests, 7 `download_job_dir` tests, 25 `stdin_ids` tests and 18 `submit_csv` tests pass; 1 unit test and 18 live tests are ignored;
   - 218 Python tests pass;
   - clippy is clean in all three configurations, and `check.sh` passes;
   - `ruff check`, `ruff format --check` and `ty check` pass (ruff 0.17.1);
   - the regenerated `mwa_giant_squid.pyi` and `openapi.rs` do not change.
4. Read "Waiting for the API developer" and "Open, not started", then ask Greg what to do first.

## Rules for this work

- Never push, and never open a PR or take any other action on GitHub. Greg does all of that.
- Deliver each change as a plain diff and a base64 diff. Each diff must contain its code **and** its test edits, and `cargo test` must pass with that diff applied on its own (to the state that Greg has).
- Ask Greg before you edit, move or delete any existing test, or a shared test helper such as `write_session_at` in `src/test_common.rs`. You may add new tests without asking. List the exact edits in the question.
- Actions that GitHub does not own are pinned by commit, with the tag in a comment. To update one, find the commit of the tag with `git ls-remote https://github.com/<owner>/<repo> 'refs/tags/<tag>*'` (an annotated tag has a second line, `<tag>^{}`: use that commit), and change the comment.
- CI: change `.github/` in the diff that needs it (Greg asked for this on 20 Oct 2026), and say in the answer what the sandbox could not run (CI itself, macOS, Windows).
- When a decision is needed, stop and ask. Keep each step small (one diff per step). Greg commits each diff and says so; then fetch and reset to `origin/apiv2` before the next step.
- Name each diff `<step>.diff` with a `.b64` copy and a commit message (first line at most 50 characters, detail lines at most 72), and check that it applies to a fresh clone.
- The OpenAPI schema (the FastAPI) dictates the library. Exceptions: wait and download, which are not part of the FastAPI.
- The Rust library and the Python library must be as consistent as possible.
- In messages, logs, `--help` and docs, always write "Job ID" and "Obs ID" (capitalised, also mid-sentence). Identifiers and JSON keys keep `job_id` and `obs_id`.
- Keep `--legacy-json` for a few releases. Its output is exactly the giant-squid 2.5.1 output (no `completed` key).
- Python code:
  - Google-style docstrings;
  - `ruff check`, and `ruff format` with line length 120;
  - `ty check` must pass.
- Rust code: `cargo fmt`, `cargo check` and `cargo clippy`.
- Do not use magic numbers or strings. Put them in constants.
- Mark each decision for Greg with a red dot (🔴), and do not go on until he has decided.
- Format arguments: bind an expression to a name first, then use the inline format string (no mix of inline and positional arguments).
- Communicate in Simplified Technical English. No fluff.

## Done (all committed and pushed)

Earlier sessions (before 19 Oct 2026) are listed first, then the session of 19 Oct 2026.

Before the review of 15 Oct 2026:
- CI: a check-only `check.sh` run by a lint workflow; the lint workflow uses the MSRV; `serde_json` build dependency optional.
- Logging: inline format arguments in `run.rs`, `client/mod.rs` and `download/mod.rs`; one "N of M ... failed" summary; no duplicate `error!` logs.
- Legacy JSON: a test for every job type, checked against tag `v2.5.1`.

From the review of 15 Oct 2026 (the IDs are from that review; its "L" items are logic errors, not the older logging items L1/L5/L6):
- `e891d31`: **L1–L10 part A** (see the list below).
- `ed92fd9`: the live tests read `error_code` and `status` from `--json` lines, not log text.
- `ef9eaf0` **L12:** a download buffer size of 0 is refused: new `AsvoError::InvalidOption` (a `ValueError` in Python, code `INVALID_ARGUMENT`), `DownloadOptions::check`; `GIANT_SQUID_BUF_SIZE=0` is an `InvalidEnvironment` error.
- `751cefc` **L15:** a timeout of 0 is refused. `AsvoClient::new` returns `AsvoError` (Greg's choice); zero `api_timeout` is `InvalidOption`; `MWA_ASVO_API_TIMEOUT=0` is an `InvalidEnvironment` error.
- `d1858f8` **L13:** `AsvoClient::get_jobs_retrying` retries network errors and HTTP 429 and 5xx (`AsvoApiError::is_transient`) with the backoff, now in `src/mwa_asvo/retry/` and shared with the downloads. `AsvoApiError::ApiError` has `status`. The CLI `wait` retries for `WAIT_RETRY_DURATION` (900 s, no environment variable).
- `962def8` **L14:** `token_store::save` writes a temporary file (mode 0600 from the start), syncs it and renames it over `tokens.json`; a new `.mwa-asvo` gets 0700; an existing directory is not changed, and a warning with the `chmod` command is logged if group or others have any permission on it.
- `4af2732` **L11:** `StoredTokens` has `host` and `api_key_sha256` (SHA-256 of the key; the key is never stored; dependency `sha2`). A session for another host or key, or a file in the old format, is ignored and replaced by a fresh login. `write_session_at` takes the host and the API key.
- `48ecadb` **C1–C7, C13, C14, C16:**
  - `ResubmitArgs<P>` and `apply_to` (macro `impl_apply_to!`) in `cli/params.rs`; `impl_schema_defaults!`;
  - one `check_preprocessing` for conversion and imaging; `parse_days` checks `days` once;
  - `submit-image-from-job` uses `run_submit`; `SubmitOptions::submission`; one `install_logger`; one `print_jobs`;
  - `ApiError` and `AuthenticationFailed` hold the schema's `ErrorResponse` (boxed, for `clippy::result_large_err`; an empty `field_errors` is stored as `None`);
  - three duplicate `job_ids_only_*` tests removed from `cli/tests.rs`, two moved to `helpers/tests.rs`;
  - `--locked` in the drift check;
  - the six nullable-bool `--no-*` `#[arg]` blocks in `PreprocessingArgs` stay: clap's derive cannot share an attribute block between fields;
  - `--help` of all 12 commands is byte-for-byte the same as before.

### Done on 19 Oct 2026 (one diff each, in this order)

- **"Fix documentation items D1-D16":**
  - README: the `--version` example (D1) and the `submit-volt --help` block (D2);
  - `staging_count` is gone from the docs (D4); `TESTING.md` names real tests (D5);
  - "Job ID" in four error messages and two debug logs (D6), and in the cancel log (`Cancel request for Job ID N: <message>`);
  - positional format arguments are bound to names first (D7/D16), except the `concat!` repr in `python/types.rs` and `write!(f, "{}", self.0)` in `obs_id`;
  - D9 (client header doc), D11, D13, D14 (`build.rs` comments) and D15 (`env` doc) are fixed;
  - D12: `AsvoError::HttpError` and `AsvoApiError::BadStatus` have the same fields, but `HttpError` is an error of the file server, not of the API (Greg confirmed this reading);
  - the `api/mod.rs` doc lists the end-user endpoints that are not wrapped (O8); `docs/CLEANUP.md` is excluded from the crate and the sdist.
- **"Split the 3.0.0 changelog from its history" (D8):** the 3.0.0 section of `CHANGELOG.md` lists only changes against 2.5.1 (checked with the tag and the 2.5.1 binary). The old entries are in `docs/history/CHANGELOG_3.0.0_DEVELOPMENT.md`, word for word. The library changes are one block with sub-points.
- **"Move the OpenAPI generator to xtask (C15)":** `cargo xtask regen-openapi` (an alias in `.cargo/config.toml`) replaces the `regen-openapi` feature of `build.rs`. Greg chose a workspace member, but the Python sdist needs the root `Cargo.lock` to match the root manifest: `maturin sdist` copies the lock and writes an empty `[workspace]`, so `cargo --locked` failed with `xtask` in the lock. So `xtask` is a standalone crate with its own `[workspace]` and its own `xtask/Cargo.lock` (same pins as before). The root `Cargo.lock` no longer lists typify, schemars, syn or prettyplease. `check.sh` formats and lints `xtask` too. The only change in `openapi.rs` was its header. The drift check in `run-tests.yaml` runs `cargo --locked xtask regen-openapi` (Greg changed it).
- **Symbolic links (three diffs):**
  1. "Make a symlink in the download dir an error": a link that is already on disk on the way to a stream-untar entry is an error (`SymlinkInDownloadDir`), as in keep-tar. A Scratch move warns about links that point outside the job folder (the links are moved as links). The download directory itself may be a link. All are in the README.
  2. "Open download files with O_NOFOLLOW": `download/no_follow/` has `open_for_writing` (Unix, `rustix`) for the files of keep-tar, stream-untar and the resume file.
  3. "Walk tar entry directories with openat": `DownloadRoot` opens the download directory once, then each directory of a tar entry with `openat` and `O_NOFOLLOW` (missing ones with `mkdirat`), so no link can be swapped in between a check and the use. `UntarCheckpoint` holds `unpack_path`.
  - Reads (hashing, the resume walk) still follow links. Off Unix, a check just before each use is made; this code has not been compiled (no non-Unix target in the sandbox).
- **"Refuse custom centre coordinates without custom" (L10 part B):** `custom_centre_ra` and `custom_centre_dec` are refused when `centre` is not `custom`, in Rust, the CLI and Python.
- **"Drop staging_count tests; true/false switches" (T1, O2):** the three `staging_count` tests are gone (the Rust one is kept as `the_client_calls_only_end_user_endpoints`). `--no-apply-amps`, `--join-polarizations` and `--multiscale` take `true` or `false` (the bare flag is `true`) and show the schema default in `--help`. The schema (2.3.4) does not allow `null` for them, so `null` is refused.
- **"Match job types by code; trim docs and imports" (O9, V1–V3):** `legacy_job_type` and `job_type_colour` match the `JOB_TYPE_CODE_*` constants; four long docs are shorter; `job.job_id()` is read once per function; the nine `#[allow(unused_imports)]` re-exports are one `use`.
- The unused `noqa` (RUF100) had already been removed in `48ecadb`.
- **Last logic items:** a second download of a Scratch job whose files an earlier download moved is not an error (`MoveTarget::AlreadyMoved`: a directory at the target, nothing at the Scratch path; a file at the target is still `DestinationExists`); a lowercase `z` is a valid time zone mark (`TIME_ZONE_MARKS`), so it no longer gets a second `Z` that failed the listing.
- **macOS CI fix (`no_follow`):** the macOS 15 wheel job failed to compile: `rustix::fs::Mode::from_raw_mode` takes a `u16` on macOS and a `u32` on Linux, and the code passed a `u32` constant. The file and directory permissions are now rustix's own `Mode` bits (`Mode::RUSR | ...`, `Mode::RWXU | ...`), with no raw number. Lesson: do not give a platform type (a raw mode, `st_mode`, `off_t`) a fixed integer type; the sandbox is Linux only, so such an error shows only in the macOS job.
- **O5 (option C):** `-f` is only `--delivery-format` (`submit-volt --from-channel` and `--to-channel` have no short flag), and `-n` is only `--dry-run` (`list --no-colour` and `wait --no-colour` have no short flag). `-d`, `-o` and `-r` stay as they were in 2.5.1 (each still has more than one meaning). The test `the_short_flags_mean_what_the_table_says` holds the table; `docs/V3_MIGRATION.md` has a "Short flags" section.

### Done on 19–20 Oct 2026, after `d635a16` (one diff each, in this order)

- `b67f232` **No credentials in trace logs:** the login and refresh bodies are not logged; URLs in other logged bodies, in the download log and in download errors are redacted; reqwest `connection_verbose` is removed.
- `95e4387`, `319895d` **One `get_jobs`:** the Python `list_jobs` is merged into `get_jobs` (signature: `job_ids`, `obs_ids`, `job_types`, `job_states`, then keyword-only `days` and dates); `get_jobs_retrying` is crate-private, so `get_jobs` is the only public way to get the job list.
- `20a15d2`, `d3b40ea`, `c5e4159` **manta-ray-client CSV import:**
  - `giant-squid submit-csv` reads the CSV job files of `mwa_client`, checks every line, then submits the jobs; a failing line does not stop the others;
  - library: `parse_manta_ray_csv`, `CsvDefaults`, `AsvoClient::submit_csv_file` and `submit_csv_jobs` (module `src/mwa_asvo/manta_ray_csv/`); the other fields of the request bodies are accepted as CSV keys; a repeated key and a repeated job give a warning; `GIANT_SQUID_DELIVERY*` give the delivery defaults;
  - Python: `AsvoClient.submit_csv_file` returns the replies; failed jobs raise `CsvSubmitError` with the replies of the jobs that were submitted;
  - the six test files are in `tests/files/manta-ray-client/` (`58e73c9`).
- `980bf66`, `6bd7c36` **IDs from standard input:** `download`, `cancel` and `wait` read their IDs from standard input when given none (or `-`): the output of `list --ids` or `list --json`, or plain IDs; the new `list --ids` prints only the Job IDs. A pipe or a file with no ID is "Nothing to do" (exit 0); no input at all (a terminal, `/dev/null`) and no ID still fails. The commands do not filter the IDs. Tests: `tests/stdin_ids.rs`, `src/cli/stdin_ids/tests.rs`.
- `377e9a9` **A directory for each job:** `download` puts each job in `<download-dir>/<job_id>`; `--no-job-subdir` restores the 2.x layout. Rust has `DownloadOptions::job_subdirectory` and Python has the argument `job_subdirectory`. Downloads start in list order, so the `[n/N]` log numbers are in order. Tests: `tests/download_job_dir.rs`.
- `2cde8b3` **Updated dependencies** (`Cargo.lock` only).

### Done on 20 Oct 2026, the B diffs and the step C review (one diff each, in this order)

- "Better CSV errors; strip a BOM from stdin": an integer out of range names its range; a CSV line lists every unknown key; the stdin parser strips a UTF-8 BOM; the BOM and the `status`/`message` JSON keys are shared constants.
- "Use a repeated ID once in download/cancel/wait": `resolve_ids` removes a repeated ID and keeps the first place (`ObsId` has `Hash`).
- "Move the CSV file rules into the library": `read_manta_ray_csv` and `CsvFileError` (`Read`, `Invalid`, `NoJobs`); a file with no job is an error in the library and `ValueError` in Python; `submit_csv_jobs` returns `CsvJobsFailed`.
- "Add the file line to submit-csv --json errors" and "Print an error line for each bad CSV line": with `--json`, an error line of a failed job has `line` and `obs_id`, and a bad file gives one `INVALID_ARGUMENT` line for each bad line, each with its `line`. A submitted job prints the plain API reply.
- "Python downloads go in a job directory by default": the Python `job_subdirectory` is `True` by default. `DownloadOptions` has no default, so its field is documented.
- "Read stdin IDs only when an argument is `-`": no guessing from the kind of input. No ID and no `-` fails with "To read them from standard input, give `-`." (`READ_STDIN_HINT`).
- Step C review (Python bindings, Dockerfile, workflows, tools): findings C1–C14. The pdoc problems of 6 Oct 2026 are fixed (types, same output on each run, no `RecursionError`, members in source order). Done in this diff: C1 (`params.rs` doc), C3 (`error.rs` closure), C11 (`.dockerignore`), C12 (`git` removed from the Dockerfile builder; untested, the Docker CI job shows it), C14 (`run_live_tests.sh`). The base images are not pinned by digest: `dhi.io` needs a login, which the sandbox does not have.

- C2: a file or directory error of a download (`AsvoError::IO` and `CannotCreate`) is a Python `OSError` (`FileNotFoundError`, `PermissionError` and so on) with the error number, and the `filename` for `CannotCreate`. `os_error` moved from `csv.rs` to `error.rs`. The kinds `IO` and `CannotCreate` are no longer raised as `AsvoError`.

- C4–C9 (the `.github/` files):
  - `releases.yaml` has a `check-version` job (the tag must be `v` plus the version in `Cargo.toml`; every build job needs it) and a `test` job (`cargo test --locked`; `create-release`, and so the crates.io publish, waits for it). The builds and the publish use `--locked` (`build.sh`, maturin, `katyo/publish-crates` with `args: --locked`). The step that was named "Run tests" but only built is gone.
  - Every action that GitHub does not own is pinned by commit, with its tag in a comment (`dtolnay/rust-toolchain`, `astral-sh/setup-uv`, `PyO3/maturin-action`, `docker/*`, `katyo/publish-crates`, `codecov/codecov-action`, `taiki-e/install-action`). `dtolnay/rust-toolchain` has `toolchain: stable` as an input, because its `@stable` branch is not used any more. The `actions/*` actions stay at their major tag.
  - `docker.yml` skips pull requests from forks (they get no secrets) and cancels a run that a newer push replaces (never a tag run).
  - `coverage.yml` installs `cargo-llvm-cov` as a prebuilt binary and runs with `--locked`.
  - Checked in the sandbox: all the workflows parse and `actionlint` reports nothing; the version-check script gives the right result for a good and a bad tag. Not run: the workflows themselves.

- C13: `tools/test_msrv.sh` runs what CI runs at the MSRV (`check.sh` and `cargo test --locked`), in `target/msrv`, with no `cargo update`, no `cargo clean` and no `--all-features`. Tried in the sandbox with a fake `rustup` and Rust 1.89, not with a real 1.88.0 toolchain.

### Expired download links (20 Oct 2026, one diff)

Greg asked for this after a download failed with HTTP 403 `AccessDenied` (the signed URL had expired). Decisions: `Expires` in the URL is the end of the 7 days (not "7 days after `Expires`"), so a job is expired when `Expires` is not after now; Option A (the job state stays the API's, nothing is added to the schema's `JobState`); the table and `wait` show `completed (ready)` or `completed (expired)`; `download` fails with `AsvoError::JobExpired` (`JOB_EXPIRED`, Python kind `JobExpired`) before any request, and a 403 `AccessDenied` from the file server is the same error; the message does not mention `--allow-resubmit`, because the MWA ASVO takes a resubmission without it. A job is expired when any Acacia URL has expired; a URL with no readable `Expires` is not expired. `download` of an Obs ID skips expired completed jobs when another works. `wait` and `AsvoJobVec::all_ready` fail with the same error for a completed job whose links have expired, as for a cancelled job (Greg's decision, after the first diff).

### Windows: not supported for now (Greg decided on 20 Oct 2026)

Step C proposed a Windows CI job (C10). Greg abandoned Windows support for now, so nothing was added: no Windows job, no Windows wheels (`docs/PYTHON.md` says so), and no change in the code. What was found, for a later attempt:

- The sandbox cannot compile for a Windows target (Debian's rust-src has no Windows std support, and `static.rust-lang.org` is not reachable). A substitute that worked: copy the repository, replace `not(unix)` with `all()`, `cfg(unix)` with `cfg(any())`, and `std::os::unix` and `std::os::fd` with names that do not exist, then run `cargo check --all-targets`, `cargo clippy --all-targets` and `cargo test` there (use the same `CARGO_TARGET_DIR` to save disk). This showed that the non-Unix branches compile and pass their tests on Linux. It cannot show real Windows behaviour.
- Only these items stop a Windows test build: the test `a_job_subdirectory_that_is_a_symlink_is_refused` in `src/mwa_asvo/download/tests.rs` calls `std::os::unix::fs::symlink` without `#[cfg(unix)]`; and in `mod unsafe_paths` of the same file, the imports `entry_target` and `EntryTarget` and the helper `try_unpack` are used only by Unix tests (an unused-import and a dead-code warning).
- `aws-lc-sys` 0.46 needs NASM on Windows, or `AWS_LC_SYS_PREBUILT_NASM=1` to use its bundled objects.
- The session cache reads `HOME` only (`ENV_HOME`), so on Windows (`USERPROFILE`) no session would be cached. The token file is written without a restricted mode off Unix.
- A Windows CI job would also need `git config --global core.autocrlf false` before the checkout, `fail-fast: false`, and, for the Python tests, a `skipif` on the Ctrl-C tests (`os.kill(os.getpid(), SIGINT)` ends the process on Windows).
- The non-Unix branches (`no_follow`, `copy_symlink`, the token store) are never built in CI. Open question for Greg: declare the crate Unix-only (a `compile_error!` and delete those branches), or keep them.

### Done in `e891d31` (L1–L10 part A)

- **L1:** downloads use a client that sends no access token (the files are on Acacia).
- **L2:** `Bounds::contains` refuses values that are not finite (serde_json sends them as `null`).
- **L3:** stream-untar creates the parent directories of an entry.
- **L4:** stream-untar unpacks only files and directories; it skips links and special files with a warning.
- **L5:** `download` fails when its ID files hold no ID.
- **L6:** `-d` must be a directory.
- **L7:** no completion log for a file already on disk; the throughput is of the last attempt.
- **L8:** `--wait` does not wait when no job was submitted; a failed wait keeps the submission summary.
- **L9:** a Scratch move checks its target first: new error `DestinationExists` (`DESTINATION_EXISTS`); a symbolic link gives `SymlinkInDownloadDir`.
- **L10 (part A):** `centre` `custom` needs both `custom_centre_ra` and `custom_centre_dec`; each submit command checks its body once, before login, also with `--dry-run`.

### Done for the API 2.6.0 revamp (draft schema; 27 Oct 2026)

- **The schema is a draft.** `src/mwa_asvo/api/openapi-schema.json` was edited by hand (`info.version` 2.6.0) and `openapi.rs` regenerated with `cargo xtask regen-openapi`. When the real `openapi.json` is ready, run `tools/generate_openapi.sh` and read the diff: it shows where the draft was wrong.
- **Endpoints:** `POST /v2/jobs/download` (`DownloadJobParams`) and `POST /v2/jobs/preprocess` (a conversion, imaging flow 1 or imaging flow 2 body). `/v2/jobs/create` and `/v2/jobs/image` are gone, and so are `VoltageJobParams`, `CancelJobParams` and `DownloadType`.
- **`DownloadJobParams`:** `obs_id`, `delivery`, `delivery_format`, `allow_resubmit`, `only_get_metadata` (metadata job), and the nullable `offset`, `duration`, `from_channel` and `to_channel` (voltage). It has no `download_type`: the MWA ASVO finds vis, beamformer and voltage from the observation, and refuses a body that does not suit it. The library and the CLI do not check this.
- **Library:** `AsvoClient::submit_download_job` and `submit_preprocess_job` (a `PreprocessJobParams`, an untagged enum). `job_params` has `download_params`, `metadata_params` and `voltage_params`. `validate_download_params` checks the delivery format and the `offset` limits only. `validate_voltage_params` was removed (Greg: the server decides).
- **CLI:** the `submit-*` commands are unchanged. `submit-volt` defaults to the schema's `acacia`/`tar`, which the MWA ASVO refuses: the user gives `--delivery scratch --delivery-format files`. A CSV voltage line with no delivery still defaults to Scratch and files (a CSV-only exception).
- **Python:** the classes `DownloadJobParams`, `ConversionJobParams`, `ImagingJobFlow1Params` and `ImagingJobFlow2Params` are plain classes (read/write attributes, no checks until submit), and `AsvoClient` has `submit_download_job` and `submit_preprocess_job`. The seven per-type methods are gone.
- **Tests:** `tests/live.rs` `live_submit_volt` passes `--delivery scratch --delivery-format files`. The live tests have not run against a server that has the new endpoints.

## Waiting for the API developer

- **O1/O7:** typify merges the two inline enums titled `Delivery`, so `dug` is accepted for conversion and imaging (Greg: wait for the API fix).
- **A13 (`days: null`)** and the questions below.

### A13 — `days: null` (waits for the API developer)

- The schema type is `integer | null`, default 30. A request has three states: key absent, `null`, or 1–30.
- Ask the API developer what `null` means. Proposed design if it is "no day limit": Rust `enum Days { ApiDefault, NoLimit, Last(NonZeroU64) }` in `JobsFilter`; Python `days=None` stays the API default and a module constant sends `null`; CLI `--days all` sends `null`.

## Open, not started

Not reviewed yet (20 Oct 2026):
- `src/python/client.rs`, except the constructor, the `get_jobs` header and the conversion and imaging signatures;
- the body of `tests/cli.rs`, and the Python tests in detail;
- `tools/coverage.sh`, `tools/scrub_recording.py`, the body of `tools/generate_openapi.sh`, `tools/stubtest_allowlist.txt`, and `mwa_giant_squid_cli`.

Not tested in the sandbox: CI itself (the maturin wheel jobs on Linux and macOS: the macOS build cannot be tried here, so read the code that uses a platform type with care; the Docker build, the MSRV job: Rust 1.88 is not in the sandbox, and the highest declared MSRV in the `xtask` dependencies is 1.85); the non-Unix code of `no_follow`.

Notes:
- `manta-ray-client` does not use the session cache file (Greg confirmed); the docs no longer say so.

### Items to leave as they are (Greg agreed)

- The Python mapping of error kinds that are never raised.
- The Python error mapping repeats `job_id()`/`obs_id()`.
- The hidden `download --hash` no-op.

## Questions still with the API developer

- The `obs_id` minimum (888888889, but an Obs ID has 10 digits).
- `clean_iterations` has no minimum.
- `dug` delivery for conversion and imaging (still refused by schema 2.3.4).
- The voltage fields (`delivery`, `delivery_format`: shared enums, but only scratch and files work for a voltage job). The server tells the kinds of download apart from the observation: a refused body must be a 4xx with a specific code, not a 500 (the live tests fail on `INTERNAL_ERROR`).
- `POST /v2/jobs/preprocess` takes one of three bodies and has no discriminator. The server tells a conversion body from an imaging flow 1 body by the imaging fields. Check with `tests/live.rs` that it does, and that a flow 2 body (with `source_job_id`) is told apart too.
- Replace the draft `openapi-schema.json` with the real one (see "Done for the API 2.6.0 revamp") and check the names (`only_get_metadata`, the paths) against it.
- `JobFile.type` uses its own `Type` enum.
- The meaning of `days: null` (A13).
- New: a voltage job with `--offset 0` on obs 1360706032 (MWAX_VCS) is refused with "the requested offset must be a multiple of 8". 0 is a multiple of 8. This fails `live_submit_volt`.
- Server-side, not a question: test-asvo sometimes returns `EXT_TAP_SERVICE_ERROR` (TAP connection reset). This made `live_cancel` fail once.

## How to check (sandbox)

- **Rust:** `apt-get install rustc-1.89 cargo-1.89 rust-1.89-clippy rustfmt-1.89 python3-dev`, then `export PATH=/usr/lib/rust-1.89/bin:$PATH CARGO_HOME=/home/claude/.cargo CARGO_INCREMENTAL=0`. Only 1.89 is in the package list. Put this `PATH` in place before `uv sync` or `uv run`, so that maturin finds Rust (it cannot download it).
- **Disk:** the sandbox disk is small. Keep one clone and its `target`, and use `CARGO_INCREMENTAL=0`.
- **Long builds:** run them with `setsid nohup … < /dev/null &` and poll in separate commands of at most about 250 s (a command that runs for 300 s is stopped and can take its background job with it). Do not edit the code while a background check runs, and do not use `pkill -f` with a word that is also in your own command.
- **`--help` must not change by accident:** build the old head in a second worktree (`git worktree add /tmp/base HEAD`, `CARGO_TARGET_DIR=/tmp/base-target cargo build --bin giant-squid`) and compare `--help` of every command with the new binary.
- **Full check:**
  - `./check.sh` (formats and lints the library and `xtask`, builds the library only, and runs `cargo doc`);
  - `cargo test --locked`;
  - `uv run pytest`;
  - `PYO3_PYTHON=$PWD/.venv/bin/python tools/generate_stubs.sh`, then check that `mwa_giant_squid.pyi` did not change (or include the regenerated file in the diff);
  - `uv run ruff check`, `uv run ruff format --check` and `uv run ty check`;
  - `cargo --locked xtask regen-openapi`, then `git diff --stat src/mwa_asvo/api/openapi.rs`: only a header change is allowed, and only in a diff that changes the generator.
- **A change of a manifest, a lock file or the package contents:** also build the sdist (`uvx --from "maturin>=1.9.4,<2.0" maturin sdist -o /tmp/sdist`), unpack it, and run `cargo check --locked --no-default-features --lib` in it; and run `cargo package --locked --allow-dirty --no-verify` and look at the file list.
- **README `--help` blocks:** they must match the real `--help` output (with `NO_COLOR=1` and `GIANT_SQUID_DELIVERY*` unset). Regenerate them when a help text changes.
- **Before a commit that touches the API client:** Greg runs `tools/run_live_tests.sh` against test-asvo.
