# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

"""The command line definition: the same commands and options as the Rust ``giant-squid`` command.

Each option has the name, short name, alias and default that the Rust command has. The defaults come from the
MWA ASVO schema through the ``*_params`` functions of ``mwa_giant_squid``, so they cannot drift from it.
"""

import argparse
import os
from typing import Any

import mwa_giant_squid
from mwa_giant_squid import Centre, Delivery, DeliveryFormat, Output, OutputMode, Polarization, Weighting

from .constants import (
    DEFAULT_CONCURRENT_DOWNLOADS,
    ENV_DELIVERY,
    ENV_DELIVERY_FORMAT,
    PLACEHOLDER_OBS_ID,
    PLACEHOLDER_SOURCE_JOB_ID,
    PLACEHOLDER_VOLTAGE_DURATION,
    PLACEHOLDER_VOLTAGE_OFFSET,
)
from .parsing import (
    JOB_STATE_NAMES,
    JOB_TYPE_NAMES,
    default_text,
    enum_parser,
    enum_values,
    name_list_parser,
    parse_bool,
    parse_utc_time,
    positive_int,
)

ABOUT = """An alternative, efficient and easy-to-use MWA ASVO client.
Source:   https://github.com/MWATelescope/giant-squid
MWA ASVO: https://asvo.mwatelescope.org"""

# The names of the options that are not arguments of an `AsvoClient.submit_*` method.
NON_PARAM_DESTS = frozenset({"command", "wait", "dry_run", "json", "verbosity", "obs_ids", "source_job_id"})

# The text of the `--version` output. The Rust command prints the crate name and version.
VERSION_TEXT = f"mwa_giant_squid {mwa_giant_squid.__version__}"

# The commands that submit jobs, with their short names, in the order of the Rust help.
SUBMIT_COMMANDS = (
    "submit-vis",
    "submit-conv",
    "submit-image",
    "submit-image-from-job",
    "submit-meta",
    "submit-volt",
    "submit-bf",
)

# The flags that take an optional value, only joined with an equals sign (`--join-channels=false`), as in the
# Rust command. A bare flag means true. Without this, argparse would take the next argument as its value.
EQUALS_BOOL_FLAGS = frozenset({"--apply-di-cal", "--apply-primary-beam", "--join-channels"})

# Where a default comes from when the command line gives none: the `*_params` function for the command.
DefaultDict = dict[str, Any]


def normalise_bool_flags(argv: list[str]) -> list[str]:
    """Write a bare optional-value boolean flag as ``--flag=true``.

    Args:
        argv: The command line arguments.

    Returns:
        The arguments, with each bare flag in ``EQUALS_BOOL_FLAGS`` followed by ``=true``.
    """
    return [f"{arg}=true" if arg in EQUALS_BOOL_FLAGS else arg for arg in argv]


def _defaults() -> dict[str, DefaultDict]:
    """Get the MWA ASVO default of each option, from the module.

    Returns:
        The request body of each job type, built with no options, by command name.
    """
    obs_id = PLACEHOLDER_OBS_ID
    return {
        "submit-vis": mwa_giant_squid.download_vis_job_params(obs_id),
        "submit-conv": mwa_giant_squid.conversion_job_params(obs_id),
        "submit-image": mwa_giant_squid.imaging_job_params(obs_id),
        "submit-image-from-job": mwa_giant_squid.image_from_job_params(obs_id, PLACEHOLDER_SOURCE_JOB_ID),
        "submit-meta": mwa_giant_squid.download_meta_job_params(obs_id),
        "submit-volt": mwa_giant_squid.voltage_job_params(
            obs_id, PLACEHOLDER_VOLTAGE_OFFSET, PLACEHOLDER_VOLTAGE_DURATION
        ),
        "submit-bf": mwa_giant_squid.beamformer_job_params(obs_id),
    }


def _add_verbosity(parser: argparse.ArgumentParser) -> None:
    parser.add_argument(
        "-v",
        "--verbosity",
        action="count",
        default=0,
        help="The verbosity of the program. The default is to print high-level information.",
    )


def _add_bool(parser: argparse.ArgumentParser, flag: str, default: bool, help_text: str) -> None:
    """Add a boolean option with an optional value: ``--flag``, ``--flag=true`` or ``--flag=false``."""
    parser.add_argument(
        flag,
        nargs="?",
        const=True,
        default=default,
        type=parse_bool,
        metavar="BOOL",
        help=f"{help_text} (default: {default_text(default)})",
    )


