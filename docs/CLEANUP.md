# giant-squid `apiv2` review: handoff for the next session

## Start here

1. Make a fresh clone of `MWATelescope/giant-squid`, branch `apiv2`.
2. Check that the head is `ed92fd9` "Live tests read --json results, not log text", or a later commit by Greg.
3. Run the full check (see "How to check") before you change anything. Expected result:
   - 343 Rust unit tests and 53 CLI tests pass; 1 unit test and 18 live tests are ignored;
   - 197 Python tests pass;
   - clippy is clean in all three configurations.

## Rules for this work

- Never push, and never open a PR or take any other action on GitHub. Greg does all of that.
- Deliver each change as a plain diff and a base64 diff. Each diff must contain its code **and** its test edits, and `cargo test` must pass with that diff applied on its own (to the state that Greg has).
- Ask Greg before you edit any existing test. You may add new tests without asking.
- When a decision is needed, stop and ask. Keep each step small (one diff per step).
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
- Communicate in Simplified Technical English. No fluff.

## Done (all committed and pushed)

- CI: a check-only `check.sh` run by a lint workflow; the lint workflow uses the MSRV; `serde_json` build dependency optional.
- Logging: inline format arguments in `run.rs`, `client/mod.rs` and `download/mod.rs`; one "N of M ... failed" summary; no duplicate `error!` logs.
- Legacy JSON: a test for every job type, checked against tag `v2.5.1`.
- `e891d31`: review logic items L1–L10 (see below).
- `ed92fd9`: the live tests read `error_code` and `status` from `--json` lines, not log text.

## Review of 15 Oct 2026

The IDs below are from that review. Its "L" items are logic errors (not the older logging items L1/L5/L6).

### Done in `e891d31`

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

### Decisions by Greg (16 Oct 2026)

- **L10 part B:** approved, including the edit of the two tests.
- **Symbolic links:** do both `O_NOFOLLOW` and `openat`.
- **L11:** add a key fingerprint and the host to the cached session (mwa-cli will be deprecated before release). Done: `StoredTokens` has `host` and `api_key_sha256` (SHA-256 of the key in lower-case hex; the key is never stored; new dependency `sha2`). A session that does not match, or a file in the old format (no fields, `#[serde(default)]`), is ignored, and a fresh login replaces it. A refresh keeps the fields of the session. Greg approved changing `write_session_at` (now takes the host and the API key) and its two callers in `tests/common/mod.rs` and `tests/live.rs`.
- **L12:** refuse a buffer size of 0. Done: new `AsvoError::InvalidOption` (a download option that cannot be used; a `ValueError` in Python, code `INVALID_ARGUMENT`), `DownloadOptions::check`, and `GIANT_SQUID_BUF_SIZE=0` is an `InvalidEnvironment` error.
- **L15:** refuse a timeout of 0. Done: `AsvoClient::new` now returns `AsvoError` (Greg chose this, not `AsvoApiError`); a zero `api_timeout` gives `InvalidOption` (`ValueError` in Python), and `MWA_ASVO_API_TIMEOUT=0` is an `InvalidEnvironment` error. Three tests in `api/client/tests.rs` were edited with Greg's approval (the patterns are wrapped in `AsvoError::AsvoApi`).
- **L13:** `wait_loop` retries. Done: new `AsvoClient::get_jobs_retrying` (network errors, HTTP 429 and 5xx; `AsvoApiError::is_transient`), the backoff moved to `src/mwa_asvo/retry/` and shared with the downloads, `AsvoApiError::ApiError` has `status`, and the CLI retries for `WAIT_RETRY_DURATION` (900 s, no environment variable; Greg chose this). Existing tests moved or edited with Greg's approval: the four `backoff_policy` tests (now `retry/tests.rs`) and the two `ApiError` constructors in `error_response/tests.rs`. **L14:** write the session file atomically with mode 0600 from the start, and give `.mwa-asvo` mode 0700. Done: `token_store::save` writes a temporary file in the same directory (created with mode 0600), syncs it and renames it over `tokens.json`; a new `.mwa-asvo` gets mode 0700; an existing directory is not changed, and a warning with the `chmod` command is logged if group or others have any permission on it (Greg chose the warning, not a change of mode). Unix only; on Windows the file is written the same way without modes. L11 will change the file content (key fingerprint and host).
- **O1/O7:** wait for the API fix.
- **O2:** the three flags work like the other nullable fields and follow the API.
- **C1, C2, C5, C13:** approved (do C1/C2 before O2).
- **T1:** remove the three `staging_count` tests. **T5:** keep the clap and version tests.

