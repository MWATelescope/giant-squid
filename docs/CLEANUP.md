# giant-squid `apiv2` review: handoff for the next session

## Start here

1. Make a fresh clone of `MWATelescope/giant-squid`, branch `apiv2`.
2. Check that the head is `d635a16` "Match job types by code; trim docs and imports", or a later commit by Greg.
3. Run the full check (see "How to check") before you change anything. Expected result:
   - 395 Rust unit tests and 53 CLI tests pass; 1 unit test and 18 live tests are ignored;
   - 194 Python tests pass;
   - clippy is clean in all three configurations, and `check.sh` passes;
   - `ruff check`, `ruff format --check` and `ty check` pass (ruff 0.17.1);
   - the regenerated `mwa_giant_squid.pyi` and `openapi.rs` do not change.
4. Read "Waiting for a decision by Greg" and "Open, not started", then ask Greg what to do first.

## Rules for this work

- Never push, and never open a PR or take any other action on GitHub. Greg does all of that.
- Deliver each change as a plain diff and a base64 diff. Each diff must contain its code **and** its test edits, and `cargo test` must pass with that diff applied on its own (to the state that Greg has).
- Ask Greg before you edit, move or delete any existing test, or a shared test helper such as `write_session_at` in `src/test_common.rs`. You may add new tests without asking. List the exact edits in the question.
- Greg changes CI himself: send a change of `.github/` as a separate diff (for example `c15-ci.diff`), and say that it must be committed with the main diff when the main diff needs it.
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
  - D12 was read as: `AsvoError::HttpError` and `AsvoApiError::BadStatus` have the same fields, but `HttpError` is an error of the file server, not of the API (Greg has not confirmed this reading);
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

## Waiting for a decision by Greg

- 🔴 **O5: short flags differ between commands.** All of them are in 2.5.1 except `-o` for `submit-conv --output`.

  | Short | Meaning in one place | Meaning in another |
  |---|---|---|
  | `-f` | `--delivery-format` (all `submit-*` except `submit-volt`) | `--from-channel` (`submit-volt`) |
  | `-o` | `--output` (`submit-conv`, new in 3.0) | `--output-mode` (`submit-image`, `submit-image-from-job`); `--offset` (`submit-volt`) |
  | `-r` | `--no-resume` (`download`) | `--allow-resubmit` (`submit-*`) |
  | `-n` | `--no-colour` (`list`, `wait`) | `--dry-run` (`download`, `submit-*`, `cancel`) |
  | `-d` | `--download-dir` (`download`) | `--delivery` (`submit-*`) |

  Options: **A** leave them all and document them in a README table (no script breaks; recommended); **B** change only the new `-o` of `submit-conv`; **C** change the inherited ones (breaks scripts that use them; Greg must give the letters).
- **D12** (above) is Greg's to confirm.

## Waiting for the API developer

- **O1/O7:** typify merges the two inline enums titled `Delivery`, so `dug` is accepted for conversion and imaging (Greg: wait for the API fix).
- **A13 (`days: null`)** and the questions below.

### A13 — `days: null` (waits for the API developer)

- The schema type is `integer | null`, default 30. A request has three states: key absent, `null`, or 1–30.
- Ask the API developer what `null` means. Proposed design if it is "no day limit": Rust `enum Days { ApiDefault, NoLimit, Last(NonZeroU64) }` in `JobsFilter`; Python `days=None` stays the API default and a module constant sends `null`; CLI `--days all` sends `null`.

## Open, not started

Logic:
- Lower: Scratch re-download after a move gives `FilesNotReachable` (now `DestinationExists` if the target is there); a lower-case `z` in a timestamp gets `Z` appended and fails the listing.

Not reviewed yet: `src/python/types.rs`, `params.rs`, `typed.rs` (and `error.rs` after the C13 change); `tests/cli.rs` and the Python tests in detail; `tools/*.sh`, `scrub_recording.py`, the Dockerfile, the release and docs workflows.

Not tested in the sandbox: CI itself (the maturin wheel jobs on Linux and macOS, the Docker build, the MSRV job: Rust 1.88 is not in the sandbox, and the highest declared MSRV in the `xtask` dependencies is 1.85); the non-Unix code of `no_follow`.

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
- The voltage fields (`delivery`, `delivery_format`, `channel_range`).
- The flow-2 `obs_id`.
- `JobFile.type` uses its own `Type` enum.
- A typed `obs_id` in `JobDetailResponse`.
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