def _add_float(
    parser: argparse.ArgumentParser, flag: str, default: float | None, help_text: str, aliases: tuple[str, ...] = ()
) -> None:
    suffix = "" if default is None else f" (default: {default:g})"
    parser.add_argument(flag, *aliases, type=float, default=default, help=f"{help_text}{suffix}")


def _add_int(
    parser: argparse.ArgumentParser, flag: str, default: int | None, help_text: str, aliases: tuple[str, ...] = ()
) -> None:
    suffix = "" if default is None else f" (default: {default})"
    parser.add_argument(flag, *aliases, type=int, default=default, help=f"{help_text}{suffix}")


def _add_enum(
    parser: argparse.ArgumentParser,
    flag: str,
    enum_type: type,
    default: str,
    help_text: str,
    short: str | None = None,
    aliases: tuple[str, ...] = (),
    env: str | None = None,
) -> None:
    """Add an option whose value is a member of an enum. ``default`` is the text of the default member."""
    names = [short, flag] if short else [flag]
    names.extend(aliases)
    parser.add_argument(
        *names,
        type=enum_parser(enum_type, flag.removeprefix("--").replace("-", " ")),
        default=os.environ.get(env, default) if env else default,
        help=f"{help_text} (default: {default}; options: {', '.join(enum_values(enum_type))})",
    )


def _add_delivery(parser: argparse.ArgumentParser, defaults: DefaultDict) -> None:
    _add_enum(
        parser,
        "--delivery",
        Delivery,
        defaults["delivery"],
        f"Tell MWA ASVO where to deliver the data. Environment variable: {ENV_DELIVERY}.",
        short="-d",
        env=ENV_DELIVERY,
    )
    _add_enum(
        parser,
        "--delivery-format",
        DeliveryFormat,
        defaults["delivery_format"],
        f"Tell MWA ASVO to deliver the data in a particular format. Environment variable: {ENV_DELIVERY_FORMAT}.",
        short="-f",
        env=ENV_DELIVERY_FORMAT,
    )


def _add_allow_resubmit(parser: argparse.ArgumentParser) -> None:
    parser.add_argument(
        "-r",
        "--allow-resubmit",
        action="store_true",
        help="Allow resubmitting a job even if an identical one has completed.",
    )


def _add_submit_common(parser: argparse.ArgumentParser, obs_help: str) -> None:
    """Add the options and the obsids that every submit command has."""
    parser.add_argument(
        "-w",
        "--wait",
        action="store_true",
        help="Do not exit giant-squid until the specified obsids are ready for download.",
    )
    parser.add_argument(
        "-n",
        "--dry-run",
        action="store_true",
        help="Don't actually submit; print information on what would've happened instead.",
    )
    parser.add_argument(
        "-j",
        "--json",
        action="store_true",
        help="Print each submitted job's response from the MWA ASVO as one line of JSON on stdout.",
    )
    _add_verbosity(parser)
    parser.add_argument("obs_ids", nargs="*", metavar="OBS_ID", help=obs_help)


def _add_conversion_options(parser: argparse.ArgumentParser, d: DefaultDict) -> None:
    _add_delivery(parser, d)
    _add_enum(parser, "--output", Output, d["output"], 'Output format: "ms" (measurement set) or "uvfits".', short="-o")
    _add_float(parser, "--avg-freq-res", d["avg_freq_res"], "Frequency resolution to average to (kHz).")
    _add_float(parser, "--avg-time-res", d["avg_time_res"], "Time resolution to average to (s).")
    _add_float(parser, "--flag-edge-width", d["flag_edge_width"], "Width of frequency edge flagging (kHz).")
    _add_bool(parser, "--apply-di-cal", d["apply_di_cal"], "Whether to apply the DI calibration solution.")
    _add_enum(
        parser,
        "--centre",
        Centre,
        d["centre"],
        'Phase centre mode: "phase", "pointing", or "custom". If "custom", also supply --custom-centre-ra '
        "and --custom-centre-dec.",
    )
    _add_float(
        parser,
        "--custom-centre-ra",
        None,
        "Custom phase centre right ascension (degrees). Requires --centre custom.",
        aliases=("--phase-centre-ra",),
    )
    _add_float(
        parser,
        "--custom-centre-dec",
        None,
        "Custom phase centre declination (degrees). Requires --centre custom.",
        aliases=("--phase-centre-dec",),
    )
    for flag, text in (
        ("--no-apply-amps", "Whether to skip applying amplitude calibration solutions."),
        ("--no-digital-gains", "Whether to skip applying digital gains."),
        ("--no-flag-dc", "Whether to skip flagging the DC channel."),
        ("--no-geometry-delay", "Whether to skip applying geometric delay corrections."),
        ("--no-passband-gains", "Whether to skip applying passband gain corrections."),
        ("--no-cable-delay", "Whether to skip applying cable delay corrections."),
        ("--no-rfi", "Whether to skip RFI flagging."),
    ):
        parser.add_argument(flag, action="store_true", help=text)
    _add_allow_resubmit(parser)