### Waiting for a decision by Greg

- **L10 part B:** refuse `custom_centre_ra`/`custom_centre_dec` when `centre` is not `custom`. This needs a change to two existing tests: `every_limit_is_enforced_on_both_ends_for_the_conversion_body` and `every_limit_is_enforced_on_both_ends_for_the_imaging_body` (they set one coordinate with the default centre). Proposed change: base parameters with `centre: Custom` and both coordinates valid. Ask Greg before you edit them. Until then, the help text "Requires --centre custom" is only half true.
- **Symbolic links (suggestions given, no choice yet):**
  1. one rule for keep-tar and stream-untar: a link in the download directory is an error in both (today stream-untar only warns and skips);
  2. close the check-then-create race with `O_NOFOLLOW` (Unix), or with `openat` if a shared download directory is a real case;
  3. links inside a Scratch job folder are copied as links: keep this, and warn for a link that points outside the job folder;
  4. the download directory itself may be a symbolic link; write this rule in the README.

### Open, not started

Logic:
- Lower: Scratch re-download after a move gives `FilesNotReachable` (now `DestinationExists` if the target is there); a lower-case `z` in a timestamp gets `Z` appended and fails the listing.

OpenAPI, library and CLI:
- **O1/O7:** typify merges the two inline enums titled `Delivery`, so `dug` is accepted for conversion and imaging. A typify patch in `build.rs` could keep two types (the API fix is also pending).
- **O2:** `--no-apply-amps`, `--join-polarizations` and `--multiscale` are plain flags: they show no schema default and cannot send `false`.
- **O5:** short flags differ between commands (`-f`, `-o`).
- **O8:** the `api/mod.rs` doc lists only the processor endpoints as not wrapped; end-user endpoints (`observation/metadata`, `project_permissions`, `user`, and others) are not wrapped either.
- **O9:** `legacy_json::legacy_job_type` and `table::job_type_colour` match job-type names as string literals; use `JOB_TYPE_NAMES` or the codes.

Documentation:
- **D1:** README line 155 shows `MWA ASVO API 2.3.0`; `--version` prints 2.3.4.
- **D2:** README `submit-volt --help` block differs from the real help.
- **D4:** `staging_count` is not in schema 2.3.4, but the `api/mod.rs` doc, `PYTHON_BINDINGS.md`, `TESTING.md` and `CHANGELOG.md` still describe it.
- **D5:** `TESTING.md` names `submit_each_obsid` (now `submit_each_obs_id`, in the library) and `test_wait_and_cancel_refuse_an_obsid` (not found).
- **D6:** "job" instead of "Job ID" in `AsvoError::NoUrl`, `NoPath`, `Http404Error`, `InvalidJob`, the cancel log and the `submit-image-from-job` message.
- **D7/D16:** positional `{}` arguments left in `client/mod.rs:195`, `helpers`, `mwa_asvo/error.rs`, `table.rs`, `value_enums`, `api/error.rs`, `token_store` and `python/download.rs`.
- **D8:** the CHANGELOG 3.0.0 section mixes in-development history with changes against 2.5.1; heading levels differ.
- **D9:** `client/mod.rs` lines 7–12 tell history, not the design.
- **D11–D15:** `api/error.rs` names `HttpValidationError` (not in the schema); the `ApiError` doc reason for copying fields is not correct; `PYTHON_BINDINGS.md` "For the API developer" is partly stale (`JobState` numbers, `staging_count`); two `build.rs` comments are not correct; the `env` doc says the Python module reads the environment (it does not).
- `docs/CLEANUP.md` (this file) ships in the crate and the sdist; exclude it before the release.

