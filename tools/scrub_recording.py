#!/usr/bin/env python3
"""Scrub secrets out of an httpmock recording before it is committed.

A recording captured from a live MWA ASVO contains real JWTs, the recording
user's ID, login name and email address. This script replaces them with the
fixed placeholder values the offline tests expect, so a fixture can be
committed safely.

It works line by line with regular expressions rather than parsing YAML, so
it needs no third-party packages and leaves the file's structure untouched.

Usage:
    python3 tools/scrub_recording.py <input> <output>
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

# Placeholders. These match the values in tests/common/mod.rs, so a scrubbed
# recording and a hand-written mock describe the same fictional user.
PLACEHOLDER_USER_ID = "4242"
PLACEHOLDER_USER_LOGIN = "test_user"
PLACEHOLDER_USER_EMAIL = "test_user@example.org"

# A JWT with an `exp` claim of 2036-01-01T00:00:00Z, so a scrubbed recording
# does not expire. Payload decodes to {"exp":2082758400}.
PLACEHOLDER_JWT = "notaheader.eyJleHAiOjIwODI3NTg0MDB9.notasignature"

# Anything shaped like a JWT: three base64url segments separated by dots.
JWT_PATTERN = re.compile(r"\beyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\b")

# Any email address.
EMAIL_PATTERN = re.compile(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b")

# Key/value pairs we replace wholesale. Each entry is a compiled pattern
# whose middle capture group is the value to replace, plus the replacement.
#
# The user's ID and login name are matched only inside a `"user": {...}`
# object. Two fields would otherwise be caught wrongly: a job's own `id`,
# which is not a secret and which the tests assert on, and the login
# request's `login` field, which carries the client version string
# ("giant-squidv3.0.0") rather than a username - rewriting that would make
# the recorded request stop matching what the client actually sends.
FIELD_REPLACEMENTS: list[tuple[re.Pattern[str], str]] = [
    (re.compile(r'("user_id"\s*:\s*)(\d+)()'), PLACEHOLDER_USER_ID),
    (re.compile(r'("user"\s*:\s*\{[^}]*?"id"\s*:\s*)(\d+)()'), PLACEHOLDER_USER_ID),
    (re.compile(r'("user"\s*:\s*\{[^}]*?"login"\s*:\s*")([^"]*)(")'), PLACEHOLDER_USER_LOGIN),
    (re.compile(r'("first_name"\s*:\s*")([^"]*)(")'), "Test"),
    (re.compile(r'("last_name"\s*:\s*")([^"]*)(")'), "User"),
    (re.compile(r'("password"\s*:\s*")([^"]*)(")'), "not-a-real-api-key"),
]

# Request headers that carry credentials.
COOKIE_PATTERN = re.compile(r"(mwa_(?:access|refresh)_token=)([^\s;\"']+)")

# Headers that must not be replayed.
#
# `content-length` describes the original body, and scrubbing changes the
# body's length, so replaying it makes the mock server send a header that
# contradicts what it actually writes - hyper then aborts the response with
# "payload claims content-length of 802, custom content-length header claims
# 801". The rest are hop-by-hop headers (RFC 7230 6.1): they describe the
# connection the recording was made over, not the message, so the replaying
# server has to set its own. Dropping them all lets it do that.
DROPPED_HEADERS = (
    "content-length",
    "transfer-encoding",
    "connection",
    "keep-alive",
)

HEADER_ENTRY_PATTERN = re.compile(
    r"^[ \t]*-[ \t]*name:[ \t]*(?P<name>[^\n]+?)[ \t]*\n[ \t]*value:[ \t]*[^\n]*\n",
    re.IGNORECASE | re.MULTILINE,
)


def drop_invalidated_headers(text: str) -> tuple[str, int]:
    """Remove header entries whose recorded value scrubbing invalidates.

    Args:
        text: The recording contents.

    Returns:
        A tuple of the text with those entries removed and the number
        removed.
    """
    removed = 0

    def replace(match: re.Match[str]) -> str:
        nonlocal removed
        if match.group("name").strip().lower() in DROPPED_HEADERS:
            removed += 1
            return ""
        return match.group(0)

    return HEADER_ENTRY_PATTERN.sub(replace, text), removed


def scrub(text: str) -> tuple[str, int]:
    """Replace every known secret in the recording text.

    Args:
        text: The raw recording contents.

    Returns:
        A tuple of the scrubbed text and the number of replacements made.
    """
    count = 0

    text, n = drop_invalidated_headers(text)
    count += n

    text, n = JWT_PATTERN.subn(PLACEHOLDER_JWT, text)
    count += n

    text, n = COOKIE_PATTERN.subn(rf"\g<1>{PLACEHOLDER_JWT}", text)
    count += n

    for pattern, replacement in FIELD_REPLACEMENTS:
        text, n = pattern.subn(rf"\g<1>{replacement}\g<3>", text)
        count += n

    text, n = EMAIL_PATTERN.subn(PLACEHOLDER_USER_EMAIL, text)
    count += n

    return text, count


def remaining_secrets(text: str) -> list[str]:
    """Find anything secret-looking that survived scrubbing.

    Args:
        text: The scrubbed recording contents.

    Returns:
        A list of the offending substrings, empty if the text looks clean.
    """
    leftovers = [match for match in JWT_PATTERN.findall(text) if match != PLACEHOLDER_JWT]
    leftovers += [match for match in EMAIL_PATTERN.findall(text) if match != PLACEHOLDER_USER_EMAIL]
    return leftovers


def main() -> int:
    """Scrub the input recording and write the result.

    Returns:
        0 on success, 1 if anything secret-looking remains.
    """
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("input", type=Path, help="raw recording written by tests/record.rs")
    parser.add_argument("output", type=Path, help="where to write the scrubbed fixture")
    args = parser.parse_args()

    text = args.input.read_text(encoding="utf-8")
    scrubbed, replacements = scrub(text)

    leftovers = remaining_secrets(scrubbed)
    if leftovers:
        print(f"refusing to write: {len(leftovers)} secret-looking value(s) remain", file=sys.stderr)
        for leftover in sorted(set(leftovers)):
            print(f"  {leftover}", file=sys.stderr)
        return 1

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(scrubbed, encoding="utf-8")
    print(f"wrote {args.output} ({replacements} replacements)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