def _add_wsclean_options(parser: argparse.ArgumentParser, d: DefaultDict) -> None:
    """Add the options that the two imaging commands share."""
    _add_bool(parser, "--apply-primary-beam", d["apply_primary_beam"], "Whether to apply the primary beam correction.")
    _add_int(parser, "--auto-mask", d["auto_mask"], "WSClean -auto-mask value.")
    _add_float(parser, "--auto-threshold", d["auto_threshold"], "WSClean -auto-threshold value.")
    _add_float(
        parser,
        "--abs-threshold",
        d["abs_threshold"],
        "Absolute cleaning threshold (Jy). Overridden by auto_threshold unless explicitly set.",
    )
    _add_int(parser, "--channels-out", d["channels_out"], "Number of output channel groups.")
    _add_int(parser, "--clean-iterations", d["clean_iterations"], "WSClean -niter value (max clean iterations).")
    _add_float(
        parser,
        "--clean-threshold",
        d["clean_threshold"],
        "WSClean cleaning threshold (Jy). Takes precedence over auto_threshold if set.",
    )
    _add_int(parser, "--image-size", d["image_size"], "WSClean image size in pixels.")
    _add_bool(parser, "--join-channels", d["join_channels"], "Join output channel groups for cleaning.")
    parser.add_argument("--join-polarizations", action="store_true", help="Join polarisations for cleaning.")
    _add_float(parser, "--mgain", d["mgain"], "WSClean -mgain value.")
    parser.add_argument("--multiscale", action="store_true", help="Enable WSClean multiscale cleaning.")
    _add_int(parser, "--nmiter", d["nmiter"], "WSClean -nmiter value (max major cleaning iterations).")
    _add_int(parser, "--nwlayers", None, "Number of w-projection layers. Leave unset to let the server decide.")
    _add_enum(
        parser, "--output-mode", OutputMode, d["output_mode"], "The output mode / product to request.", short="-o"
    )
    _add_float(parser, "--pixel-scale", d["pixel_scale"], "Pixel scale (arcsec/pixel).")
    _add_enum(parser, "--pol", Polarization, d["pol"], "Polarisation to image: XX, YY or XXYY.")
    _add_float(parser, "--robust", d["robust"], "WSClean -robust (Briggs robustness) value.")
    _add_float(
        parser,
        "--uvw-max",
        None,
        "Maximum uv distance to image, in wavelengths (upper bound on the range that can be requested).",
    )
    _add_float(parser, "--uvw-min", d["uvw_min"], "Minimum uv distance to image, in wavelengths.")
    _add_enum(parser, "--weighting", Weighting, d["weighting"], "WSClean weighting scheme.")
    _add_int(parser, "--wstack-nwlayers", None, "Number of w-stacking layers. Leave unset to let the server decide.")
    _add_allow_resubmit(parser)


def _add_imaging_options(parser: argparse.ArgumentParser, d: DefaultDict) -> None:
    _add_delivery(parser, d)
    _add_bool(parser, "--apply-di-cal", d["apply_di_cal"], "Whether to apply the DI calibration solution.")
    _add_float(parser, "--avg-freq-res", d["avg_freq_res"], "Frequency resolution to average to before imaging (kHz).")
    _add_float(parser, "--avg-time-res", d["avg_time_res"], "Time resolution to average to before imaging (s).")
    _add_float(
        parser,
        "--custom-centre-dec",
        None,
        "Custom phase centre declination (degrees). Requires --centre custom.",
        aliases=("--custom-dec",),
    )
    _add_float(
        parser,
        "--custom-centre-ra",
        None,
        "Custom phase centre right ascension (degrees). Requires --centre custom.",
        aliases=("--custom-ra",),
    )
    _add_float(parser, "--flag-edge-width", d["flag_edge_width"], "Width of frequency edge flagging (kHz).")
    _add_enum(
        parser,
        "--centre",
        Centre,
        d["centre"],
        'Where to centre the image: "phase", "pointing", or "custom". If "custom", also supply '
        "--custom-centre-ra and --custom-centre-dec.",
        aliases=("--phase-center",),
    )
    parser.add_argument(
        "--no-apply-amps",
        action="store_true",
        help="Whether to skip applying amplitude calibration solutions. Leave at the default (false) unless you "
        "know you need this.",
    )
    _add_wsclean_options(parser, d)


