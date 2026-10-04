# giant-squid test suite plan

## Goals and constraints

1. Every test must run in GitHub CI, on all matrix platforms, with no network
   access to any MWA ASVO server.
2. No test may submit, cancel or modify a job on the dev, test or production
   MWA ASVO, and no test may download real data from Acacia.
3. Tests must pass regardless of MWA ASVO availability, congestion or schema
   version.
4. Every CLI usage form must be covered for every job type: single obsid,
   multiple obsids, obsids from a file, aliases, defaults, environment
   variable defaults, and each optional flag.

The development server (`test-asvo.mwatelescope.org`) usually runs a newer
schema than production. CI cannot reach it. Recordings bridge that gap: they
are captured manually against test-asvo when the schema changes, committed,
and replayed offline.

## Approach

Three layers, cheapest first.

### Layer 1 - pure CLI and params tests (no HTTP)

The clap `Args` enum lives in the library (`src/cli/`), so tests can parse
argument vectors directly with `Args::try_parse_from` and inspect the result.
Each job type has a flattened clap arg struct (`ConversionJobArgs`,
`ImagingJobArgs`, ...) with a `to_params()` method that produces the generated
OpenAPI request type. That makes the whole argument-to-request-body mapping a
pure function, testable without a server.

This layer covers goal 4, plus clap's own value parsers (image size, f64/i64
ranges, `require_equals` booleans), the `[possible values]` that `--help` lists
for the schema enums (`src/cli/value_enums/tests.rs`), and the names that the
help of `list --job-states` and `--job-types` offers (each must parse).

### Layer 2 - mock server tests with httpmock

`httpmock` (0.8.x, `record` feature) runs a real local HTTP server with a
synchronous API, which suits the blocking `reqwest` client. It is a
dev-dependency, so `Cargo.lock` must be regenerated (`./check.sh` does this
via `cargo update`) and committed - CI runs `cargo test --locked` and will
fail on a stale lock file.

Pointing the client at the mock server needs no special production code: the
base URL of every API call comes from `AsvoClientConfig::host` (the CLI sets
it from `MWA_ASVO_HOST`). The one thing that did need changing is TLS. Both `reqwest` clients were built with
`https_only(true)`, which rejects the mock server's `http://127.0.0.1:PORT`
address outright. `require_tls()` in `src/asvo/apiv2/client/mod.rs` now derives that flag from
the configured host's scheme, so the default host and any `https://` host
stay HTTPS-only, while an explicitly configured `http://` host - a mock
server, or a plain-HTTP dev instance - is allowed.

`src/test_common.rs` holds the harness (`#[cfg(test)]`, and also pulled into
`tests/common/mod.rs` for the subprocess tests). `TestEnv` starts a mock
server and a temporary home directory, and optionally writes a valid cached
session there so the client skips the login round trip.
`client_config(&env)` in `src/test_config.rs` builds an `AsvoClientConfig`
with the mock server as the host and the token cache in the temporary
directory, so the real token cache is never touched. No environment
variable is set, so the tests run in parallel.

Recording is manual and never runs in CI. `record_login_and_get_jobs` in
`src/asvo/apiv2/client/tests.rs` is `#[ignore]`d and its section notes carry
the exact command; in outline it starts a mock
server forwarding to the target, points the client at it, exercises a
read-only command, and saves the interactions. Run it with a throwaway
`HOME` so a fresh login is captured and the real token cache is left alone.
Only read-only endpoints are recorded: recording a submission would create a
real job on the target server, so that is deliberately not automated.

Playback loads a recording into a fresh `MockServer`, which serves the
recorded interactions on its own base URL - the same `MWA_ASVO_HOST`
injection the other tests use. Proxy mode is only needed when a client's
base URL cannot be overridden, which does not apply here.

Recordings cover the happy paths. Error paths cannot be recorded from a
healthy server, so these are hand-written `httpmock` mocks in
`src/asvo/apiv2/client/tests.rs`:

