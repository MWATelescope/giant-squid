"""Tests that the Python API matches the Rust API where possible.

The manifest (``parity_manifest.py``) lists each public item of the Rust library and what the Python module does with
it. These tests keep the manifest true: it must agree with the Python module that is installed and with the Rust
source (``src/lib.rs`` and the two error enums). When the Rust API or the Python API changes, a test here fails until
the manifest says what the change means.
"""

import re
import types
from pathlib import Path
from typing import Any

import pytest

import mwa_giant_squid

from .parity_manifest import (
    API_ERROR_VARIANTS,
    ERROR_VARIANTS,
    MANIFEST,
    OBS_ID_ERROR_VARIANTS,
    PARSE_ERROR_VARIANTS,
    PYTHON_ONLY,
    Entry,
)

REPO_ROOT = Path(__file__).resolve().parents[2]
LIB_RS = REPO_ROOT / "src" / "lib.rs"
API_ERROR_RS = REPO_ROOT / "src" / "mwa_asvo" / "api" / "error.rs"
ERROR_RS = REPO_ROOT / "src" / "mwa_asvo" / "error.rs"
OBS_ID_RS = REPO_ROOT / "src" / "obs_id" / "mod.rs"
HELPERS_RS = REPO_ROOT / "src" / "helpers" / "mod.rs"

# What `resolve` returns for a name that the module does not have.
MISSING = object()

CRATE_ROOT_PREFIX = "crate::"


def resolve(dotted: str) -> Any:
    """The object that a dotted name has in ``mwa_giant_squid``, or ``MISSING``."""
    obj: Any = mwa_giant_squid
    for part in dotted.split("."):
        obj = getattr(obj, part, MISSING)
        if obj is MISSING:
            return MISSING
    return obj


def entries_with(status: str) -> list[Entry]:
    """The manifest entries that have this status."""
    return [entry for entry in MANIFEST if entry.status == status]


def rust_root_names() -> set[str]:
    """The names that ``src/lib.rs`` re-exports at the crate root with ``pub use``."""
    text = "\n".join(line for line in LIB_RS.read_text().splitlines() if not line.lstrip().startswith("//"))
    names: set[str] = set()
    for statement in re.findall(r"\bpub use ([^;]+);", text):
        if "{" in statement:
            inner = statement[statement.index("{") + 1 : statement.rindex("}")]
            items = [item.strip() for item in inner.split(",") if item.strip()]
        else:
            items = [statement.strip().split("::")[-1]]
        names.update(item.split(" as ")[-1].strip() for item in items)
    return names


def rust_enum_variants(path: Path, enum: str) -> list[str]:
    """The variant names of a Rust enum, in the order of the file."""
    match = re.search(rf"pub enum {enum} \{{(.*?)\n\}}", path.read_text(), re.DOTALL)
    assert match, f"{enum} was not found in {path}"
    return re.findall(r"^    ([A-Z][A-Za-z0-9]*)", match.group(1), re.MULTILINE)


def test_manifest_is_well_formed() -> None:
    """Each Rust path is in the manifest once, and an item that is not done has a reason or a Python name."""
    paths = [entry.rust for entry in MANIFEST]
    assert len(paths) == len(set(paths)), f"Duplicate Rust paths: {sorted({p for p in paths if paths.count(p) > 1})}"
    for entry in MANIFEST:
        if entry.status in ("done", "pending"):
            assert entry.python, f"{entry.rust} is {entry.status} and needs a Python name"
        else:
            assert entry.note, f"{entry.rust} is {entry.status} and needs a note"


@pytest.mark.parametrize("entry", entries_with("done"), ids=lambda e: e.rust)
def test_done_items_exist(entry: Entry) -> None:
    """An item that the manifest calls done is in the module."""
    assert entry.python is not None
    assert resolve(entry.python) is not MISSING, f"{entry.rust}: mwa_giant_squid.{entry.python} does not exist"


@pytest.mark.parametrize("entry", entries_with("pending"), ids=lambda e: e.rust)
def test_pending_items_do_not_exist_yet(entry: Entry) -> None:
    """An item that the manifest calls pending is not in the module. When it is, the status must be done."""
    assert entry.python is not None
    assert resolve(entry.python) is MISSING, f"{entry.rust}: mwa_giant_squid.{entry.python} exists. Make it `done`."


def test_every_python_name_is_in_the_manifest() -> None:
    """A public name of the module is the counterpart of a Rust item, or is listed as Python only."""
    known = set(PYTHON_ONLY)
    for entry in MANIFEST:
        if entry.python and entry.status != "excluded":
            known.add(entry.python.split(".")[0])
    public = [
        name
        for name in dir(mwa_giant_squid)
        if not name.startswith("_") and not isinstance(getattr(mwa_giant_squid, name), types.ModuleType)
    ]
    unknown = sorted(set(public) - known)
    assert not unknown, f"Python names with no Rust counterpart in the manifest: {unknown}"


def test_python_only_names_exist() -> None:
    """A name that is listed as Python only is in the module."""
    for name in PYTHON_ONLY:
        assert resolve(name) is not MISSING, f"{name} is listed as Python only but is not in the module"


def test_every_crate_root_export_is_in_the_manifest() -> None:
    """Each name that ``src/lib.rs`` re-exports has an entry, so a new Rust export cannot be forgotten."""
    in_manifest = {e.rust[len(CRATE_ROOT_PREFIX) :] for e in MANIFEST if e.rust.startswith(CRATE_ROOT_PREFIX)}
    missing = sorted(rust_root_names() - in_manifest)
    assert not missing, f"Exported by src/lib.rs, with no manifest entry: {missing}"


def test_every_crate_root_entry_is_exported() -> None:
    """Each ``crate::Name`` entry (without a member) is something that ``src/lib.rs`` exports."""
    root = {
        e.rust[len(CRATE_ROOT_PREFIX) :]
        for e in MANIFEST
        if e.rust.startswith(CRATE_ROOT_PREFIX) and "::" not in e.rust[len(CRATE_ROOT_PREFIX) :]
    }
    unknown = sorted(root - rust_root_names())
    assert not unknown, f"Manifest entries that src/lib.rs does not export: {unknown}"


@pytest.mark.parametrize(
    ("path", "enum", "listed"),
    [
        (API_ERROR_RS, "AsvoApiError", API_ERROR_VARIANTS),
        (ERROR_RS, "AsvoError", ERROR_VARIANTS),
        (OBS_ID_RS, "ObsIdError", OBS_ID_ERROR_VARIANTS),
        (HELPERS_RS, "ParseError", PARSE_ERROR_VARIANTS),
    ],
)
def test_error_variants_match_the_rust_enum(path: Path, enum: str, listed: list[str]) -> None:
    """The variants in the manifest are those of the Rust enum."""
    assert sorted(rust_enum_variants(path, enum)) == sorted(listed)
    in_manifest = {e.rust for e in MANIFEST if e.rust.startswith(f"crate::{enum}::")}
    assert in_manifest == {f"crate::{enum}::{variant}" for variant in listed}