Duplication and verbosity:
- **C1/C2:** in `cli/params.rs`, the nullable-bool `#[arg]` block (6 times) and `allow_resubmit` (6 times); the WSClean and preprocessing assignments in `to_params` are repeated. Proposal: a generic `ResubmitArgs<P>` and a setter macro next to `impl_fields!`.
- **C3:** `validate` repeats the preprocessing checks for conversion and imaging; `days` bounds are checked twice.
- **C4:** the `SchemaDefaults` impls are one-line wrappers.
- **C5/C6:** `SubmitImageFromJob` re-implements `run_submit` (different error output); the 6 submit arms repeat ~25 lines.
- **C7:** two logger set-ups; `list` and `wait` repeat the output branch.
- **C13:** the six `ErrorResponse` fields are copied in four places; the variants could hold an `ErrorResponse`.
- **C14:** the `cli/tests.rs` tests `job_ids_only_*` repeat the `helpers/tests.rs` tests.
- **C15:** `build.rs` writes into `src/` (an anti-pattern; an xtask is the usual pattern). Low priority.
- **C16:** CI `openapi-drift-check` builds without `--locked`.
- **V1–V3:** long docs (`normalize_job_value`, `get_jobs`, `require_tls`, a comment repeated 3 times in `cli/mod.rs`); repeated `job.job_id()` calls; 10 `#[allow(unused_imports)]` re-export lines.

Tests:
- **T1:** the three `staging_count` tests (Rust client, CLI, Python) cannot fail now.
- **T5:** tests of clap itself (`verbosity_counts_repeats`, `unknown_command_is_rejected`, `single_obs_id_is_collected`, `many_obs_ids_are_collected`) and `the_recorded_versions_look_like_versions`.

Not reviewed yet: `src/python/types.rs`, `error.rs`, `params.rs`, `typed.rs`; `tests/cli.rs` and the Python tests in detail; `tools/*.sh`, `scrub_recording.py`, the Dockerfile, the release and docs workflows.

### A13 — `days: null` (waits for the API developer)

- The schema type is `integer | null`, default 30. A request has three states: key absent, `null`, or 1–30.
- Ask the API developer what `null` means. Proposed design if it is "no day limit": Rust `enum Days { ApiDefault, NoLimit, Last(NonZeroU64) }` in `JobsFilter`; Python `days=None` stays the API default and a module constant sends `null`; CLI `--days all` sends `null`.

### Items to leave as they are (Greg agreed)

- The Python mapping of error kinds that are never raised.
- The Python error mapping repeats `job_id()`/`obs_id()`.
- The hidden `download --hash` no-op.

## Questions still with the API developer

- The `obs_id` minimum (888888889, but an Obs ID has 10 digits).
- `clean_iterations` has no minimum.
- `dug` delivery for conversion and imaging (still refused by schema 2.3.4).
- The voltage fields (`delivery`, `delivery_format`, `channel_range`).
- The flow-2 `obs_id`.
- `JobFile.type` uses its own `Type` enum.
- A typed `obs_id` in `JobDetailResponse`.
- The meaning of `days: null` (A13).
- New: a voltage job with `--offset 0` on obs 1360706032 (MWAX_VCS) is refused with "the requested offset must be a multiple of 8". 0 is a multiple of 8. This fails `live_submit_volt`.
- Server-side, not a question: test-asvo sometimes returns `EXT_TAP_SERVICE_ERROR` (TAP connection reset). This made `live_cancel` fail once.

## How to check (sandbox)

- **Rust:** `apt-get install rustc-1.89 cargo-1.89 rust-1.89-clippy rustfmt-1.89 python3-dev`, then `export PATH=/usr/lib/rust-1.89/bin:$PATH CARGO_HOME=/home/claude/.cargo CARGO_INCREMENTAL=0`.
- **Disk:** the sandbox disk is small. Keep one clone and its `target`, and use `CARGO_INCREMENTAL=0`.
- **Long builds:** run them with `setsid nohup … &` and poll, because a command can run for at most 300 s.
- **Full check:**
  - `cargo fmt --check`;
  - `cargo test --locked`;
  - clippy `-D warnings` with `--all-targets`, with `--no-default-features --features python --lib`, and with `--no-default-features --lib`;
  - `uv run pytest`;
  - `PYO3_PYTHON=$PWD/.venv/bin/python tools/generate_stubs.sh`, then check that `mwa_giant_squid.pyi` did not change (or include the regenerated file in the diff);
  - `uv run ruff check`, `uv run ruff format --check` and `uv run ty check`;
  - `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`.
- **README `--help` blocks:** they must match the real `--help` output (with `NO_COLOR=1` and `GIANT_SQUID_DELIVERY*` unset). Regenerate them when a help text changes.
- **Before a commit that touches the API client:** Greg runs `tools/run_live_tests.sh` against test-asvo.