- A missing API key, a rejected login, and a failed login not being cached.
- A valid cached session being reused; an expired access token being
  refreshed; a failed refresh falling back to a fresh login; an expired
  refresh token skipping the refresh entirely.
- `AUTH_INVALID_TOKEN` driving exactly one re-login and one retry in
  `send_authed`.
- Structured `ErrorResponse` bodies mapping to `AsvoApiError::ApiError`.
- Non-JSON error bodies mapping to `AsvoApiError::BadStatus`.
- Job listing: `completed` mapping to `Ready`, `error_text` populating
  `Error`, naive timestamps and a missing `modified` being normalised, and
  unusable jobs being skipped rather than failing the listing.
- Submission posting the exact body the CLI built, to the right endpoint,
  and cancellation issuing a `DELETE` to the job resource.

`src/asvo/tests.rs` covers the download path: an unknown job ID, a job that
is not ready, an unknown obsid, an obsid whose only job is unfinished, an
obsid with several ready jobs, successful downloads, resume, stop requests,
and tar entries with unsafe paths. Stream-untar resume has three test
modules:

- `retries`: a retry in the same run, after a failed attempt. The mock
  server cannot drop a connection, so these tests give the stream-untar
  code a reader that fails part way through. The retry carries on inside
  the member that the failure stopped, or after the last finished member,
  and asks the server only for the rest of the archive. A hash mismatch
  makes the retry start again from the beginning.
- `reruns`: a new run that finds files from an earlier run on disk. The
  mock server answers the closed byte range requests (at most 64 KiB each)
  that read the tar headers and padding. A file with the wrong contents
  makes the hash check fail, and the retry fetches the whole archive.
- `sidecar`: the resume file (`.<tar name>.giant-squid-resume.json`) of a
  failed run. The tests edit the file to check that it is not used for
  another archive, after a finished file changed, or with an unsafe path.

Pagination is
covered too - `src/asvo/apiv2/client/tests.rs` serves two pages by matching on the
`offset` the client sends, so no per-call response variation is needed.

### Layer 2b - the binary, end to end

`tests/cli.rs` runs the built binary as a subprocess (via
`CARGO_BIN_EXE_giant-squid`, so no extra dependency). It is one of the two Rust
test files in `tests/` (the other is the live tests below), because Cargo sets
that variable only for integration tests. It runs against a mock server.
It covers what only `main` can answer: `--dry-run` making no request at all,
exit codes, the `No obsids specified` and job-ID-instead-of-obsid guards (and
the obsid-instead-of-job-ID guard of `wait` and `cancel`, which must send no
request), `--json` output, state filtering, a rejected cancellation being logged
without failing the run, and the `GIANT_SQUID_DELIVERY` /
`GIANT_SQUID_DELIVERY_FORMAT` defaults - which clap reads at parse time, so
they can only be set before the process starts.

`CliEnv` passes the child's environment on the `Command` itself and mutates
nothing process-wide, so these tests run in parallel, unlike the in-process
ones. Every variable the client reads is set or cleared explicitly, so a
developer's own settings cannot leak into a run.

### Layer 3 - opt-in live tests

`tests/live.rs` (18 tests) runs the built binary against a real MWA ASVO.
Every test is `#[ignore]`d, so CI never runs it. Run it by hand with
`tools/run_live_tests.sh`, which sets the target to the test server and runs:

```text
MWA_ASVO_E2E_TARGET=https://test-asvo.mwatelescope.org \
MWA_ASVO_API_KEY=<your key> \
  cargo test --test live -- --ignored --test-threads=1 --nocapture
```

Run it before a release, and after a change to anything that talks to the
server (login, listing, a submit command, `cancel`, `wait`). It last passed
in full on 2026-10-02.

