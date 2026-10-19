# giant-squid `apiv2` review: handoff for the next session

## Start here

1. Make a fresh clone of `MWATelescope/giant-squid`, branch `apiv2`.
2. Check that the head is `48ecadb` "Remove duplicated code (C items)", or a later commit by Greg.
3. Run the full check (see "How to check") before you change anything. Expected result:
   - 365 Rust unit tests and 53 CLI tests pass; 1 unit test and 18 live tests are ignored;
   - 201 Python tests pass;
   - clippy is clean in all three configurations;
   - `uv run ruff check` has one finding that is not from our work (see "Open"): RUF100 at `tests/python/test_client.py:462`, from ruff 0.17.1.
4. Read "Waiting for a decision by Greg" and "Decided, not started", then ask Greg what to do first.

## Rules for this work

- Never push, and never open a PR or take any other action on GitHub. Greg does all of that.
- Deliver each change as a plain diff and a base64 diff. Each diff must contain its code **and** its test edits, and `cargo test` must pass with that diff applied on its own (to the state that Greg has).
- Ask Greg before you edit, move or delete any existing test, or a shared test helper such as `write_session_at` in `src/test_common.rs`. You may add new tests without asking. List the exact edits in the question.
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
- Communicate in Simplified Technical English. No fluff.

## Done (all committed and pushed)

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

- **C15:** the OpenAPI generator moved from `build.rs` (feature `regen-openapi`) to the standalone crate `xtask`: `cargo xtask regen-openapi` (an alias in `.cargo/config.toml`). Greg chose a workspace member, but the Python sdist needs the root `Cargo.lock` to match the root manifest (`maturin sdist` copies the lock and writes an empty `[workspace]`, so `cargo --locked` failed with `xtask` in the lock). So `xtask` is not a workspace member: it has its own `[workspace]` and its own `xtask/Cargo.lock` (same pins as before). The root `Cargo.lock` no longer lists typify, schemars, syn or prettyplease. `check.sh` formats and lints `xtask` too. The only change in `openapi.rs` is its header.

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

## Decided by Greg, not started

- **L10 part B:** refuse `custom_centre_ra`/`custom_centre_dec` when `centre` is not `custom`. Greg approved the edit of the two tests that this needs: `every_limit_is_enforced_on_both_ends_for_the_conversion_body` and `every_limit_is_enforced_on_both_ends_for_the_imaging_body` (they set one coordinate with the default centre). Change: base parameters with `centre: Custom` and both coordinates valid. Until this is done, the help text "Requires --centre custom" is only half true.
- **Symbolic links:** close the check-then-create race with both `O_NOFOLLOW` (the last path part, Unix; std has no constant, so it needs `libc` or `rustix`) and `openat` (every path part, mainly in stream-untar, which creates parent directories). Greg has not yet answered the other three suggestions, so confirm them first:
  1. one rule for keep-tar and stream-untar: a link in the download directory is an error in both (today stream-untar only warns and skips);
  2. links inside a Scratch job folder are copied as links: keep this, and warn for a link that points outside the job folder;
  3. the download directory itself may be a symbolic link; write this rule in the README.
- **O2:** `--no-apply-amps`, `--join-polarizations` and `--multiscale` work like the other nullable fields (a value `true|false|null`, with the schema default shown in `--help`) and follow the API. C1/C2 are done, so this can start. The same three fields need the same in Python if they are not already typed there.
- **T1:** remove the three `staging_count` tests (Rust client, CLI, Python). **T5:** keep the clap and version tests.
- **O1/O7:** wait for the API fix.
- **A13 (`days: null`)** and the other questions below: with the API developer.

## Waiting for a decision by Greg

- **ruff 0.17.1:** `uv run` now installs ruff 0.17.1, which reports RUF100 (unused `# noqa: DTZ001`) at `tests/python/test_client.py:462`. This is an existing test. Remove the `noqa`, or pin ruff?
- **`manta-ray-client`** does not use the session cache file (Greg confirmed); the docs no longer say so.

## Open, not started

Logic:
- Lower: Scratch re-download after a move gives `FilesNotReachable` (now `DestinationExists` if the target is there); a lower-case `z` in a timestamp gets `Z` appended and fails the listing.

OpenAPI, library and CLI:
- **O1/O7:** typify merges the two inline enums titled `Delivery`, so `dug` is accepted for conversion and imaging (Greg: wait for the API fix).
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
- **V1–V3:** long docs (`normalize_job_value`, `get_jobs`, `require_tls`, a comment repeated 3 times in `cli/mod.rs`); repeated `job.job_id()` calls; 10 `#[allow(unused_imports)]` re-export lines.

Tests: T1 and T5 are decided (see above).

Not reviewed yet: `src/python/types.rs`, `params.rs`, `typed.rs` (and `error.rs` after the C13 change); `tests/cli.rs` and the Python tests in detail; `tools/*.sh`, `scrub_recording.py`, the Dockerfile, the release and docs workflows.

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
- **Long builds:** run them with `setsid nohup … < /dev/null &` and poll in separate commands of at most about 250 s (a command that runs for 300 s is stopped and can take its background job with it). Do not edit the code while a background check runs, and do not use `pkill -f` with a word that is also in your own command.
- **`--help` must not change by accident:** build the old head in a second worktree (`git worktree add /tmp/base HEAD`, `CARGO_TARGET_DIR=/tmp/base-target cargo build --bin giant-squid`) and compare `--help` of every command with the new binary.
- **Full check:**
  - `cargo fmt --check`;
  - `cargo test --locked`;
  - clippy `-D warnings` with `--all-targets`, with `--no-default-features --features python --lib`, and with `--no-default-features --lib`;
  - `uv run pytest`;
  - `PYO3_PYTHON=$PWD/.venv/bin/python tools/generate_stubs.sh`, then check that `mwa_giant_squid.pyi` did not change (or include the regenerated file in the diff);
  - `uv run ruff check`, `uv run ruff format --check` and `uv run ty check` (`uv run` may install a newer ruff than the one in an earlier session; see the RUF100 finding above);
  - `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`.
- **README `--help` blocks:** they must match the real `--help` output (with `NO_COLOR=1` and `GIANT_SQUID_DELIVERY*` unset). Regenerate them when a help text changes.
- **Before a commit that touches the API client:** Greg runs `tools/run_live_tests.sh` against test-asvo.
