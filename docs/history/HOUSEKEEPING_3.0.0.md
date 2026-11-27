# giant-squid 3.0.0: housekeeping

These changes of 3.0.0 do not change what the command, the library or the Python module do. They were the
"Housekeeping in 3.0.0" section of the changelog.

* Tests moved into their own module: a module that has tests is a folder with `mod.rs` (the code) and `tests.rs`
  (the tests).
* A test suite that runs offline against a mock MWA ASVO server (recorded responses are replayed), so that no
  test submits or cancels a job or downloads data. See [TESTING.md](TESTING.md).
* Tests that keep decisions from being undone by accident: only parameters of the API schema are sent, only
  end-user endpoints are called. See [TESTING.md](TESTING.md).
* `tools/generate_openapi.sh` regenerates the library's types from the schema, and keeps the request and response
  schemas of each operation. `cargo xtask regen-openapi` writes the help texts of the schema (`x-cmd-help`) into
  `openapi.rs` (the `cmd_help` module and the `cmd_help_text!` macro).
* `tools/run_live_tests.sh` runs the opt-in live tests against the MWA ASVO test server.
* CI checks that the generated API types and the Python type stubs are up to date, builds and tests the Python
  package on Linux and macOS, and attaches the wheels to releases.
* The crates.io package does not include the files that only the Python package uses.
* Unused dependencies removed: `bytes`, `litemap`, `zerofrom`, `time`, `time-core`, `reqwest-retry`,
  `task-local-extensions`, `reqwest_cookie_store`, and the dev-dependency `rand`. The API client keeps no cookie
  jar: it sends the session token in its own `Cookie` header.
* The unmaintained `backoff` crate (RUSTSEC-2025-0012, and `instant`, RUSTSEC-2024-0384) is replaced by a small
  exponential backoff with jitter in the download code. The waits are as before: from 0.5 s, growing by 1.5 times,
  at most 60 s, each moved by up to half of itself at random.
* The job table of `list` and `wait` is drawn with `comfy-table` (was `prettytable-rs`, whose `term` dependency is
  unmaintained). The layout and the colours are the same.
* `examples/list_jobs.py` uses the 3.0.0 Python API (`MwaAsvoClientConfig`, `JobsFilter`), as
  `examples/list_jobs.rs` does with the Rust library.