It covers every command except `download` (a job is not ready in the time a
test runs): each submit command and its alias with `--allow-resubmit`,
`list` and its filters, `cancel`, `wait` on a cancelled and an unknown job,
and server rejections - a duplicate without `--allow-resubmit`, an obsid
with no data, `submit-image-from-job` from an unfinished or unknown job, an
unknown job ID (`JOB_NOT_FOUND`), a bad API key, and a cached token the
server did not issue (`AUTH_INVALID_TOKEN` / `AUTH_REQUIRED`, then a fresh
login). Job IDs come from the submit commands' `--json` output.

The production host is refused unless `MWA_ASVO_E2E_ALLOW_PRODUCTION=1` is
set. The server allows 5 logins a minute, so the tests share one token cache
(a `HOME` under `CARGO_TARGET_TMPDIR`, keyed by host and API key) and log in
at most once per run; the two authentication tests use their own `HOME` and
log in themselves. The tests take a lock, so they run one at a time. Every
job a test submits is cancelled when it ends, pass or fail. See the module
docs for details.

### Layer 4 - the Python module and the `giant-squid` launcher (pytest)

`tests/python/` has the pytest suite (150 tests) for the `mwa_giant_squid`
module (`src/python/`) and for the `giant-squid` command, which is the Rust
program run inside the module (`mwa_giant_squid_cli/`). The rules are those of the Rust tests: every request
goes to a local `pytest-httpserver` mock (the `host` and `mock_login`
fixtures in `conftest.py`), never to a real server, and a download is served
by the mock too. Run it with:

```text
uv sync            # builds the module and installs the dev tools
uv run pytest
```

