# giant-squid

A Rust client for the MWA ASVO, with Python bindings (`mwa-giant-squid`) built from the `python` feature by maturin. See `docs/PYTHON.md`.

## Architecture

- The MWA ASVO FastAPI (its OpenAPI schema) dictates the library: its methods, types, names, defaults and limits.
- The Rust CLI uses the library as much as possible. Put logic in the library, not in the CLI.
- The Python CLI (`mwa_giant_squid_cli`) is only a wrapper of the Rust CLI. It adds no logic of its own.

## Versioning

- giant-squid strictly uses Semantic Versioning (https://semver.org). A breaking change to the library API, the Python API or the CLI (commands, options, output, exit codes) needs a new major version. A new feature that is backward compatible needs a new minor version. A bug fix needs a new patch version.
- The Rust crate, the Python package and the CLI have the same version (`Cargo.toml`; `pyproject.toml` takes it from there).

## Before you hand over code

Rust:
- Run `uv run ./check.sh`. It runs `cargo fmt --check`, clippy with `-D warnings` on every feature set, and `cargo doc`. CI runs the same script. `uv run` gives it the Python of `.venv`, which the clippy step of the Python module needs.
- Run `cargo test`.

Python:
- Run `uv sync` after a Rust change, to rebuild the module.
- Run `uv run ruff format`, `uv run ruff check` and `uv run ty check`. The line length (120) is set in `pyproject.toml`.
- Run `uv run pytest`.
- Use Google-style docstrings.
