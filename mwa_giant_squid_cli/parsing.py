# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

"""Parsers for command line values: enums, times and booleans."""

import argparse
import re
from collections.abc import Callable
from datetime import datetime
from typing import Any, TypeVar

import mwa_giant_squid
from mwa_giant_squid import AsvoError, AsvoJobState, AsvoJobType

# The type of an enum member.
T = TypeVar("T")

# The enums that have a list of names and a parser in the library.
NamedEnum = type[AsvoJobState] | type[AsvoJobType]

# Text that a boolean option accepts.
TRUE_TEXT = frozenset({"true", "yes", "on", "1"})
FALSE_TEXT = frozenset({"false", "no", "off", "0"})


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


def name_list_parser(enum_type: NamedEnum, description: str) -> Callable[[str], list[AsvoJobState | AsvoJobType]]:
    """Make a parser for a comma-separated list of names of a job state or a job type.

    The library decides which text is a name (it ignores case, spaces, hyphens and underscores). This function only
    splits the text and words the error.

    Args:
        enum_type: ``AsvoJobState`` or ``AsvoJobType``.
        description: What the values are, for the error message.

    Returns:
        A function that returns the members, or raises ``ArgumentTypeError``.
    """

    def parse(text: str) -> list[AsvoJobState | AsvoJobType]:
        values: list[AsvoJobState | AsvoJobType] = []
        for item in text.split(","):
            try:
                values.append(enum_type.parse(item))
            except AsvoError:
                msg = f"Invalid {description} '{item}': expected one of: {', '.join(enum_type.names())}"
                raise argparse.ArgumentTypeError(msg) from None
        return values

    return parse


def parse_utc_time(text: str) -> datetime:
    """Parse a time for ``list --date-from`` and ``--date-to`` with the module's ``parse_utc_time``.

    Args:
        text: RFC 3339 (for example ``2026-09-01T00:00:00Z``), or a date alone (``2026-09-01``), which is
            midnight UTC.

    Returns:
        The time, with a time zone.

    Raises:
        ArgumentTypeError: The text is neither of these. The message is the module's.
    """
    try:
        return mwa_giant_squid.parse_utc_time(text)
    except ValueError as error:
        raise argparse.ArgumentTypeError(str(error)) from None


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