| File | What it covers |
| --- | --- |
| `test_module.py` | The import, `__version__`, `reset_logging` |
| `test_client.py` | `AsvoClient`: login, `get_jobs` and `list_jobs` (every filter, the schema defaults, the days limits), the job types and states, errors (with the server's detail and suggestion), threads |
| `test_native_cli.py` | The `giant-squid` command (the Rust program run in the module), started as a process: the installed script and entry point, help, version, exit codes 0, 1 and 2, the command's own logger, `list` against the mock, and SIGINT ending the process. The behaviour of each sub-command is tested by the Rust CLI tests (Layer 3), which run the same code |
| `test_submit.py` | The seven submit methods and `cancel_job`: the body each sends, as the mock receives it; arguments checked before any request; every field of a body is in the schema, and `staging_count` is never taken or sent |
| `test_download.py` | Downloads: tar and untar, hash, resume, the progress callback, errors, and Ctrl-C |

Two things are particular to this suite:

- The Ctrl-C tests send SIGINT to the test process from a timer thread, while
  a download runs on the main thread. The `sigint_raises` fixture installs
  Python's handler for the test, because a process that a non-interactive
  shell started in the background starts with SIGINT ignored. One of them
  (`test_ctrl_c_that_lands_in_a_log_call_still_stops_the_download`) uses a
  slow log handler, so that the signal always arrives while the Rust code is
  writing a log record through Python's `logging`. That is where a
  `KeyboardInterrupt` was once lost.
- `main()` removes its log handler when it ends, and a test checks it.
  Otherwise the handler of one test writes to the standard error of a test
  that is over, and every later log record fails and prints a traceback.

CI runs the suite on Python 3.10 and 3.14 on four platforms, and on 3.11 to
3.13 on Linux (`.github/workflows/python.yaml`). That workflow also checks
`ruff`, `ty`, the stub with `mypy.stubtest`, that `mwa_giant_squid.pyi` is
what `tools/generate_stubs.sh` writes, and builds the wheels and the sdist.

## Tests that pin a decision

Some tests exist to keep a decision from being undone by accident. If one
fails, the change is probably wrong; if the decision changed, change the test
with it.

| Decision | Where it is pinned |
| --- | --- |
| Limits, names and defaults come from the OpenAPI schema | `src/asvo/apiv2/validate/tests.rs` compares each limit with `openapi-schema.json`; the CLI and Python defaults are read from the generated types |
| Only parameters that the API defines are sent (no `flags`) | `every_field_of_every_request_body_is_in_the_schema` (Rust) and `test_every_field_of_a_body_is_in_the_schema` (Python) |
| Only end-user endpoints are called; `staging_count` is never sent or exposed | `the_client_calls_only_end_user_endpoints_and_never_sends_staging_count`, `staging_count_is_not_an_option_and_not_in_any_body`, and `test_staging_count_is_not_an_argument_and_not_in_a_body` in `test_submit.py` |
| A schema enum value that is added or removed breaks the build | The `schema_enum!` macro in `src/cli/value_enums/mod.rs` |
| `list` without `--days` uses the API default, not `null` | `get_jobs_with_no_filter_uses_the_schema_defaults`, `list_days_defaults_to_the_schema_default`, `test_get_jobs_with_no_filter_sends_none` |
| `wait` and `cancel` refuse an obsid and send nothing | `waiting_for_an_obsid_is_rejected_and_nothing_is_sent`, `cancelling_an_obsid_is_rejected_and_nothing_is_sent`, `test_wait_and_cancel_refuse_an_obsid` |
| `cancel` does not say a job was cancelled; a refused cancel is a normal reply | `a_cancellation_refused_with_a_normal_reply_is_not_reported_as_cancelled` and its Python twin |
| `list --job-types` refuses text that is not a job type | `list_refuses_a_job_type_that_does_not_exist`, `text_that_is_not_a_job_type_is_an_error` |
| `--version` prints the program name, not the crate name | `the_version_has_the_name_of_the_program`, `test_the_version_is_the_modules` |
| The code of `openapi.rs` is what the schema generates | The `openapi-drift-check` job of `run-tests.yaml` |
| `--no-resume` downloads again, but a complete keep-tar file that matches the hash is still skipped | `a_partial_file_is_downloaded_again_when_no_resume_is_set`, `a_complete_and_verified_file_is_skipped_when_no_resume_is_set`, `a_rerun_with_no_resume_set_fetches_the_whole_archive`, `no_resume_ignores_the_resume_file_and_deletes_it_when_finished` |
| A retry continues the run's own partial output, even with `--no-resume` | `a_keep_tar_retry_resumes_its_own_partial_file_when_no_resume_is_set` |
| Files that a stream-untar download reuses from an earlier run are hash checked, even with `--skip-hash` | `without_a_hash_check_a_reused_file_is_still_checked`, `a_rerun_without_a_hash_check_still_checks_the_hash_from_the_resume_file` |
| A tar entry with `..`, an absolute path or no name is not written | `an_entry_with_a_parent_dir_path_is_skipped`, `an_entry_with_an_absolute_path_is_skipped`, `an_entry_with_no_name_is_skipped`, `a_rerun_carries_on_from_a_skipped_entry` |

## Test environment isolation

The library reads no environment variables. The CLI reads them in
`src/cli/config.rs` and gives the library an explicit `AsvoClientConfig` and
`DownloadOptions`. So the in-process tests set no environment variable, and
cargo can run them in parallel threads.

The download tests set `DownloadOptions::retry_duration` to zero. A download
classifies most failures as transient and retries them under exponential
backoff for fifteen minutes; a test that deliberately triggers one (the hash
mismatch test) would otherwise sit in backoff for that whole time. The CLI
tests set `GIANT_SQUID_DOWNLOAD_RETRY_SECS=0` on the child process for the
same reason.

Each test also needs its own temporary token cache: the CLI's cache is
`$HOME/.mwa-asvo/tokens.json`, shared with mwa-cli, and tests must never read
or overwrite a real developer session.

Layer 1 tests touch no environment variable at all. The
`GIANT_SQUID_DELIVERY` and `GIANT_SQUID_DELIVERY_FORMAT` defaults are read by
clap at parse time and cannot be injected per call, so they are covered in
layer 2b instead, where the value is set on the child process.

## Fixture hygiene

Recordings contain real access and refresh tokens, a user ID, a login name
and an email address. `tools/scrub_recording.py` replaces them with the same
placeholder values the hand-written mocks use, including a JWT whose `exp`
claim is in 2036 so a fixture does not expire:

```text
python3 tools/scrub_recording.py <recorded file> tests/fixtures/<name>.yaml
```

It also drops `content-length` and the hop-by-hop headers
(`transfer-encoding`, `connection`, `keep-alive`) from the recorded
messages. A recorded `content-length` describes the original body, and
scrubbing changes the body's length, so replaying it makes the playback
server send a header that contradicts what it writes - hyper then aborts the
response with "payload claims content-length of 802, custom content-length
header claims 801". The rest describe the connection the recording was made
over rather than the message, so the replaying server has to set its own.

It refuses to write the output if anything still looks like a JWT or an email
address, so a fixture cannot be committed half-scrubbed. Two fields are
deliberately left alone: a job's own `id`, which is not a secret and which
the tests assert on, and the login request's `login` field, which carries the
client version string rather than a username - rewriting it would stop the
recorded request matching what the client sends.

Recorded bodies are already validated against the schema, indirectly but
effectively: the playback tests in `src/asvo/apiv2/client/tests.rs` drive the real client over the fixture, so
each recorded response is deserialised through the types generated from
`openapi-schema.json`. If the schema is regenerated with a renamed or newly
required field, that test fails rather than the fixture silently describing
an API that no longer exists.

A dedicated JSON-schema validation job would only add value for fixture
bodies that no client call exercises, of which there are none today. It
would also mean a new dependency (a JSON-schema validator, or PyYAML in
CI), so it is deliberately not added.

## CLI coverage matrix

Per submit command (`submit-vis`, `submit-meta`, `submit-conv`,
`submit-image`, `submit-image-from-job`, `submit-volt`, `submit-bf`):

| Case | Layer |
| --- | --- |
| Long name and short alias (`sv`, `sc`, `si`, `sifj`, `sm`, `st`, `sb`) | 1 |
| Single obsid | 1 |
| Multiple obsids | 1 |
| Obsids read from a file | 1 |
| Job ID supplied where an obsid is required (must fail) | 1 |
| No obsid supplied (must fail) | 1 |
| Schema defaults applied when no flags given | 1 |
| Every optional flag set to a non-default value | 1 |
| Out-of-range values rejected by the value parser | 1 |
| `GIANT_SQUID_DELIVERY` / `GIANT_SQUID_DELIVERY_FORMAT` defaults | 2b |
| `--dry-run` submits nothing | 2b |
| Request body sent to the server | 2 |
| `--wait` polling until ready | 2 |
| Server error responses | 2 |

Plus `list` (filters by state, type, job ID, obsid, `--days`, `--json`),
`wait`, `cancel`, and `download` (job ID, obsid, `--keep-tar`, `--no-resume`,
`--skip-hash`, `--concurrent-downloads`, missing download directory).

## `--dry-run`

Every submit command's dry run prints the endpoint the request would go to
and the resolved JSON body, one per obsid, then a summary saying nothing was
sent. `cancel` prints the job resource it would `DELETE`. The body is built
by the same `to_params()` call a real submission uses, so a dry run
exercises the argument-to-request mapping rather than echoing arguments
back.

It previously short-circuited before the body was built and printed
something different per command - a count for `submit-vis` and
`submit-meta`, a hand-picked subset of arguments for `submit-image`. The
endpoint paths now live in `pub const ENDPOINT_*` in
`src/asvo/apiv2/client/mod.rs`, used both
by the client's requests and by the dry-run output, so the two cannot
disagree.

`tests/cli.rs` checks that a dry run makes no request at all, prints the
endpoint, and prints one body per obsid.

## The `product` field, and what it unblocked

`product` is typed in the schema as a free-form object, so its shape had to
come from a real response. A recording from test-asvo shows:

```json
"product": { "files": [ { "type": "acacia",
                          "url": "https://.../1115977528_30000517_meta.tar?...",
                          "size": 117016360960,
                          "sha1": "ce32e0ae..." } ] }
```

`product_to_files` in `src/asvo/apiv2/client/mod.rs` maps that to
`AsvoFilesArray`, so `AsvoJob.product.files` is populated, downloads work, and `list` can show File Size
and Delivery. Because nothing about `product` is guaranteed by type, the
mapping is tolerant: an entry with an unrecognised or missing delivery type
is skipped with a warning, a missing `size` becomes 0 (it only feeds
progress reporting), and a job left with nothing usable reports
`AsvoError::NoFiles` rather than half-downloading. Scratch and DUG
deliveries carry a `path` instead of a `url`; those are mapped but have no
recorded sample yet.

the playback tests in `src/asvo/apiv2/client/tests.rs` replay the recording and pin the mapping against that
real payload. `src/asvo/tests.rs` now runs a download end to end, with the
mock server serving the file as well as the API.

One thing the recording also showed: `job_params.obs_id` came back as a
JSON *number* here, where an earlier sample had it as a string. The client
already accepts both.

## Defects the tests surfaced

- A submit command given several obsids stopped at the first failure, so
  `submit-meta A B C` reported one error and silently never attempted B or
  C. Every obsid is now attempted, each failure is reported as it happens,
  and the run ends with a `Submitted N of M` summary plus a list of what
  failed; the exit code still reflects the failure, but only after the whole
  list has been tried. `submit_each_obsid` in the binary does this for all
  seven submit commands. `download` already ran every download before
  reporting, but exited 0 regardless - it now fails the run too.

- A hash mismatch is treated as a transient error, so a failed checksum
  re-downloads the file under backoff. Kept deliberately: a mismatch
  usually means a corrupted transfer, which a retry can fix. The window is
  configurable via `DownloadOptions::retry_duration` (default 900 s; the CLI
  reads `GIANT_SQUID_DOWNLOAD_RETRY_SECS`), which the test suite sets to 0.

- Resume was broken in three linked ways, found while writing the download
  tests, and now fixed together:
  1. The `RANGE` header value was built as `"Range: bytes=0-123"`, with the
     header name inside the value, so the header sent was
     `Range: Range: bytes=0-123`. A real server ignores that and returns the
     whole file, which was then appended to the partial file. Now the value
     is `bytes=<offset>-`, and the header is only sent when there is
     something to skip.
  2. `prepare_output_file` signalled "file already complete" by returning
     the expected size, with a comment saying the caller should return
     early - but `try_download` never checked, so a complete file was
     downloaded again. It now returns an explicit `OutputTarget`, whose
     `AlreadyDone` case the caller cannot miss.
  3. The hash was taken from the `TeeReader`, which on a resumed download
     only sees the bytes fetched this time, so it described the tail rather
     than the file. Worse, in combination with (1) it *passed*: the whole
     file was re-fetched and hashed while the file on disk had the partial
     bytes in front of it. A resumed download now verifies the assembled
     file on disk with `check_file_sha1_hash`; a fresh one still uses the
     cheaper streamed hash.

  A server that ignores the range request and answers `200` instead of
  `206` is now detected, and the download restarts from the beginning
  rather than appending. Tests in `src/asvo/tests.rs` cover resume, an
  already-complete file, a complete-but-corrupt file, `--no-resume`, and the
  ignored-range case.

- `--custom-dec -26.7`, `--robust -1.5` and `--phase-centre-dec -26.7` were
  rejected with "unknown argument '-2'": clap reads a leading `-` as a flag
  unless the command opts in. Any southern declination, negative robustness
  or negative `uvw_min` was unusable in the natural `--flag value` form; only
  `--flag=value` worked. Fixed by setting `allow_negative_numbers` on
  submit-conv, submit-image and submit-image-from-job, with tests covering
  both forms and a guard that the short flags (`-n`, `-v`) still work. This
  predates the CLI refactor - the arguments behaved the same when they lived
  on the enum variants.

- `submit-image --pol` defaulted to `XX,YY`, which the schema's
  `Polarization` type rejects - it accepts only `XX`, `YY` or `XXYY` - so
  every `submit-image` run without an explicit `--pol` failed when the
  request body was built. Fixed: the default now comes from the schema
  (`XXYY`), like every other imaging default, and a value parser rejects an
  unsupported polarisation at parse time.
- The schema was inconsistent between the two imaging endpoints: `pol` was
  the `Polarization` enum on `imaging_job` but a free-form string (default
  `XX,YY`) on `image_from_job`. Schema 1.11 made both the enum, so both
  commands validate `--pol` and default it to `XXYY`.

## Code coverage

`tools/coverage.sh` makes a local coverage report, to run before pushing. It
uses `cargo-llvm-cov`, as the "Generate Coverage report" CI workflow does, so
its Rust numbers are the same as CI's. It also runs the Python tests against
an instrumented build of the extension module, because the code they test is
the Rust in `src/python/`. The `giant-squid` launcher (`mwa_giant_squid_cli/`) is a few lines of Python,
and the script does not measure it.

```bash
cargo install cargo-llvm-cov        # once
rustup component add llvm-tools     # once
tools/coverage.sh
```

It prints two summaries, after the Rust tests and with the Python tests
added, and writes `coverage/html/index.html` and `coverage/coverage.lcov`
(git-ignored). The line coverage of `src/python/` in the second summary is
the Python tests' coverage. With a Rust toolchain that rustup did not
install, set `LLVM_COV` and `LLVM_PROFDATA` to its `llvm-cov` and
`llvm-profdata`. The script rebuilds the extension module in `.venv` with
instrumentation; run `uv sync` afterwards for a normal build. A test that
fails does not stop the script; the report covers the tests that ran, and
the script exits with the failure.

The generated `src/asvo/apiv2/openapi.rs` counts in the totals, and much of
it (builders and types for endpoints the client does not use) is never run,
so read the per-file numbers, not only the total.

## Phases

| Phase | Work | Status |
| --- | --- | --- |
| 1 | Move `Args` into `src/cli/`, extract per-job-type params builders, add pure CLI tests | Done |
| 2 | Add `httpmock` dev-dependency, test harness, recording script, fixture scrubbing | Done |
| 3 | Hand-written error-path mocks (auth, error mapping, listing, submit, cancel) | Done |
| 3b | `get_jobs` pagination, plus download-path error mocks | Done |
| 3c | Recorded fixture from test-asvo, replayed offline | Done |
| 3d | `product` mapping, plus successful download, tar and hash tests | Done |
| 3e | Resume fix and its tests | Done |
| 3f | Stream-untar resume (retries, reruns, resume file), unsafe tar paths, and their tests | Done |
| 4 | Drop `MWA_ASVO_API_KEY` from CI | Done |
| 4b | Fixture schema validation in CI | Covered by playback, see below |
| 5 | End-to-end CLI tests against the mock server | Done |
| 6 | Uniform `--dry-run` output (endpoint plus JSON body) | Done |
| 7 | Python module: mock-server tests for the client, submit, builders and downloads | Done |
| 8 | Python command (`giant-squid`) tests, and the stub, lint and wheel CI | Done |
| 9 | Live tests against test-asvo (18), run by `tools/run_live_tests.sh` | Done; last full pass 2026-10-02 |
| 10 | Tests that pin decisions (see above) | Done; extend them when a decision is made |