def _add_voltage_options(parser: argparse.ArgumentParser, d: DefaultDict) -> None:
    parser.add_argument(
        "-d",
        "--delivery",
        default=os.environ.get(ENV_DELIVERY, d["delivery"]),
        help='Tell MWA ASVO where to deliver the data. The only valid value for a voltage job is "scratch", '
        f'which requires the "mwavcs" Pawsey Group on your MWA ASVO profile. (default: {d["delivery"]}) '
        f"Environment variable: {ENV_DELIVERY}.",
    )
    parser.add_argument(
        "-o",
        "--offset",
        type=int,
        required=True,
        help="The offset in seconds from the start GPS time of the observation.",
    )
    parser.add_argument("-u", "--duration", type=int, required=True, help="The duration (in seconds) to download.")
    parser.add_argument("-f", "--from-channel", type=int, help="The 'from' receiver channel number (0-255).")
    parser.add_argument("-t", "--to-channel", type=int, help="The 'to' receiver channel number (0-255).")
    _add_allow_resubmit(parser)


def build_parser() -> argparse.ArgumentParser:
    """Build the argument parser, with a sub-command for each ``giant-squid`` command.

    Returns:
        The parser. A parsed command line has ``command`` (the full command name) and the options.
    """
    d = _defaults()
    parser = argparse.ArgumentParser(
        prog="giant-squid",
        description=ABOUT,
        formatter_class=argparse.RawDescriptionHelpFormatter,
        allow_abbrev=False,
    )
    parser.add_argument("-V", "--version", action="version", version=VERSION_TEXT)
    commands = parser.add_subparsers(dest="command", required=True, metavar="COMMAND")

    def add(name: str, alias: str, help_text: str) -> argparse.ArgumentParser:
        sub = commands.add_parser(name, aliases=[alias], help=help_text, description=help_text, allow_abbrev=False)
        sub.set_defaults(command=name)
        return sub

    list_parser = add("list", "l", "List your current and recent MWA ASVO jobs")
    list_parser.add_argument("-j", "--json", action="store_true", help="Print the jobs as a simple JSON")
    _add_verbosity(list_parser)
    list_parser.add_argument(
        "--job-states",
        "--states",
        dest="job_states",
        type=name_list_parser(JOB_STATE_NAMES, "job state"),
        metavar="JOB_STATE",
        help="show only jobs matching the provided states (comma-separated), case insensitive. Options: queued, "
        "waitcal, staging, staged, downloading, preparing, preprocessing, imaging, delivering, ready, error, "
        "expired, cancelled",
    )
    list_parser.add_argument(
        "--job-types",
        "--types",
        dest="job_types",
        type=name_list_parser(JOB_TYPE_NAMES, "job type"),
        metavar="JOB_TYPE",
        help="filter job list by type (comma-separated), case insensitive with underscores. Options: conversion, "
        "download_visibilities, download_metadata, download_voltages, download_beamformer, imaging or cancel_job",
    )
    list_parser.add_argument(
        "-n",
        "--no-colour",
        action="store_true",
        help="Disables colouring of output. Useful when you have a non-black terminal background for example",
    )
    list_parser.add_argument(
        "--days", type=int, help="Only fetch jobs from the past N days. If not given, fetches your full job history."
    )
    list_parser.add_argument(
        "--date-from",
        type=parse_utc_time,
        help="Only jobs created at or after this time: RFC 3339 (for example 2026-09-01T00:00:00Z) or a date "
        "(2026-09-01, midnight UTC).",
    )
    list_parser.add_argument(
        "--date-to",
        type=parse_utc_time,
        help="Only jobs created at or before this time: RFC 3339 or a date (midnight UTC).",
    )
    list_parser.add_argument("--sort-by", help='The column to sort the jobs by, for example "id".')
    list_parser.add_argument(
        "job_ids_or_obs_ids",
        nargs="*",
        metavar="JOB_ID_OR_OBS_ID",
        help="job IDs or obsids to filter by. Files containing job IDs or obsids are also accepted.",
    )

    download = add("download", "d", "Download an MWA ASVO job")
    download.add_argument(
        "-d", "--download-dir", default=".", help="Which dir should downloads be written to (default: %(default)s)."
    )
    download.add_argument(
        "-k",
        "--keep-tar",
        "--keep-zip",
        action="store_true",
        help="Acacia delivery jobs only: Don't untar the contents of your download. NOTE: This option allows "
        "resuming downloads by rerunning giant-squid after an interruption. Giant-squid will resume where it "
        "left off.",
    )
    download.add_argument(
        "-r",
        "--no-resume",
        action="store_true",
        help="Do not attempt to resume a partial download. Leave the partial file alone.",
    )
    download.add_argument(
        "-c",
        "--concurrent-downloads",
        type=int,
        default=DEFAULT_CONCURRENT_DOWNLOADS,
        help="Download up to this number of jobs concurrently. 2-4 is a good number for most users. Set this to "
        "0 to use the number of CPU cores you machine has (default: %(default)s)",
    )
    download.add_argument(
        "--skip-hash", action="store_true", help="Don't verify the downloaded contents against the upstream hash."
    )
    download.add_argument("--hash", action="store_true", help=argparse.SUPPRESS)
    download.add_argument(
        "-n",
        "--dry-run",
        action="store_true",
        help="Don't actually download; print information on what would've happened instead.",
    )
    _add_verbosity(download)
    download.add_argument(
        "job_ids_or_obs_ids",
        nargs="*",
        metavar="JOB_ID_OR_OBS_ID",
        help="The job IDs or obsids to be downloaded. Files containing job IDs or obsids are also accepted.",
    )

    vis = add("submit-vis", "sv", "Submit MWA ASVO jobs to download MWA raw visibilities")
    _add_delivery(vis, d["submit-vis"])
    _add_allow_resubmit(vis)
    _add_submit_common(vis, "The obsids to be submitted. Files containing obsids are also accepted.")

    conv = add("submit-conv", "sc", "Submit MWA ASVO preprocessing/conversion jobs")
    _add_conversion_options(conv, d["submit-conv"])
    _add_submit_common(conv, "The obsids to be submitted. Files containing obsids are also accepted.")

    image = add("submit-image", "si", "Submit MWA ASVO imaging jobs")
    _add_imaging_options(image, d["submit-image"])
    _add_submit_common(
        image,
        "The obsids to submit for imaging. Files containing obsids are also accepted. All obsids in one "
        "invocation share the same parameters above.",
    )

    from_job = add(
        "submit-image-from-job",
        "sifj",
        "Submit MWA ASVO imaging jobs from an existing conversion job. Unlike submit-image, this skips the "
        "conversion step and images directly from the output of a previous conversion job.",
    )
    from_job.add_argument(
        "--source-job-id",
        type=positive_int,
        required=True,
        help="The MWA ASVO conversion job ID to image from. Required.",
    )
    _add_delivery(from_job, d["submit-image-from-job"])
    _add_wsclean_options(from_job, d["submit-image-from-job"])
    _add_submit_common(
        from_job,
        "The obsid to image. Exactly one obsid is required (the source_job_id identifies the conversion job for "
        "this obsid).",
    )

    meta = add(
        "submit-meta",
        "sm",
        "Submit MWA ASVO jobs to download MWA metadata - metafits (with PPDs for each tile) and RFI flags (if "
        "available)",
    )
    _add_delivery(meta, d["submit-meta"])
    _add_allow_resubmit(meta)
    _add_submit_common(meta, "The obsids to be submitted. Files containing obsids are also accepted.")

    volt = add("submit-volt", "st", "Submit MWA ASVO jobs to download MWA voltages")
    _add_voltage_options(volt, d["submit-volt"])
    _add_submit_common(volt, "The obsids to be submitted. Files containing obsids are also accepted.")

    bf = add("submit-bf", "sb", "Submit MWA ASVO jobs to download MWA beamformer files (vdif,hdr,fil)")
    _add_delivery(bf, d["submit-bf"])
    _add_allow_resubmit(bf)
    _add_submit_common(bf, "The obsids to be submitted. Files containing obsids are also accepted.")

    wait = add("wait", "w", "Wait for MWA ASVO jobs to complete, return the urls")
    wait.add_argument("-j", "--json", action="store_true", help="Print the jobs as a simple JSON after waiting")
    _add_verbosity(wait)
    wait.add_argument(
        "-n",
        "--no-colour",
        action="store_true",
        help="Disables colouring of output. Useful when you have a non-black terminal background for example",
    )
    wait.add_argument(
        "jobs", nargs="*", metavar="JOB_ID", help="The jobs to wait for. Files containing jobs are also accepted."
    )

    cancel = add("cancel", "c", "Cancel MWA ASVO job")
    cancel.add_argument(
        "-n",
        "--dry-run",
        action="store_true",
        help="Don't actually cancel; print information on what would've happened instead.",
    )
    _add_verbosity(cancel)
    cancel.add_argument(
        "jobs", nargs="*", metavar="JOB_ID", help="The jobs to be cancelled. Files containing obsids are also accepted."
    )
    return parser
