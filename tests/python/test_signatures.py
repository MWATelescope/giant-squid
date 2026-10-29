"""Tests that the run-time signature of each request body class has the MWA ASVO schema's defaults.

The signature of each class is a hand-written ``text_signature`` (the default values are made from the schema when the
class is made, but pyo3 cannot write them in the run-time signature). This compares the two, so that a change of the
schema, or a typing mistake, cannot leave ``help()`` showing a default that the class does not use.
"""

import inspect
import json
from typing import Any

import pytest

import mwa_giant_squid as gs

from .test_client import SCHEMA_PATH

# Each request body class: the schema type, the positional arguments, and the schema fields that the class does not
# take as an argument.
CLASSES = [
    (gs.DownloadJobParams, "DownloadJobParams", ["obs_id"], set()),
    (gs.ConversionJobParams, "ConversionJobParams", ["obs_id"], set()),
    (gs.ImagingJobFlow1Params, "ImagingJobFlow1Params", ["obs_id"], set()),
    (gs.ImagingJobFlow2Params, "ImagingJobFlow2Params", ["source_job_id"], set()),
]

# The schema enums that the classes take, by the name that a default has in a signature.
ENUMS = {
    "DeliveryDestination": gs.DeliveryDestination,
    "DeliveryFormat": gs.DeliveryFormat,
    "OutputFormat": gs.OutputFormat,
    "PhaseCentre": gs.PhaseCentre,
    "ImageOutputMode": gs.ImageOutputMode,
    "Polarization": gs.Polarization,
    "ImageWeighting": gs.ImageWeighting,
}


def signature_default(default: Any) -> Any:
    """The value that a default in a signature stands for.

    An enum member is written in a signature as its quoted name, for example ``'DeliveryDestination.Acacia'``, because
    ``inspect.signature`` accepts only literals. Its schema value is the ``str`` of the member. Any other value
    is its own value.

    Args:
        default: The default in the signature.

    Returns:
        The default, with an enum member's name replaced by the member's schema value.
    """
    if isinstance(default, str):
        enum_name, _, member = default.partition(".")
        if enum_name in ENUMS:
            return str(getattr(ENUMS[enum_name], member))
    return default


@pytest.mark.parametrize(("job", "schema_name", "positional", "not_arguments"), CLASSES)
def test_the_signature_has_the_schema_defaults(
    job: type, schema_name: str, positional: list[str], not_arguments: set[str]
) -> None:
    """Each argument of the class is a field of the schema, with the schema's default.

    Args:
        job: The request body class.
        schema_name: Its schema type.
        positional: The positional arguments of the class.
        not_arguments: The fields of the schema type that are not arguments.
    """
    properties = json.loads(SCHEMA_PATH.read_text())["definitions"][schema_name]["properties"]
    parameters = list(inspect.signature(job).parameters.values())

    assert [p.name for p in parameters if p.default is inspect.Parameter.empty] == positional
    keyword = {p.name: p.default for p in parameters if p.default is not inspect.Parameter.empty}
    assert set(keyword) == set(properties) - set(positional) - not_arguments

    for name, default in keyword.items():
        assert signature_default(default) == properties[name].get("default"), f"{schema_name}: {name}"
