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

### Layer 2 - record and playback with httpmock

`httpmock` (0.8.x, `record` feature) runs a real local HTTP server with a
synchronous API, which suits the blocking `reqwest` client.

Recording (manual, never in CI):

1. Start a mock server with a forwarding rule pointing at test-asvo.
2. Point giant-squid at it by setting `MWA_ASVO_HOST` to the mock server's
   base URL - no code change needed, because every API call already derives
   its base URL from that variable.
3. Exercise the command, then save the recording to `tests/fixtures/`.

Playback (in CI): a fresh `MockServer` loads the recording and serves the
recorded interactions on its own base URL, so the tests set `MWA_ASVO_HOST`
the same way. Proxy mode is only needed when a client's base URL cannot be
overridden, which does not apply here.

Recordings cover the happy paths. Error paths cannot be recorded from a
healthy server, so these stay hand-written `httpmock` mocks:

- `401` and `AUTH_INVALID_TOKEN` / `AUTH_REQUIRED`, including the re-login
  and retry path in `send_authed`.
- Structured `ErrorResponse` bodies mapping to `AsvoApiError::ApiError`.
- Non-JSON error bodies mapping to `AsvoApiError::BadStatus`.
- Download `404`, HTTP errors, hash mismatch, and resume via `RANGE`.

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

Layer 1 tests touch no environment variable, with one exception: the
`GIANT_SQUID_DELIVERY` and `GIANT_SQUID_DELIVERY_FORMAT` defaults are read by
clap at parse time and cannot be injected per call, so those tests are
deferred to the serialised group in layer 2.

## Fixture hygiene

Recordings contain real access and refresh tokens, a user ID, a login name
and an email address. They must be scrubbed before being committed. A
`tools/` script should do this mechanically rather than by hand:

- replace `access_token` / `refresh_token` values with a dummy JWT whose
  `exp` claim is far in the future,
- replace `user_id`, `user_login`, `user_email` with fixed test values,
- normalise timestamps so playback is deterministic.

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
| `GIANT_SQUID_DELIVERY` / `GIANT_SQUID_DELIVERY_FORMAT` defaults | 2 |
| `--dry-run` submits nothing | 2 |
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

## Defects the tests surfaced

- `submit-image --pol` defaults to `XX,YY`, but the schema's `Polarization`
  type only accepts `XX`, `YY` or `XXYY`. Every `submit-image` run that does
  not pass `--pol` explicitly therefore fails when the request body is built.
  `submit_image_default_pol_is_rejected_by_the_schema` pins this so it stays
  visible; correcting the default to `XXYY` would make that test fail, which
  is the point.

## Phases

| Phase | Work | Status |
| --- | --- | --- |
| 1 | Move `Args` into `src/cli/`, extract per-job-type params builders, add pure CLI tests | Done |
| 2 | Add `httpmock` dev-dependency, recording script, fixture scrubbing | Not started |
| 3 | Playback tests per endpoint, plus hand-written error-path mocks | Not started |
| 4 | Fixture schema validation in CI; drop `MWA_ASVO_API_KEY` from CI | Not started |
| 5 | Optional: uniform `--dry-run` output plus snapshot tests | Deferred |
