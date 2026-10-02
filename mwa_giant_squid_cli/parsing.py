# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

"""Parsers for command line values: enums, times and booleans."""

import argparse
import re
from collections.abc import Callable
from datetime import datetime, timezone
from typing import Any, TypeVar

from mwa_giant_squid import AsvoJobState, AsvoJobType

from .constants import DATE_ONLY_FORMAT

# The job states that `--job-states` accepts, by the name the Rust command accepts (letters only, lower case).
# `AsvoJobState.Error` stands for every job with an error.
JOB_STATE_NAMES = {
    "queued": AsvoJobState.Queued,
    "waitcal": AsvoJobState.WaitCal,
    "staging": AsvoJobState.Staging,
    "staged": AsvoJobState.Staged,
    "downloading": AsvoJobState.Downloading,
    "preparing": AsvoJobState.Preparing,
    "preprocessing": AsvoJobState.Preprocessing,
    "imaging": AsvoJobState.Imaging,
    "delivering": AsvoJobState.Delivering,
    "ready": AsvoJobState.Ready,
    "error": AsvoJobState.Error,
    "expired": AsvoJobState.Expired,
    "cancelled": AsvoJobState.Cancelled,
}

# The job types that `--job-types` accepts, in the same form. The Rust command takes any other text as the
# "unknown" type, which cannot be filtered by, so this program refuses it.
JOB_TYPE_NAMES = {
    "conversion": AsvoJobType.Conversion,
    "downloadvisibilities": AsvoJobType.DownloadVisibilities,
    "downloadmetadata": AsvoJobType.DownloadMetadata,
    "downloadvoltage": AsvoJobType.DownloadVoltage,
    "downloadvoltages": AsvoJobType.DownloadVoltage,
    "downloadbeamformer": AsvoJobType.DownloadBeamformer,
    "canceljob": AsvoJobType.CancelJob,
    "imaging": AsvoJobType.Imaging,
}

# The type of an enum member, and of a value in a table of names.
T = TypeVar("T")

# Text that a boolean option accepts.
TRUE_TEXT = frozenset({"true", "yes", "on", "1"})
FALSE_TEXT = frozenset({"false", "no", "off", "0"})

# A time with a "Z" for UTC at the end. Python 3.10 does not read it.
UTC_SUFFIX = re.compile(r"[zZ]$")


def sanitize_identifier(text: str) -> str:
    """Reduce text to lower case letters only, as the Rust command does before it looks up a name.

    Args:
        text: The text.

    Returns:
        The lower case letters of ``text``. ``download_visibilities`` and ``Download-Visibilities`` both become
        ``downloadvisibilities``.
    """
    return re.sub(r"[^a-z]", "", text.lower())


def enum_values(enum_type: type) -> list[str]:
    """List the API values of the members of an enum.

    Args:
        enum_type: The enum class.

    Returns:
        The text of each member (``str(member)``), in alphabetical order. The Rust command lists them in the same
        order.
    """
    members = (getattr(enum_type, name) for name in dir(enum_type))
    return sorted({str(member) for member in members if isinstance(member, enum_type)})


def enum_parser(enum_type: type[T], description: str) -> Callable[[str], T]:
    """Make a parser for the members of an enum, by the text of their API value or their name.

    Args:
        enum_type: The enum class.
        description: What the value is, for the error message.

    Returns:
        A function that returns the member for some text, ignoring case, or raises ``ArgumentTypeError``.
    """
    members = {name: member for name in dir(enum_type) if isinstance(member := getattr(enum_type, name), enum_type)}
    lookup = {sanitize_identifier(str(member)): member for member in members.values()}
    lookup.update({sanitize_identifier(name): member for name, member in members.items()})
    choices = ", ".join(sorted({str(member).lower() for member in members.values()}))

    def parse(text: str) -> T:
        try:
            return lookup[sanitize_identifier(text)]
        except KeyError:
            msg = f"invalid {description} '{text}' (choose from {choices})"
            raise argparse.ArgumentTypeError(msg) from None

    return parse


def name_list_parser(names: dict[str, T], description: str) -> Callable[[str], list[T]]:
    """Make a parser for a comma-separated list of names, ignoring case, spaces, hyphens and underscores.

    Args:
        names: The accepted names (letters only, lower case) and the value of each.
        description: What the values are, for the error message.

    Returns:
        A function that returns the values, or raises ``ArgumentTypeError``.
    """

    def parse(text: str) -> list[T]:
        values = []
        for item in text.split(","):
            try:
                values.append(names[sanitize_identifier(item)])
            except KeyError:
                msg = f"invalid {description} '{item}'"
                raise argparse.ArgumentTypeError(msg) from None
        return values

    return parse


def parse_utc_time(text: str) -> datetime:
    """Parse a time for ``list --date-from`` and ``--date-to``.

    Args:
        text: RFC 3339 (for example ``2026-09-01T00:00:00Z``), or a date alone (``2026-09-01``), which is
            midnight UTC.

    Returns:
        The time, with a time zone.

    Raises:
        ArgumentTypeError: The text is neither of these. A date and time with no offset
            (``2026-09-01T12:00:00``) is refused rather than guessed.
    """
    message = "not a time: use RFC 3339 (2026-09-01T00:00:00Z) or a date (2026-09-01)"
    try:
        return datetime.strptime(text, DATE_ONLY_FORMAT).replace(tzinfo=timezone.utc)
    except ValueError:
        pass
    try:
        parsed = datetime.fromisoformat(UTC_SUFFIX.sub("+00:00", text))
    except ValueError:
        raise argparse.ArgumentTypeError(message) from None
    if parsed.tzinfo is None:
        raise argparse.ArgumentTypeError(message)
    return parsed


def parse_bool(text: str) -> bool:
    """Parse the value of a boolean option such as ``--join-channels=false``.

    Args:
        text: ``true`` or ``false`` (also ``yes``, ``no``, ``on``, ``off``, ``1`` and ``0``), in any case.

    Returns:
        The value.

    Raises:
        ArgumentTypeError: The text is none of these.
    """
    lowered = text.lower()
    if lowered in TRUE_TEXT:
        return True
    if lowered in FALSE_TEXT:
        return False
    msg = f"invalid boolean '{text}' (use true or false)"
    raise argparse.ArgumentTypeError(msg)


def positive_int(text: str) -> int:
    """Parse an integer that is 1 or more.

    Args:
        text: The text.

    Returns:
        The number.

    Raises:
        ArgumentTypeError: The text is not an integer, or is less than 1.
    """
    try:
        value = int(text)
    except ValueError:
        msg = f"not a valid integer: '{text}'"
        raise argparse.ArgumentTypeError(msg) from None
    if value < 1:
        msg = f"must be 1 or more, not {value}"
        raise argparse.ArgumentTypeError(msg)
    return value


def default_text(value: Any) -> str:
    """The text of a default value, as the command line takes it.

    Args:
        value: The default: a number, a boolean or an enum member.

    Returns:
        The text, in lower case for a boolean, so that ``True`` is ``true``.
    """
    return str(value).lower() if isinstance(value, bool) else str(value)
