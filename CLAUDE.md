# giant-squid

A Rust client for the MWA ASVO, with Python bindings (`mwa-giant-squid`) built from the `python` feature by maturin. See `docs/PYTHON_BINDINGS.md`.

## Architecture

- The MWA ASVO FastAPI (its OpenAPI schema) dictates the library: its methods, types, names, defaults and limits.
- The Rust CLI uses the library as much as possible. Put logic in the library, not in the CLI.
- The Python CLI (`mwa_giant_squid_cli`) is only a wrapper of the Rust CLI. It adds no logic of its own.

## Before you hand over code

Rust:
- Run `uv run ./check.sh`. It runs `cargo fmt --check`, clippy with `-D warnings` on every feature set, and `cargo doc`. CI runs the same script. `uv run` gives it the Python of `.venv`, which the clippy step of the Python module needs.
- Run `cargo test`.

Python:
- Run `uv sync` after a Rust change, to rebuild the module.
- Run `uv run ruff format`, `uv run ruff check` and `uv run ty check`. The line length (120) is set in `pyproject.toml`.
- Run `uv run pytest`.
- Use Google-style docstrings.
