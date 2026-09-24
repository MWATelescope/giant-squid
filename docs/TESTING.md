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
ranges, `require_equals` booleans).

### Layer 2 - mock server tests with httpmock

`httpmock` (0.8.x, `record` feature) runs a real local HTTP server with a
synchronous API, which suits the blocking `reqwest` client. It is a
dev-dependency, so `Cargo.lock` must be regenerated (`./check.sh` does this
via `cargo update`) and committed - CI runs `cargo test --locked` and will
fail on a stale lock file.

Pointing the client at the mock server needs no production code change: the
base URL of every API call already comes from `MWA_ASVO_HOST`. The one thing
that did need changing is TLS. Both `reqwest` clients were built with
`https_only(true)`, which rejects the mock server's `http://127.0.0.1:PORT`
address outright. `require_tls()` in `client.rs` now derives that flag from
the configured host's scheme, so the default host and any `https://` host
stay HTTPS-only, while an explicitly configured `http://` host - a mock
server, or a plain-HTTP dev instance - is allowed.

`src/test_common.rs` holds the harness (`#[cfg(test)]`, and also pulled into
`tests/common/mod.rs` for the subprocess tests). `TestEnv` starts a mock server,
points `MWA_ASVO_HOST` at it, redirects `HOME` to a temporary directory so
the real token cache is never touched, and optionally writes a valid cached
session so `AsvoClient::new()` skips the login round trip. Because those
variables are process-wide and cargo runs tests in parallel threads, the
harness holds a mutex for the life of each test.

Recording is manual and never runs in CI. `record_login_and_get_jobs` in
`src/asvo/apiv2/client/test.rs` is `#[ignore]`d and its section notes carry
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
`src/asvo/apiv2/client/test.rs`:

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

`src/asvo/test.rs` covers the download path as far as it can go today:
an unknown job ID, a job that is not ready, an unknown obsid, an obsid whose
only job is unfinished, and an obsid with several ready jobs. Pagination is
covered too - `src/asvo/apiv2/client/test.rs` serves two pages by matching on the
`offset` the client sends, so no per-call response variation is needed.

Still to write: a successful download, hash verification, tar handling and
resume via `RANGE`. Those are blocked - see below.

### Layer 2b - the binary, end to end

`tests/cli.rs` runs the built binary as a subprocess (via
`CARGO_BIN_EXE_giant-squid`, so no extra dependency). It is the only test
file left in `tests/`, because Cargo sets that variable only for integration
tests. It runs against a mock server.
It covers what only `main` can answer: `--dry-run` making no request at all,
exit codes, the `No obsids specified` and job-ID-instead-of-obsid guards,
`--json` output, state filtering, a rejected cancellation being logged
without failing the run, and the `GIANT_SQUID_DELIVERY` /
`GIANT_SQUID_DELIVERY_FORMAT` defaults - which clap reads at parse time, so
they can only be set before the process starts.

`CliEnv` passes the child's environment on the `Command` itself and mutates
nothing process-wide, so these tests run in parallel, unlike the in-process
ones. Every variable the client reads is set or cleared explicitly, so a
developer's own settings cannot leak into a run.

### Layer 3 - opt-in live tests

A small `#[ignore]`d or feature-gated suite that talks to a real server, run
by hand only. Once layers 1 and 2 exist, the `MWA_ASVO_API_KEY` secret can be
removed from `run-tests.yaml` and `coverage.yml`.

## Test environment isolation

The client reads process-wide environment variables, and cargo runs tests in
parallel threads within one process. Any test that sets `MWA_ASVO_HOST`,
`MWA_ASVO_API_KEY`, `MWA_ASVO_API_TIMEOUT` or `HOME` must therefore be
serialised (for example with `serial_test`), or the client must take an
explicit config value instead of reading the environment.

The harness also sets `GIANT_SQUID_DOWNLOAD_RETRY_SECS=0`. A download
classifies most failures as transient and retries them under exponential
backoff for fifteen minutes; a test that deliberately triggers one (the hash
mismatch test) would otherwise sit in backoff for that whole time while
holding the lock, stalling every other test in the binary. That is exactly
what happened before the retry window was made configurable.

`HOME` also needs a per-test temporary directory: the token cache is
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
effectively: the playback tests in `src/asvo/apiv2/client/test.rs` drives the real client over the fixture, so
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
endpoint paths now live in `pub const ENDPOINT_*` in `client.rs`, used both
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

`product_to_files` in `client.rs` maps that to `AsvoFilesArray`, so
`AsvoJob.files` is populated, downloads work, and `list` can show File Size
and Delivery. Because nothing about `product` is guaranteed by type, the
mapping is tolerant: an entry with an unrecognised or missing delivery type
is skipped with a warning, a missing `size` becomes 0 (it only feeds
progress reporting), and a job left with nothing usable reports
`AsvoError::NoFiles` rather than half-downloading. Scratch and DUG
deliveries carry a `path` instead of a `url`; those are mapped but have no
recorded sample yet.

the playback tests in `src/asvo/apiv2/client/test.rs` replays the recording and pins the mapping against that
real payload. `src/asvo/test.rs` now runs a download end to end, with the
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
  configurable via `GIANT_SQUID_DOWNLOAD_RETRY_SECS` (default 900s), which
  the test suite sets to 0.

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
  rather than appending. Tests in `src/asvo/test.rs` cover resume, an
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
- The schema is inconsistent between the two imaging endpoints: `pol` is the
  `Polarization` enum on `imaging_job` (default `XXYY`) but a free-form
  string on `image_from_job` (default `XX,YY`). giant-squid follows each
  endpoint, and only validates the enum one. Worth raising with the API dev.

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
| 4 | Drop `MWA_ASVO_API_KEY` from CI | Done |
| 4b | Fixture schema validation in CI | Covered by playback, see below |
| 5 | End-to-end CLI tests against the mock server | Done |
| 6 | Uniform `--dry-run` output (endpoint plus JSON body) | Done |
