"""Tests that the run-time signature of each submit method has the MWA ASVO schema's defaults.

The signature of each submit method is a hand-written ``text_signature`` (the default values are made from the schema
when the method runs, but pyo3 cannot write them in the run-time signature). This compares the two, so that a change
of the schema, or a typing mistake, cannot leave ``help()`` showing a default that the method does not use.
"""

import inspect
import json
from typing import Any

import pytest

import mwa_giant_squid as gs

from .test_client import SCHEMA_PATH

# Each submit method: the schema type of its request body, the positional arguments, and the schema fields that
# the method does not take as an argument (it sets them itself, or leaves them to the MWA ASVO).
METHODS = [
    ("submit_download_vis_job", "DownloadJobParams", ["obs_id"], {"download_type"}),
    ("submit_download_meta_job", "DownloadJobParams", ["obs_id"], {"download_type"}),
    ("submit_conversion_job", "ConversionJobParams", ["obs_id"], set()),
    ("submit_imaging_job", "ImagingJobFlow1Params", ["obs_id"], set()),
    ("submit_image_from_job", "ImagingJobFlow2Params", ["obs_id", "source_job_id"], set()),
    ("submit_voltage_job", "VoltageJobParams", ["obs_id", "offset", "duration"], {"channel_range", "delivery_format"}),
    ("submit_beamformer_job", "BeamformerJobParams", ["obs_id"], {"mode"}),
]

# The schema enums that the methods take, by the name that a default has in a signature.
ENUMS = {
    "Delivery": gs.Delivery,
    "DeliveryFormat": gs.DeliveryFormat,
    "Output": gs.Output,
    "Centre": gs.Centre,
    "OutputMode": gs.OutputMode,
    "Polarization": gs.Polarization,
    "Weighting": gs.Weighting,
}


def signature_default(default: Any) -> Any:
    """The value that a default in a signature stands for.

    An enum member is written in a signature as its quoted name, for example ``'Delivery.Acacia'``, because
    ``inspect.signature`` accepts only literals. Its schema value is the ``str`` of the member. Any other string
    (for example the voltage ``delivery``, which is a plain string in the schema) is its own value.

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


@pytest.mark.parametrize(("method", "schema_name", "positional", "not_arguments"), METHODS)
def test_the_signature_has_the_schema_defaults(
    method: str, schema_name: str, positional: list[str], not_arguments: set[str]
) -> None:
    """Each argument of the method is a field of the schema, with the schema's default.

    Args:
        method: The submit method.
        schema_name: The schema type of its request body.
        positional: The positional arguments of the method.
        not_arguments: The fields of the schema type that are not arguments.
    """
    properties = json.loads(SCHEMA_PATH.read_text())["definitions"][schema_name]["properties"]
    parameters = list(inspect.signature(getattr(gs.AsvoClient, method)).parameters.values())[1:]  # without self

    assert [p.name for p in parameters if p.default is inspect.Parameter.empty] == positional
    keyword = {p.name: p.default for p in parameters if p.default is not inspect.Parameter.empty}
    assert set(keyword) == set(properties) - set(positional) - not_arguments

    for name, default in keyword.items():
        assert signature_default(default) == properties[name].get("default"), f"{method}: {name}"
