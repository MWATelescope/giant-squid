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

`tests/common/mod.rs` holds the harness. `TestEnv` starts a mock server,
points `MWA_ASVO_HOST` at it, redirects `HOME` to a temporary directory so
the real token cache is never touched, and optionally writes a valid cached
session so `AsvoClient::new()` skips the login round trip. Because those
variables are process-wide and cargo runs tests in parallel threads, the
harness holds a mutex for the life of each test.

Recording is manual and never runs in CI. `tests/record.rs` is `#[ignore]`d
and its module docs carry the exact command; in outline it starts a mock
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
`tests/apiv2_client.rs`:

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

`tests/download.rs` covers the download path as far as it can go today:
an unknown job ID, a job that is not ready, an unknown obsid, an obsid whose
only job is unfinished, and an obsid with several ready jobs. Pagination is
covered too - `tests/apiv2_client.rs` serves two pages by matching on the
`offset` the client sends, so no per-call response variation is needed.

Still to write: a successful download, hash verification, tar handling and
resume via `RANGE`. Those are blocked - see below.

### Layer 2b - the binary, end to end

`tests/cli.rs` runs the built binary as a subprocess (via
`CARGO_BIN_EXE_giant-squid`, so no extra dependency) against a mock server.
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

It refuses to write the output if anything still looks like a JWT or an email
address, so a fixture cannot be committed half-scrubbed. Two fields are
deliberately left alone: a job's own `id`, which is not a secret and which
the tests assert on, and the login request's `login` field, which carries the
client version string rather than a username - rewriting it would stop the
recorded request matching what the client sends.

A CI job should validate recorded request and response bodies against
`src/asvo/apiv2/openapi-schema.json`, extending the existing
`openapi-drift-check` job. That keeps the fixtures honest: if the schema
changes, stale recordings fail rather than silently testing an API that no
longer exists.

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

## `--dry-run` as it stands

Nine commands already accept `--dry-run`, but it short-circuits before the
request body is built, so it exercises none of the mapping code, and what it
prints varies per command: `submit-vis` and `submit-meta` print only a count,
`submit-image` prints a hand-picked subset of arguments, `download` prints
parsed IDs plus `keep_zip` and `hash`.

Now that `to_params()` exists, dry-run could instead print the resolved
endpoint and the serialised JSON body for each obsid. That would be uniform
across commands, more useful to users, and directly snapshot-testable.
Deferred - no behaviour change made yet.

## Blocked: downloads do not work on apiv2

`job_detail_to_asvo_job` sets `AsvoJob.files` to `None` unconditionally,
because the `product` field carrying the file listing and download links is
typed in the schema as a free-form object (`additionalProperties: true`)
with no documented shape. Consequences:

- every `giant-squid download` ends in `AsvoError::NoFiles`, whatever the
  job's state;
- `list` shows blank File Size and Delivery columns;
- the download tests can only pin error paths, and hash verification, tar
  handling and resume are untestable.

The API dev is changing the API to define this properly; revisit once that
lands. Unblocking it needs one real `product` payload from a completed job on
test-asvo. `tests/record.rs` will capture it: run the recorder while a
completed job is in the account's history, then read `product` out of the
scrubbed recording. Once its shape is known, map it to `AsvoFilesArray`
(`type`, `url`, `path`, `size`, `sha1`) and the download tests can serve
the file from the same mock server.

## Defects the tests surfaced

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
| 3d | Successful download, hash, tar and resume tests | Blocked on the `product` shape |
| 3c | Recorded fixtures from test-asvo, replayed offline | Needs a recording run |
| 4 | Drop `MWA_ASVO_API_KEY` from CI | Done |
| 4b | Fixture schema validation in CI | Needs fixtures first |
| 5 | End-to-end CLI tests against the mock server | Done |
| 6 | Optional: uniform `--dry-run` output plus snapshot tests | Deferred |
