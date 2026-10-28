"""Tests for the `ObsId` class and the `ObsIdError` exceptions."""

import copy
import operator
import pickle
from typing import Any

import pytest

import mwa_giant_squid as gs

OBS_ID = 1065880128
LATER_OBS_ID = 1065880248
SMALLEST = 1_000_000_000
LARGEST = 9_999_999_999


def test_the_limits_are_the_smallest_and_largest_10_digit_numbers() -> None:
    """MIN and MAX are the class attributes of the library's limits."""
    assert gs.ObsId.MIN == SMALLEST
    assert gs.ObsId.MAX == LARGEST


@pytest.mark.parametrize("value", [SMALLEST, OBS_ID, LARGEST])
def test_a_valid_obs_id_is_made(value: int) -> None:
    """Both constructors accept every 10-digit number."""
    assert int(gs.ObsId(value)) == value
    assert gs.ObsId.validate(value).get() == value


def test_an_obs_id_is_made_from_another_obs_id() -> None:
    """The constructor takes an `ObsId`, as every argument does."""
    assert gs.ObsId(gs.ObsId(OBS_ID)) == OBS_ID


@pytest.mark.parametrize("value", [0, 1, SMALLEST - 1, LARGEST + 1])
def test_a_number_without_10_digits_raises_obs_id_error(value: int) -> None:
    """The error is the class of its variant, has the number, and is an ObsIdError."""
    for make in (gs.ObsId, gs.ObsId.validate):
        with pytest.raises(gs.ObsIdErrorWrongNumDigits, match="10 digits") as excinfo:
            make(value)
        error = excinfo.value
        assert isinstance(error, gs.ObsIdError)
        assert error.kind == "WrongNumDigits"
        assert error.value == value


def test_a_negative_number_is_an_overflow_error() -> None:
    """There is no negative `u64`, so Python's own error says so."""
    with pytest.raises(OverflowError):
        gs.ObsId(-1)


def test_a_float_is_a_type_error() -> None:
    """An Obs ID is a whole number."""
    with pytest.raises(TypeError):
        gs.ObsId(1065880128.0)  # ty: ignore[invalid-argument-type]


def test_from_str_reads_digits() -> None:
    """The text of an Obs ID is read as the library's `FromStr` reads it."""
    assert gs.ObsId.from_str("1065880128") == OBS_ID


def test_from_str_raises_for_text_that_is_not_a_number() -> None:
    """The kind is `Parse`."""
    with pytest.raises(gs.ObsIdErrorParse) as excinfo:
        gs.ObsId.from_str("abc")
    assert isinstance(excinfo.value, gs.ObsIdError)
    assert excinfo.value.kind == "Parse"


def test_from_str_raises_for_a_number_without_10_digits() -> None:
    """The number is read, then checked."""
    with pytest.raises(gs.ObsIdErrorWrongNumDigits):
        gs.ObsId.from_str("123")


def test_the_obs_id_errors_are_not_value_errors() -> None:
    """The hierarchy is the library's: `ObsIdError` is an `Exception`, and a variant is an `ObsIdError`."""
    assert issubclass(gs.ObsIdError, Exception)
    assert not issubclass(gs.ObsIdError, ValueError)
    assert issubclass(gs.ObsIdErrorWrongNumDigits, gs.ObsIdError)
    assert issubclass(gs.ObsIdErrorParse, gs.ObsIdError)


def test_an_obs_id_acts_as_a_number() -> None:
    """`int`, `operator.index`, `str` and `repr` give the number."""
    obs_id = gs.ObsId(OBS_ID)
    assert int(obs_id) == OBS_ID
    assert operator.index(obs_id) == OBS_ID
    assert str(obs_id) == str(OBS_ID)
    assert repr(obs_id) == f"ObsId({OBS_ID})"


def test_an_obs_id_is_equal_to_the_int_of_the_same_value() -> None:
    """Equality works with an `int` and with another `ObsId`, in both orders."""
    obs_id = gs.ObsId(OBS_ID)
    assert obs_id == OBS_ID
    assert OBS_ID == obs_id
    assert obs_id == gs.ObsId(OBS_ID)
    assert obs_id != LATER_OBS_ID
    assert obs_id != gs.ObsId(LATER_OBS_ID)


def test_an_obs_id_is_not_equal_to_other_types() -> None:
    """A string or a float is not an Obs ID."""
    obs_id = gs.ObsId(OBS_ID)
    assert obs_id != str(OBS_ID)
    assert obs_id != float(OBS_ID)
    assert obs_id != None


def test_an_obs_id_and_an_int_are_the_same_dictionary_key() -> None:
    """The hash is the hash of the `int`."""
    obs_id = gs.ObsId(OBS_ID)
    assert hash(obs_id) == hash(OBS_ID)
    by_int: dict[Any, str] = {OBS_ID: "a"}
    by_obs_id: dict[Any, str] = {obs_id: "b"}
    assert by_int[obs_id] == "a"
    assert by_obs_id[OBS_ID] == "b"
    assert len({obs_id, OBS_ID, gs.ObsId(OBS_ID)}) == 1


def test_obs_ids_are_ordered() -> None:
    """An `ObsId` compares with an `ObsId` and an `int`, so `sorted` and `max` work."""
    early, late = gs.ObsId(OBS_ID), gs.ObsId(LATER_OBS_ID)
    assert early < late
    assert early <= late
    assert late > early
    assert late >= early
    assert early < LATER_OBS_ID
    assert early >= OBS_ID
    assert sorted([late, early]) == [early, late]
    assert max([early, late]) is late


def test_ordering_with_another_type_is_a_type_error() -> None:
    """Only numbers have an order."""
    with pytest.raises(TypeError, match="cannot compare an ObsId with str"):
        _ = gs.ObsId(OBS_ID) < "text"


def test_an_obs_id_is_copied_and_pickled() -> None:
    """`copy` and `pickle` make an equal `ObsId`."""
    obs_id = gs.ObsId(OBS_ID)
    assert copy.copy(obs_id) == obs_id
    assert copy.deepcopy(obs_id) == obs_id
    restored = pickle.loads(pickle.dumps(obs_id))
    assert isinstance(restored, gs.ObsId)
    assert restored == obs_id


def test_an_obs_id_cannot_be_changed() -> None:
    """The class is frozen, as the Rust type is."""
    with pytest.raises(AttributeError):
        gs.ObsId(OBS_ID).value = 1  # ty: ignore[unresolved-attribute]
