# giant-squid `apiv2` review: handoff for the next session

## Start here

1. Make a fresh clone of `MWATelescope/giant-squid`, branch `apiv2`.
2. Check the head commit:
   - If Greg committed step 7, the head is a commit after `9c35411` "step 5(part 2)".
   - If not, the head is still `9c35411`. Then ask Greg for `step7-all.diff`, or for the step diffs 7a–7f, and apply them.
3. Run the full check (see "How to check") before you change anything. Expected result:
   - 308 Rust unit tests and 51 CLI tests pass; 1 unit test and 18 live tests are ignored;
   - 155 Python tests pass;
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

## Step 7 (delivered; check that it is in the branch)

- **7a:**
  - `AsvoClient::download_job_from` and `download_obs_from`; Python `download_job`/`download_obs` take `jobs=`. The CLI gets the job list once.
  - Downloads run in a local rayon pool.
  - A Scratch move works across file systems (copy, then remove).
  - The CLI-only constants moved to `cli`.
  - The job table is monochrome with `NO_COLOR`.
- **7b:** `AsvoApiError::BadStatus { status: u16, message }`, the same fields as `HttpError`. The Python attribute is `status`.
- **7c:** a bad `MWA_ASVO_API_TIMEOUT` or `GIANT_SQUID_DOWNLOAD_RETRY_SECS` is an error. `client_config_from_env` returns `AsvoError`.
- **7d:** `pol`, `nmiter`, `image_size` and `source_job_id` use the schema types. `DownloadOptions.download_dir` stays a `&str`.
- **7e:** one parser for every named value: case, spaces, `-` and `_` do not matter. The values are listed in schema order. `--job-states` and `--job-types` show `[possible values]`.
- **7f:** one public path for each item.
  - Crate root: the client, jobs, errors, downloads, the environment helpers, `ObsId`/`ObsIdError`, the parsers and `API_PREFIX`.
  - `mwa_giant_squid::mwa_asvo::api`: `openapi`, `job_args`, `validate` and `schema_enums`.
  - `mwa_giant_squid::mwa_asvo::error_response`: the `--json` error codes.

## Open items, in Greg's order

### 1. A13 — `days` (design is still to be discussed)

- The schema type is `integer | null`, with a default of 30. A request has three states: key absent (the API default), `null`, or 1–30.
- Today the library cannot send `null`. Greg wants Rust and Python to allow it.
- First ask the API developer what `null` means. Is it "no day limit"? Does the 30-day maximum still apply?
- Proposed design, if `null` is "no day limit":
  - Rust: `enum Days { ApiDefault, NoLimit, Last(NonZeroU64) }` in `JobsFilter`;
  - Python: `days=None` stays the API default, and a module constant sends `null`;
  - CLI: `--days` is unset by default (the API default), and `--days all` sends `null`. Today `--days` always sends 30 (A13 in the review).

### 2. CI (Greg changes the CI himself: send proposals as diffs for him to review)

- **CI1:** no workflow runs `cargo clippy -D warnings` or `cargo fmt --check`.
- **CI2:** no workflow builds `--no-default-features` (the library only) or runs clippy with `--features python`.
- **CI3:** `check.sh` runs `cargo clean`, `cargo update` and `clippy --fix`, and has the typo "Upadting". Proposal: a check-only script.
- **CI4:** the action versions are mixed (`checkout@v4` vs `@v6`; `rust-toolchain@stable` vs `@v1`), and `releases.yaml` has a commented-out ARM job.
- **CI5:** the Dockerfile builder installs `clang`, `jq`, `lcov`, `unzip`, `zip`, `automake` and `libtool`. They are probably not needed; ask Greg first.
- **Possible:** make the build dependency `serde_json` optional (only `regen-openapi` uses it). This needs a `regen-openapi` build to test.

### 3. Logging

- **L1:** mixed format style. About 100 positional `"{}"` arguments in `run.rs`, `client/mod.rs` and `download/mod.rs`. Proposal: inline arguments everywhere.
- **L5:** the three failure summaries use different wording:
  - downloads: "N of M downloads failed; see the errors above.";
  - submissions: "N of M job submissions failed";
  - cancel: "N of M cancel requests failed".
  
  Proposal: one pattern, based on the submission wording.
- **L6:** `send_checked` and `create_file_logged` log at `error!` and then return the error, so the error is reported twice. Proposal: remove these lines, or make them `debug!`.

### 4. Test coverage

- **Legacy JSON:** add a new test for the 4 job types without one (`DownloadMetadata`, `DownloadVoltage`, `CancelJob`, `DownloadBeamformer`) and for no type (`Unknown`), checked against tag `v2.5.1`.

### Items to leave as they are (Greg agreed)

- D7: the Python mapping of error kinds that are never raised.
- U13: the Python error mapping repeats `job_id()`/`obs_id()`.
- The hidden `download --hash` no-op.

## Questions still with the API developer

These are listed in `docs/PYTHON_BINDINGS.md`, section "For the API developer":
- the `obs_id` minimum (888888889, but an Obs ID has 10 digits);
- `clean_iterations` has no minimum;
- `dug` delivery for conversion and imaging;
- the voltage fields (`delivery`, `delivery_format`, `channel_range`);
- the flow-2 `obs_id`;
- `JobFile.type` uses its own `Type` enum;
- a typed `obs_id` in `JobDetailResponse`;
- `staging_count`;
- the meaning of `days: null` (see item 1).

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
- **Before a commit that touches the API client:** Greg runs `tools/run_live_tests.sh` against test-asvo. This matters especially for the removal of the cookie jar.
