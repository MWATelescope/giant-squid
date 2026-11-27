// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Tests for the CLI definition and the argument-to-request-body mapping.
//!
//! Everything here is offline: no MWA ASVO server is contacted, no job is
//! submitted and no environment variable is read or written. See
//! `docs/history/TESTING.md` for the wider plan.

use std::io::Write;

use clap::Parser;
use serde_json::Value;
use tempfile::NamedTempFile;

use super::params::{
    ConversionJobArgs, DownloadJobArgs, ImagingFromJobArgs, ImagingJobArgs, Nullable,
    SubmitDownloadArgs, VoltageJobArgs,
};
use super::Args;
use crate::mwa_asvo::api::job_params::{
    conversion_defaults, download_defaults, image_from_job_defaults, imaging_defaults,
};
use crate::mwa_asvo::api::openapi::{ImageOutputMode, MWA_ASVO_API_VERSION, OPENAPI_VERSION};
use crate::parse_many_job_ids_or_obs_ids;

/// An obsid used throughout these tests, and its `i64` form as the request
/// bodies carry it.
const TEST_OBS_ID: &str = "1065880128";
const TEST_OBS_ID_I64: i64 = 1065880128;

/// A job ID, which is distinguishable from an obsid by not having 10 digits.
const TEST_JOB_ID: &str = "12345";

fn parse(args: &[&str]) -> Args {
    Args::try_parse_from(args).expect("expected these arguments to parse")
}

fn parse_err(args: &[&str]) -> clap::Error {
    Args::try_parse_from(args).expect_err("expected these arguments to be rejected")
}

fn vis_args(args: &[&str]) -> (DownloadJobArgs, Vec<String>) {
    match parse(args) {
        Args::SubmitVis {
            download, obs_ids, ..
        } => (download, obs_ids),
        other => panic!("expected SubmitVis, got {other:?}"),
    }
}

fn meta_args(args: &[&str]) -> (DownloadJobArgs, Vec<String>) {
    match parse(args) {
        Args::SubmitMeta {
            download, obs_ids, ..
        } => (download, obs_ids),
        other => panic!("expected SubmitMeta, got {other:?}"),
    }
}

fn conv_args(args: &[&str]) -> (ConversionJobArgs, Vec<String>) {
    match parse(args) {
        Args::SubmitConv { conv, obs_ids, .. } => (conv, obs_ids),
        other => panic!("expected SubmitConv, got {other:?}"),
    }
}

fn image_args(args: &[&str]) -> (ImagingJobArgs, Vec<String>) {
    match parse(args) {
        Args::SubmitImage { image, obs_ids, .. } => (image, obs_ids),
        other => panic!("expected SubmitImage, got {other:?}"),
    }
}

fn image_from_job_args(args: &[&str]) -> ImagingFromJobArgs {
    match parse(args) {
        Args::SubmitImageFromJob { image, .. } => image,
        other => panic!("expected SubmitImageFromJob, got {other:?}"),
    }
}

fn volt_args(args: &[&str]) -> (VoltageJobArgs, Vec<String>) {
    match parse(args) {
        Args::SubmitVolt { volt, obs_ids, .. } => (volt, obs_ids),
        other => panic!("expected SubmitVolt, got {other:?}"),
    }
}

fn bf_args(args: &[&str]) -> (DownloadJobArgs, Vec<String>) {
    match parse(args) {
        Args::SubmitBf { bf, obs_ids, .. } => (bf, obs_ids),
        other => panic!("expected SubmitBf, got {other:?}"),
    }
}

fn json_of<T: serde::Serialize>(params: &T) -> Value {
    serde_json::to_value(params).expect("request body should serialise")
}

// ---------------------------------------------------------------------------
// Commands and aliases
// ---------------------------------------------------------------------------

#[test]
fn every_command_alias_resolves_to_its_command() {
    assert!(matches!(parse(&["giant-squid", "l"]), Args::List { .. }));
    assert!(matches!(
        parse(&["giant-squid", "d", TEST_JOB_ID]),
        Args::Download { .. }
    ));
    assert!(matches!(
        parse(&["giant-squid", "sv", TEST_OBS_ID]),
        Args::SubmitVis { .. }
    ));
    assert!(matches!(
        parse(&["giant-squid", "sc", TEST_OBS_ID]),
        Args::SubmitConv { .. }
    ));
    assert!(matches!(
        parse(&["giant-squid", "si", "--output-mode", "fits", TEST_OBS_ID]),
        Args::SubmitImage { .. }
    ));
    assert!(matches!(
        parse(&[
            "giant-squid",
            "sifj",
            "--output-mode",
            "fits",
            "--source-job-id",
            TEST_JOB_ID
        ]),
        Args::SubmitImageFromJob { .. }
    ));
    assert!(matches!(
        parse(&["giant-squid", "sm", TEST_OBS_ID]),
        Args::SubmitMeta { .. }
    ));
    assert!(matches!(
        parse(&[
            "giant-squid",
            "st",
            "--offset",
            "0",
            "--duration",
            "8",
            TEST_OBS_ID
        ]),
        Args::SubmitVolt { .. }
    ));
    assert!(matches!(
        parse(&["giant-squid", "sb", TEST_OBS_ID]),
        Args::SubmitBf { .. }
    ));
    assert!(matches!(
        parse(&["giant-squid", "w", TEST_JOB_ID]),
        Args::Wait { .. }
    ));
    assert!(matches!(
        parse(&["giant-squid", "c", TEST_JOB_ID]),
        Args::Cancel { .. }
    ));
}

// ---------------------------------------------------------------------------
// Obsid arguments: one, many, and from a file
// ---------------------------------------------------------------------------

#[test]
fn many_obs_ids_are_collected() {
    let (_, obs_ids) = vis_args(&[
        "giant-squid",
        "submit-vis",
        "1061311664",
        "1061311784",
        "1061312032",
    ]);
    assert_eq!(obs_ids.len(), 3);

    let (job_ids, parsed) = parse_many_job_ids_or_obs_ids(&obs_ids).expect("obsids should parse");
    assert!(job_ids.is_empty());
    assert_eq!(
        parsed.iter().map(|o| o.get()).collect::<Vec<_>>(),
        vec![1061311664, 1061311784, 1061312032]
    );
}

#[test]
fn obs_ids_can_come_from_a_file() {
    let mut file = NamedTempFile::new().expect("could not create tmp file");
    writeln!(file, "1061311664 1061311784\n1061312032").expect("could not write tmp file");
    file.flush().expect("could not flush tmp file");
    let path = file.path().display().to_string();

    let (_, obs_ids) = vis_args(&["giant-squid", "submit-vis", &path]);
    let (job_ids, parsed) = parse_many_job_ids_or_obs_ids(&obs_ids).expect("file should parse");
    assert!(job_ids.is_empty());
    assert_eq!(parsed.len(), 3);
}

#[test]
fn a_job_id_is_distinguished_from_an_obs_id() {
    // The submit commands reject job IDs; the check itself lives in the
    // binary, but it relies on this classification.
    let (job_ids, obs_ids) =
        parse_many_job_ids_or_obs_ids(&[TEST_JOB_ID.to_string()]).expect("job ID should parse");
    assert_eq!(job_ids.len(), 1);
    assert!(obs_ids.is_empty());
}

// ---------------------------------------------------------------------------
// submit-vis / submit-meta
// ---------------------------------------------------------------------------

#[test]
fn submit_vis_defaults_come_from_the_schema() {
    let (args, _) = vis_args(&["giant-squid", "submit-vis", TEST_OBS_ID]);
    assert_eq!(args.delivery_args.delivery, download_defaults().delivery);
    assert_eq!(
        args.delivery_args.delivery_format,
        download_defaults().delivery_format
    );
    assert_eq!(args.resubmit.allow_resubmit, Nullable(Some(false)));
}

#[test]
fn submit_vis_builds_a_vis_download_body() {
    let (args, _) = vis_args(&["giant-squid", "submit-vis", TEST_OBS_ID]);
    let params = args
        .to_vis_download()
        .to_params(crate::test_config::test_obs_id());
    let json = json_of(&params);

    assert_eq!(json["obs_id"], TEST_OBS_ID_I64);
    // The schema gives `only_metadata` no default, so it is left out.
    assert!(json.get("only_metadata").is_none());
    assert_eq!(json["allow_resubmit"], false);
}

#[test]
fn submit_meta_builds_a_meta_download_body() {
    let (args, _) = meta_args(&["giant-squid", "submit-meta", TEST_OBS_ID]);
    let params = args
        .to_meta_download()
        .to_params(crate::test_config::test_obs_id());
    assert_eq!(json_of(&params)["only_metadata"], true);
}

#[test]
fn delivery_and_format_can_be_overridden() {
    let (args, _) = vis_args(&[
        "giant-squid",
        "submit-vis",
        "--delivery",
        "scratch",
        "--delivery-format",
        "tar",
        TEST_OBS_ID,
    ]);
    let params = args
        .to_vis_download()
        .to_params(crate::test_config::test_obs_id());
    assert_eq!(json_of(&params)["delivery"], "scratch");
}

#[test]
fn allow_resubmit_reaches_the_request_body() {
    let (args, _) = vis_args(&["giant-squid", "submit-vis", "-r", TEST_OBS_ID]);
    assert_eq!(args.resubmit.allow_resubmit, Nullable(Some(true)));
    let params = args
        .to_vis_download()
        .to_params(crate::test_config::test_obs_id());
    assert_eq!(json_of(&params)["allow_resubmit"], true);
}

// ---------------------------------------------------------------------------
// submit-conv
// ---------------------------------------------------------------------------

#[test]
fn submit_conv_defaults_come_from_the_schema() {
    let (args, _) = conv_args(&["giant-squid", "submit-conv", TEST_OBS_ID]);
    assert_eq!(args.output, conversion_defaults().output);
    assert_eq!(args.preprocessing.centre, conversion_defaults().centre);
    assert_eq!(args.delivery_args.delivery, conversion_defaults().delivery);
    assert_eq!(
        args.delivery_args.delivery_format,
        conversion_defaults().delivery_format
    );

    // The numeric defaults are checked through the request body, so that
    // floats are compared as JSON values rather than directly.
    let json = json_of(&args.to_params(crate::test_config::test_obs_id()));
    assert_eq!(
        json["avg_freq_res"],
        conversion_defaults().avg_freq_res.expect("schema default")
    );
    assert_eq!(
        json["avg_time_res"],
        conversion_defaults().avg_time_res.expect("schema default")
    );
    assert_eq!(
        json["flag_edge_width"],
        conversion_defaults()
            .flag_edge_width
            .expect("schema default")
    );
}

#[test]
fn submit_conv_custom_phase_centre_is_passed_through() {
    let (args, _) = conv_args(&[
        "giant-squid",
        "submit-conv",
        "--centre",
        "custom",
        "--phase-centre-ra",
        "10.5",
        "--phase-centre-dec",
        "-26.7",
        TEST_OBS_ID,
    ]);
    let params = args.to_params(crate::test_config::test_obs_id());
    let json = json_of(&params);

    assert_eq!(json["custom_centre_ra"], 10.5);
    assert_eq!(json["custom_centre_dec"], -26.7);
}

#[test]
fn submit_conv_output_can_be_overridden() {
    let (args, _) = conv_args(&[
        "giant-squid",
        "submit-conv",
        "--output",
        "ms",
        "--avg-freq-res",
        "40",
        TEST_OBS_ID,
    ]);
    let json = json_of(&args.to_params(crate::test_config::test_obs_id()));
    assert_eq!(json["output"], "ms");
    assert_eq!(json["avg_freq_res"], 40.0);
}

// ---------------------------------------------------------------------------
// submit-image
// ---------------------------------------------------------------------------

#[test]
fn submit_image_defaults_come_from_the_schema() {
    let (args, _) = image_args(&[
        "giant-squid",
        "submit-image",
        "--output-mode",
        "fits",
        TEST_OBS_ID,
    ]);
    assert_eq!(args.wsclean.image_size, imaging_defaults().image_size);
    assert_eq!(args.wsclean.pixel_scale, imaging_defaults().pixel_scale);
    assert_eq!(args.wsclean.weighting, imaging_defaults().weighting);
    assert_eq!(args.wsclean.auto_mask, imaging_defaults().auto_mask);
    assert_eq!(args.wsclean.nmiter, imaging_defaults().nmiter);
    assert_eq!(args.wsclean.channels_out, imaging_defaults().channels_out);
}

#[test]
fn submit_image_builds_an_imaging_body() {
    let (args, _) = image_args(&[
        "giant-squid",
        "submit-image",
        "--output-mode",
        "fits",
        TEST_OBS_ID,
    ]);
    let json = json_of(&args.to_params(crate::test_config::test_obs_id()));

    assert_eq!(json["obs_id"], TEST_OBS_ID_I64);
    assert_eq!(json["output_mode"], "fits");
    assert_eq!(
        json["polarization"],
        imaging_defaults().polarization.to_string()
    );
    // The schema gives `image_size` and `pixel_scale` no default: the server
    // calculates them, so the body leaves them out.
    let object = json.as_object().expect("an object");
    assert!(!object.contains_key("image_size"));
    assert!(!object.contains_key("pixel_scale"));
}

/// `--output-mode` is required: the schema gives `output_mode` no default.
#[test]
fn submit_image_requires_an_output_mode() {
    for argv in [
        &["giant-squid", "submit-image", TEST_OBS_ID][..],
        &[
            "giant-squid",
            "submit-image-from-job",
            "--source-job-id",
            "5",
        ][..],
    ] {
        let err = parse_err(argv);
        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::MissingRequiredArgument,
            "{argv:?}"
        );
        assert!(err.to_string().contains("--output-mode"), "{err}");
    }
}

#[test]
fn submit_image_accepts_each_supported_polarization() {
    for polarization in ["xx", "yy", "xy", "yx", "xxyy", "i", "iquv"] {
        let (args, _) = image_args(&[
            "giant-squid",
            "submit-image",
            "--output-mode",
            "fits",
            "--polarization",
            polarization,
            TEST_OBS_ID,
        ]);
        let json = json_of(&args.to_params(crate::test_config::test_obs_id()));
        assert_eq!(json["polarization"], polarization);
    }
}

#[test]
fn submit_image_rejects_an_unsupported_polarization() {
    // `XX` is `xx`: the case does not matter. `--pol` is not an alias.
    for bad in ["xx,yy", "X", "Q"] {
        let err = parse_err(&[
            "giant-squid",
            "submit-image",
            "--output-mode",
            "fits",
            "--polarization",
            bad,
            TEST_OBS_ID,
        ]);
        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::ValueValidation,
            "expected --polarization {bad} to be rejected"
        );
    }
    let err = parse_err(&[
        "giant-squid",
        "submit-image",
        "--output-mode",
        "fits",
        "--pol",
        "xx",
        TEST_OBS_ID,
    ]);
    assert_eq!(err.kind(), clap::error::ErrorKind::UnknownArgument);
}

#[test]
fn submit_image_custom_centre_is_renamed_for_the_api() {
    let (args, _) = image_args(&[
        "giant-squid",
        "submit-image",
        "--output-mode",
        "fits",
        "--phase-center",
        "custom",
        "--custom-ra",
        "10.5",
        "--custom-dec",
        "-26.7",
        TEST_OBS_ID,
    ]);
    let json = json_of(&args.to_params(crate::test_config::test_obs_id()));

    assert_eq!(json["custom_centre_ra"], 10.5);
    assert_eq!(json["custom_centre_dec"], -26.7);
    assert_eq!(json["centre"], "custom");
}

/// A negative value has to work both as `--flag value` and `--flag=value`.
/// clap reads a leading '-' as a flag unless the command opts in, so
/// `--custom-dec -26.7` used to fail with "unknown argument '-2'" - a
/// defect for any southern declination, a negative robustness, or a
/// negative uv_min.
#[test]
fn negative_values_are_accepted_in_either_form() {
    for argv in [
        vec![
            "giant-squid",
            "submit-image",
            "--output-mode",
            "fits",
            "--custom-dec",
            "-26.7",
            "--robust",
            "-1.5",
            TEST_OBS_ID,
        ],
        vec![
            "giant-squid",
            "submit-image",
            "--output-mode",
            "fits",
            "--custom-dec=-26.7",
            "--robust=-1.5",
            TEST_OBS_ID,
        ],
    ] {
        let (args, _) = image_args(&argv);
        let json = json_of(&args.to_params(crate::test_config::test_obs_id()));
        assert_eq!(json["custom_centre_dec"], -26.7, "argv: {argv:?}");
        assert_eq!(json["robust"], -1.5, "argv: {argv:?}");
    }

    for argv in [
        vec![
            "giant-squid",
            "submit-conv",
            "--centre",
            "custom",
            "--phase-centre-dec",
            "-26.7",
            TEST_OBS_ID,
        ],
        vec![
            "giant-squid",
            "submit-conv",
            "--centre",
            "custom",
            "--phase-centre-dec=-26.7",
            TEST_OBS_ID,
        ],
    ] {
        let (args, _) = conv_args(&argv);
        let json = json_of(&args.to_params(crate::test_config::test_obs_id()));
        assert_eq!(json["custom_centre_dec"], -26.7, "argv: {argv:?}");
    }
}

/// Negative numbers being allowed must not swallow the short flags on
/// those same commands.
#[test]
fn allowing_negative_numbers_does_not_break_short_flags() {
    match parse(&[
        "giant-squid",
        "submit-image",
        "--output-mode",
        "fits",
        "-n",
        "-vv",
        TEST_OBS_ID,
    ]) {
        Args::SubmitImage { submit, .. } => {
            assert!(submit.dry_run);
            assert_eq!(submit.verbosity, 2);
            assert!(!submit.wait);
        }
        other => panic!("expected SubmitImage, got {other:?}"),
    }
}

#[test]
fn submit_image_optional_fields_are_omitted_when_unset() {
    let (args, _) = image_args(&[
        "giant-squid",
        "submit-image",
        "--output-mode",
        "fits",
        TEST_OBS_ID,
    ]);
    let json = json_of(&args.to_params(crate::test_config::test_obs_id()));

    assert!(json.get("nwlayers").is_none());
    assert!(json.get("uv_max").is_none());
    assert!(json.get("wstack_nwlayers").is_none());
}

#[test]
fn submit_image_boolean_flags_require_equals() {
    let (args, _) = image_args(&[
        "giant-squid",
        "submit-image",
        "--output-mode",
        "fits",
        "--apply-di-cal=false",
        "--join-channels=false",
        TEST_OBS_ID,
    ]);
    assert!(!args.preprocessing.apply_di_cal);
    assert!(!args.wsclean.join_channels);

    // Given without a value, the flag takes its default_missing_value.
    let (args, _) = image_args(&[
        "giant-squid",
        "submit-image",
        "--output-mode",
        "fits",
        "--apply-di-cal",
        TEST_OBS_ID,
    ]);
    assert!(args.preprocessing.apply_di_cal);
}

#[test]
fn submit_image_accepts_every_supported_image_size() {
    for size in ["512", "1024", "2048", "3072", "4096", "8192"] {
        let (args, _) = image_args(&[
            "giant-squid",
            "submit-image",
            "--output-mode",
            "fits",
            "--image-size",
            size,
            TEST_OBS_ID,
        ]);
        assert_eq!(
            args.wsclean.image_size.map(|s| s.to_string()).as_deref(),
            Some(size)
        );
    }
}

#[test]
fn submit_image_rejects_out_of_range_values() {
    for bad in [
        vec!["--auto-mask", "1"],
        vec!["--auto-mask", "513"],
        vec!["--auto-threshold", "0.05"],
        vec!["--mgain", "1.5"],
        vec!["--nmiter", "0"],
        vec!["--nmiter", "501"],
        vec!["--robust", "-2.5"],
        vec!["--pixel-scale", "9"],
        vec!["--custom-dec", "-91"],
    ] {
        let mut argv = vec!["giant-squid", "submit-image", "--output-mode", "fits"];
        argv.extend_from_slice(&bad);
        argv.push(TEST_OBS_ID);
        let err = parse_err(&argv);
        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::ValueValidation,
            "expected {bad:?} to be rejected"
        );
    }
}

/// The options of the fields that the schema marks as deprecated are not
/// there: use `--abs-threshold` and `--wstack-nwlayers`.
#[test]
fn the_deprecated_imaging_options_are_refused() {
    for option in ["--clean-threshold", "--nwlayers"] {
        let err = parse_err(&[
            "giant-squid",
            "submit-image",
            "--output-mode",
            "fits",
            option,
            "1",
            TEST_OBS_ID,
        ]);
        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::UnknownArgument,
            "{option}"
        );
        let err = parse_err(&[
            "giant-squid",
            "submit-image-from-job",
            "--output-mode",
            "fits",
            "--source-job-id",
            TEST_JOB_ID,
            option,
            "1",
        ]);
        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::UnknownArgument,
            "{option}"
        );
    }
}

/// The CLI's range errors are the library's, so they read the same in the
/// CLI, the Rust client and the Python module.
#[test]
fn submit_image_range_errors_carry_the_librarys_message() {
    for (flag, value, expected) in [
        ("--mgain", "1.5", "must be between 0.1 and 1 (got 1.5)"),
        ("--uv-min", "101", "must be between 1 and 100 (got 101)"),
        ("--avg-time-res", "-1", "must be at least 0 (got -1)"),
        (
            "--image-size",
            "1000",
            "must be one of 512, 1024, 2048, 3072, 4096, 8192",
        ),
    ] {
        let err = parse_err(&[
            "giant-squid",
            "submit-image",
            "--output-mode",
            "fits",
            flag,
            value,
            TEST_OBS_ID,
        ]);
        assert!(
            err.to_string().contains(expected),
            "{flag} {value}: expected {expected:?} in {err}"
        );
    }
}

// ---------------------------------------------------------------------------
// submit-image-from-job
// ---------------------------------------------------------------------------

#[test]
fn submit_image_from_job_requires_a_source_job_id() {
    // `--output-mode` is given, so the only missing argument is
    // `--source-job-id`.
    let err = parse_err(&[
        "giant-squid",
        "submit-image-from-job",
        "--output-mode",
        "fits",
    ]);
    assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    assert!(err.to_string().contains("--source-job-id"), "{err}");
}

#[test]
fn submit_image_from_job_builds_an_image_from_job_body() {
    let args = image_from_job_args(&[
        "giant-squid",
        "submit-image-from-job",
        "--output-mode",
        "fits",
        "--source-job-id",
        "4242",
    ]);
    assert_eq!(args.source_job_id.get(), 4242);
    assert_eq!(
        args.wsclean.polarization,
        image_from_job_defaults().polarization
    );

    let json = json_of(&args.to_params());
    assert_eq!(json["source_job_id"], 4242);
    assert_eq!(json["output_mode"], "fits");
    // The body has no Obs ID: the MWA ASVO takes it from the source job.
    assert!(json.get("obs_id").is_none());
}

#[test]
fn submit_image_from_job_rejects_a_zero_source_job_id() {
    let err = parse_err(&[
        "giant-squid",
        "submit-image-from-job",
        "--output-mode",
        "fits",
        "--source-job-id",
        "0",
    ]);
    assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
}

// ---------------------------------------------------------------------------
// submit-download, and the deprecated download commands
// ---------------------------------------------------------------------------

fn download_args(args: &[&str]) -> SubmitDownloadArgs {
    match parse(args) {
        Args::SubmitDownload { download, .. } => download,
        other => panic!("expected SubmitDownload, got {other:?}"),
    }
}

/// The request body of a download command line: `submit-download` or one
/// of the deprecated download commands.
fn download_body(args: &[&str]) -> Value {
    let obs_id = crate::test_config::test_obs_id();
    let download = match parse(args) {
        Args::SubmitDownload { download, .. } => download,
        Args::SubmitVis { download, .. } => download.to_vis_download(),
        Args::SubmitMeta { download, .. } => download.to_meta_download(),
        Args::SubmitVolt { volt, .. } => volt.to_download(),
        Args::SubmitBf { bf, .. } => bf.to_vis_download(),
        other => panic!("expected a download command, got {other:?}"),
    };
    json_of(&download.to_params(obs_id))
}

#[test]
fn submit_download_sends_the_schema_defaults() {
    use crate::mwa_asvo::api::openapi::DownloadJobParams;

    let expected: DownloadJobParams = DownloadJobParams::builder()
        .obs_id(TEST_OBS_ID_I64)
        .try_into()
        .expect("defaults build");
    assert_eq!(
        download_body(&["giant-squid", "submit-download", TEST_OBS_ID]),
        json_of(&expected)
    );
    assert!(matches!(
        parse(&["giant-squid", "sd", TEST_OBS_ID]),
        Args::SubmitDownload { .. }
    ));
}

#[test]
fn submit_download_sends_the_voltage_options_that_are_given() {
    let json = download_body(&[
        "giant-squid",
        "submit-download",
        "-o",
        "16",
        "-u",
        "8",
        "--from-channel",
        "109",
        "--to-channel",
        "132",
        TEST_OBS_ID,
    ]);
    assert_eq!(json["offset"], 16);
    assert_eq!(json["duration"], 8);
    assert_eq!(json["from_channel"], 109);
    assert_eq!(json["to_channel"], 132);

    let json = download_body(&[
        "giant-squid",
        "submit-download",
        "--offset",
        "0",
        TEST_OBS_ID,
    ]);
    assert_eq!(json["offset"], 0);
    for field in ["duration", "from_channel", "to_channel"] {
        assert!(json.get(field).is_none(), "{field} was sent");
    }
}

#[test]
fn submit_download_refuses_an_offset_or_a_channel_out_of_range() {
    for (option, value) in [
        ("--offset", "-1"),
        ("--offset", "7201"),
        ("--from-channel", "256"),
        ("--duration", "-1"),
    ] {
        let err = parse_err(&["giant-squid", "submit-download", option, value, TEST_OBS_ID]);
        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::ValueValidation,
            "{option} {value}"
        );
    }
}

#[test]
fn submit_download_only_metadata_takes_true_or_false() {
    let only_metadata = |args: &[&str]| download_args(args).only_metadata;

    // The schema gives `only_metadata` no default, so it is null when it is
    // not given.
    assert_eq!(
        only_metadata(&["giant-squid", "submit-download", TEST_OBS_ID]),
        None
    );
    assert_eq!(
        only_metadata(&[
            "giant-squid",
            "submit-download",
            "--only-metadata",
            TEST_OBS_ID
        ]),
        Some(true)
    );
    assert_eq!(
        only_metadata(&[
            "giant-squid",
            "submit-download",
            "--only-metadata=false",
            TEST_OBS_ID
        ]),
        Some(false)
    );
    let json = download_body(&["giant-squid", "submit-download", TEST_OBS_ID]);
    assert!(json.get("only_metadata").is_none());
    let err = parse_err(&[
        "giant-squid",
        "submit-download",
        "--only-metadata=null",
        TEST_OBS_ID,
    ]);
    assert_eq!(err.kind(), clap::error::ErrorKind::InvalidValue);
}

/// Each deprecated download command sends the body of the `submit-download`
/// command line that replaces it.
#[test]
fn each_deprecated_download_command_sends_the_body_of_submit_download() {
    let cases: [(&[&str], &[&str]); 5] = [
        (
            &["submit-vis", "-d", "scratch", "-f", "files", "-r"],
            &["submit-download", "-d", "scratch", "-f", "files", "-r"],
        ),
        (
            &["submit-meta", "-d", "dug"],
            &["submit-download", "-d", "dug", "--only-metadata"],
        ),
        (
            &[
                "submit-volt",
                "-o",
                "16",
                "-u",
                "8",
                "--to-channel",
                "3",
                "-r",
            ],
            &[
                "submit-download",
                "-o",
                "16",
                "-u",
                "8",
                "--to-channel",
                "3",
                "-r",
            ],
        ),
        (&["submit-bf", "-r"], &["submit-download", "-r"]),
        (&["submit-bf"], &["submit-vis"]),
    ];
    for (deprecated, replacement) in cases {
        let line = |args: &[&str]| {
            let mut line = vec!["giant-squid"];
            line.extend_from_slice(args);
            line.push(TEST_OBS_ID);
            download_body(&line)
        };
        assert_eq!(line(deprecated), line(replacement), "{deprecated:?}");
    }
}

/// The conversion request body of a `submit-preprocess` or `submit-conv`
/// command line.
fn preprocess_body(args: &[&str]) -> Value {
    let conv = match parse(args) {
        Args::SubmitPreprocess { conv, .. } | Args::SubmitConv { conv, .. } => conv,
        other => panic!("expected a preprocess command, got {other:?}"),
    };
    json_of(&conv.to_params(crate::test_config::test_obs_id()))
}

/// `submit-preprocess` sends the conversion defaults, and the deprecated
/// `submit-conv` sends the body of the same `submit-preprocess` command
/// line.
#[test]
fn submit_conv_sends_the_body_of_submit_preprocess() {
    use crate::mwa_asvo::api::openapi::ConversionJobParams;

    let expected: ConversionJobParams = ConversionJobParams::builder()
        .obs_id(TEST_OBS_ID_I64)
        .try_into()
        .expect("defaults build");
    assert_eq!(
        preprocess_body(&["giant-squid", "submit-preprocess", TEST_OBS_ID]),
        json_of(&expected)
    );
    assert!(matches!(
        parse(&["giant-squid", "sp", TEST_OBS_ID]),
        Args::SubmitPreprocess { .. }
    ));

    let options = [
        "-d",
        "scratch",
        "-f",
        "files",
        "-r",
        "-o",
        "uvfits",
        "--avg-freq-res",
        "80",
        "--centre",
        "custom",
        "--custom-centre-ra",
        "10.5",
        "--custom-centre-dec",
        "-26.7",
        "--no-rfi",
        TEST_OBS_ID,
    ];
    let line = |command: &'static str| {
        let mut line = vec!["giant-squid", command];
        line.extend_from_slice(&options);
        preprocess_body(&line)
    };
    let body = line("submit-preprocess");
    assert_eq!(body, line("submit-conv"));
    assert_eq!(body["output"], "uvfits");
    assert_eq!(body["custom_centre_dec"], -26.7);
}

/// The deprecated commands are hidden from the list of commands, and the
/// end of the main help lists each with what to use instead. Every hidden
/// command is in the list.
#[test]
fn the_deprecated_commands_are_hidden_and_listed_at_the_end_of_the_help() {
    use clap::CommandFactory;

    use super::{deprecated_command_warning, DEPRECATED_COMMANDS};

    let mut command = Args::command();
    let hidden: Vec<String> = command
        .get_subcommands()
        .filter(|sub| sub.is_hide_set())
        .map(|sub| sub.get_name().to_string())
        .collect();
    let listed: Vec<String> = DEPRECATED_COMMANDS
        .iter()
        .map(|deprecated| deprecated.name.to_string())
        .collect();
    assert_eq!(hidden, listed);

    let help = command.render_help().to_string();
    let commands = help
        .split("Options:")
        .next()
        .expect("the help has a list of commands");
    for deprecated in &DEPRECATED_COMMANDS {
        assert!(
            !commands.contains(deprecated.name),
            "{} is in the list of commands",
            deprecated.name
        );
        assert!(
            help.contains(&format!("Use {}", deprecated.replacement)),
            "{help}"
        );
        let warning = deprecated_command_warning(deprecated);
        assert!(warning.starts_with(deprecated.name), "{warning}");
        assert!(warning.contains(deprecated.replacement), "{warning}");
    }
}

// ---------------------------------------------------------------------------
// submit-volt
// ---------------------------------------------------------------------------

#[test]
fn submit_volt_requires_offset_and_duration() {
    let err = parse_err(&["giant-squid", "submit-volt", TEST_OBS_ID]);
    assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
}

/// A voltage job has no delivery options: its body has the schema's
/// delivery defaults, and the MWA ASVO decides where it is delivered.
#[test]
fn submit_volt_has_no_delivery_options() {
    for delivery in [
        ["--delivery", "scratch"],
        ["-d", "scratch"],
        ["--delivery-format", "files"],
    ] {
        let err = parse_err(&[
            "giant-squid",
            "submit-volt",
            "--offset",
            "0",
            "--duration",
            "8",
            delivery[0],
            delivery[1],
            TEST_OBS_ID,
        ]);
        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::UnknownArgument,
            "{delivery:?}"
        );
    }

    let (args, _) = volt_args(&[
        "giant-squid",
        "submit-volt",
        "--offset",
        "0",
        "--duration",
        "8",
        TEST_OBS_ID,
    ]);
    let params = args
        .to_download()
        .to_params(crate::test_config::test_obs_id());
    assert_eq!(params.delivery, download_defaults().delivery);
    assert_eq!(params.delivery_format, download_defaults().delivery_format);
}

#[test]
fn submit_volt_sends_the_channel_bounds_and_no_channel_range() {
    let (args, _) = volt_args(&[
        "giant-squid",
        "submit-volt",
        "--offset",
        "16",
        "--duration",
        "8",
        TEST_OBS_ID,
    ]);
    let json = json_of(
        &args
            .to_download()
            .to_params(crate::test_config::test_obs_id()),
    );
    assert!(json.get("channel_range").is_none());
    assert_eq!(json["offset"], 16);
    assert_eq!(json["duration"], 8);

    let (args, _) = volt_args(&[
        "giant-squid",
        "submit-volt",
        "--offset",
        "16",
        "--duration",
        "8",
        "--from-channel",
        "109",
        TEST_OBS_ID,
    ]);
    let json = json_of(
        &args
            .to_download()
            .to_params(crate::test_config::test_obs_id()),
    );
    assert!(json.get("channel_range").is_none());
    assert_eq!(json["from_channel"], 109);
    assert!(json.get("to_channel").is_none());
}

#[test]
fn submit_volt_rejects_a_channel_above_the_receiver_range() {
    let err = parse_err(&[
        "giant-squid",
        "submit-volt",
        "--offset",
        "0",
        "--duration",
        "8",
        "--to-channel",
        "256",
        TEST_OBS_ID,
    ]);
    assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
}

// ---------------------------------------------------------------------------
// submit-bf
// ---------------------------------------------------------------------------

#[test]
fn submit_bf_builds_a_beamformer_body() {
    let (args, _) = bf_args(&["giant-squid", "submit-bf", TEST_OBS_ID]);
    assert_eq!(args.delivery_args.delivery, download_defaults().delivery);
    assert_eq!(
        args.delivery_args.delivery_format,
        download_defaults().delivery_format
    );

    let json = json_of(
        &args
            .to_vis_download()
            .to_params(crate::test_config::test_obs_id()),
    );
    assert_eq!(json["obs_id"], TEST_OBS_ID_I64);
    // A beamformer body is a download body: the MWA ASVO finds the kind of
    // data from the observation. It has no voltage fields, and no
    // `only_metadata` (the schema gives it no default).
    for field in [
        "only_metadata",
        "offset",
        "duration",
        "from_channel",
        "to_channel",
    ] {
        assert!(json.get(field).is_none(), "{field} was sent");
    }
}

// ---------------------------------------------------------------------------
// list, download, wait, cancel
// ---------------------------------------------------------------------------

#[test]
fn list_filters_parse() {
    match parse(&[
        "giant-squid",
        "list",
        "--states",
        "queued,completed",
        "--types",
        "conversion,visibility",
        "--days",
        "7",
        "--json",
        TEST_OBS_ID,
    ]) {
        Args::List {
            job_states: states,
            job_types: types,
            days,
            json,
            job_ids_or_obs_ids,
            ..
        } => {
            assert_eq!(states.len(), 2);
            assert_eq!(types.len(), 2);
            assert_eq!(days, Some(crate::test_config::nonzero(7)));
            assert!(json);
            assert_eq!(job_ids_or_obs_ids, vec![TEST_OBS_ID.to_string()]);
        }
        other => panic!("expected List, got {other:?}"),
    }
}

/// Without `--days`, the CLI uses the schema's default for the days of a job
/// listing, and the help shows it. It does not ask for the full history.
#[test]
fn list_days_defaults_to_the_schema_default() {
    use clap::CommandFactory;

    match parse(&["giant-squid", "list"]) {
        Args::List { days, .. } => {
            assert_eq!(days, Some(super::params::list_days_default()));
        }
        other => panic!("expected List, got {other:?}"),
    }

    let cli = Args::command();
    let days = cli
        .find_subcommand("list")
        .expect("list exists")
        .get_arguments()
        .find(|arg| arg.get_long() == Some("days"))
        .expect("list has --days");
    let defaults: Vec<String> = days
        .get_default_values()
        .iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect();
    assert_eq!(defaults, [super::params::list_days_default().to_string()]);
}

/// `--days` takes the schema's 1 to 30, and says so when it is outside it.
#[test]
fn list_days_is_limited_to_the_schema_range() {
    for days in ["1", "30"] {
        match parse(&["giant-squid", "list", "--days", days]) {
            Args::List { days: parsed, .. } => assert_eq!(parsed, Some(days.parse().unwrap())),
            other => panic!("expected List, got {other:?}"),
        }
    }
    for days in ["0", "31", "many"] {
        let err = parse_err(&["giant-squid", "list", "--days", days]);
        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::ValueValidation,
            "{days}"
        );
    }
    // `list` does not take negative numbers as values, so clap reads this one as a flag.
    parse_err(&["giant-squid", "list", "--days", "-3"]);
    let err = parse_err(&["giant-squid", "list", "--days", "31"]);
    assert!(err.to_string().contains("between 1 and 30"), "{err}");
}

#[test]
fn list_rejects_an_unknown_state() {
    let err = parse_err(&["giant-squid", "list", "--states", "nonsense"]);
    assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
}

#[test]
fn download_defaults_and_aliases_parse() {
    match parse(&["giant-squid", "download", TEST_JOB_ID]) {
        Args::Download {
            download_dir,
            concurrent_downloads,
            keep_tar,
            no_resume,
            skip_hash,
            dry_run,
            ..
        } => {
            assert_eq!(download_dir, ".");
            assert_eq!(concurrent_downloads, 4);
            assert!(!keep_tar);
            assert!(!no_resume);
            assert!(!skip_hash);
            assert!(!dry_run);
        }
        other => panic!("expected Download, got {other:?}"),
    }

    // --keep-zip is the backwards-compatible alias for --keep-tar.
    match parse(&[
        "giant-squid",
        "download",
        "--keep-zip",
        "--download-dir",
        "/tmp",
        "--concurrent-downloads",
        "2",
        "--skip-hash",
        "--no-resume",
        "-n",
        TEST_JOB_ID,
    ]) {
        Args::Download {
            download_dir,
            concurrent_downloads,
            keep_tar,
            no_resume,
            skip_hash,
            dry_run,
            ..
        } => {
            assert_eq!(download_dir, "/tmp");
            assert_eq!(concurrent_downloads, 2);
            assert!(keep_tar);
            assert!(no_resume);
            assert!(skip_hash);
            assert!(dry_run);
        }
        other => panic!("expected Download, got {other:?}"),
    }
}

#[test]
fn download_accepts_obs_ids_as_well_as_job_ids() {
    match parse(&["giant-squid", "download", TEST_OBS_ID, TEST_JOB_ID]) {
        Args::Download {
            job_ids_or_obs_ids, ..
        } => {
            let (job_ids, obs_ids) =
                parse_many_job_ids_or_obs_ids(&job_ids_or_obs_ids).expect("arguments should parse");
            assert_eq!(job_ids.len(), 1);
            assert_eq!(obs_ids.len(), 1);
        }
        other => panic!("expected Download, got {other:?}"),
    }
}

#[test]
fn wait_and_cancel_take_job_ids() {
    match parse(&["giant-squid", "wait", "--json", TEST_JOB_ID]) {
        Args::Wait { jobs, json, .. } => {
            assert_eq!(jobs, vec![TEST_JOB_ID.to_string()]);
            assert!(json);
        }
        other => panic!("expected Wait, got {other:?}"),
    }

    match parse(&["giant-squid", "cancel", "-n", TEST_JOB_ID]) {
        Args::Cancel { jobs, dry_run, .. } => {
            assert_eq!(jobs, vec![TEST_JOB_ID.to_string()]);
            assert!(dry_run);
        }
        other => panic!("expected Cancel, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// The CLI definition itself
// ---------------------------------------------------------------------------

#[test]
fn cli_definition_is_internally_consistent() {
    // Catches duplicate short/long flags and other clap definition errors,
    // including any introduced by the flattened argument groups.
    use clap::CommandFactory;
    Args::command().debug_assert();
}

#[test]
fn submit_image_from_job_rejects_out_of_range_values() {
    for bad in [
        vec!["--auto-mask", "1"],
        vec!["--auto-mask", "513"],
        vec!["--auto-threshold", "0.05"],
        vec!["--mgain", "1.5"],
        vec!["--nmiter", "0"],
        vec!["--nmiter", "501"],
        vec!["--robust", "-2.5"],
        vec!["--pixel-scale", "9"],
        vec!["--uv-max", "0.5"],
        vec!["--image-size", "1000"],
    ] {
        let mut argv = vec![
            "giant-squid",
            "submit-image-from-job",
            "--output-mode",
            "fits",
            "--source-job-id",
            TEST_JOB_ID,
        ];
        argv.extend_from_slice(&bad);
        let err = parse_err(&argv);
        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::ValueValidation,
            "expected {bad:?} to be rejected"
        );
    }
}

#[test]
fn submit_conv_rejects_out_of_range_values() {
    for bad in [
        vec!["--avg-freq-res", "1281"],
        vec!["--avg-freq-res", "-1"],
        vec!["--avg-time-res", "-1"],
        vec!["--flag-edge-width", "641"],
        vec!["--phase-centre-ra", "360"],
        vec!["--phase-centre-dec", "91"],
    ] {
        let mut argv = vec!["giant-squid", "submit-conv"];
        argv.extend_from_slice(&bad);
        argv.push(TEST_OBS_ID);
        let err = parse_err(&argv);
        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::ValueValidation,
            "expected {bad:?} to be rejected"
        );
    }
}

#[test]
fn submit_volt_rejects_an_offset_outside_the_observation() {
    // The "=" form. The separate form is tested below.
    for offset in ["--offset=-1", "--offset=7201"] {
        let err = parse_err(&[
            "giant-squid",
            "submit-volt",
            offset,
            "--duration",
            "8",
            TEST_OBS_ID,
        ]);
        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::ValueValidation,
            "expected {offset} to be rejected"
        );
    }
}

#[test]
fn submit_image_rejects_an_out_of_range_wstack_nwlayers() {
    let err = parse_err(&[
        "giant-squid",
        "submit-image",
        "--output-mode",
        "fits",
        "--wstack-nwlayers",
        "16",
        TEST_OBS_ID,
    ]);
    assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
}

#[test]
fn submit_image_from_job_rejects_an_out_of_range_wstack_nwlayers() {
    // Since schema v1.11 the image-from-job body limits it too.
    let err = parse_err(&[
        "giant-squid",
        "submit-image-from-job",
        "--output-mode",
        "fits",
        "--source-job-id",
        TEST_JOB_ID,
        "--wstack-nwlayers",
        "16",
    ]);
    assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
}

#[test]
fn submit_volt_reads_a_negative_offset_as_a_value() {
    // "-1" is a value of --offset, not an unknown flag, so the range check
    // gives its own message.
    let err = parse_err(&[
        "giant-squid",
        "submit-volt",
        "--offset",
        "-1",
        "--duration",
        "8",
        TEST_OBS_ID,
    ]);
    assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
    assert!(
        err.to_string()
            .contains("must be between 0 and 7200 (got -1)"),
        "{err}"
    );
}

// ---------------------------------------------------------------------------
// Flag names follow the OpenAPI schema; the old names are hidden aliases
// ---------------------------------------------------------------------------

#[test]
fn submit_conv_takes_the_schema_names_for_a_custom_centre() {
    let (args, _) = conv_args(&[
        "giant-squid",
        "submit-conv",
        "--centre",
        "custom",
        "--custom-centre-ra",
        "10.5",
        "--custom-centre-dec",
        "-26.7",
        TEST_OBS_ID,
    ]);
    let json = json_of(&args.to_params(crate::test_config::test_obs_id()));

    assert_eq!(json["centre"], "custom");
    assert_eq!(json["custom_centre_ra"], 10.5);
    assert_eq!(json["custom_centre_dec"], -26.7);
}

#[test]
fn submit_image_takes_the_schema_names_for_the_centre() {
    let (args, _) = image_args(&[
        "giant-squid",
        "submit-image",
        "--output-mode",
        "fits",
        "--centre",
        "custom",
        "--custom-centre-ra",
        "10.5",
        "--custom-centre-dec",
        "-26.7",
        TEST_OBS_ID,
    ]);
    let json = json_of(&args.to_params(crate::test_config::test_obs_id()));

    assert_eq!(json["centre"], "custom");
    assert_eq!(json["custom_centre_ra"], 10.5);
    assert_eq!(json["custom_centre_dec"], -26.7);
}

/// A job type that is not one of the names is an error, not a filter that
/// quietly matches nothing. The error names the text that was given.
#[test]
fn list_refuses_a_job_type_that_does_not_exist() {
    for bad in ["convertion", "unknown", "download"] {
        let err = parse_err(&["giant-squid", "list", "--job-types", bad]);

        assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation, "{bad}");
        let text = err.to_string();
        assert!(text.contains("--job-types"), "error: {text}");
        assert!(text.contains(bad), "error: {text}");
    }
}

/// The old names still work (the tests above this section use them), but
/// `--help` shows only the schema names.
#[test]
fn the_old_flag_names_are_not_shown_in_help() {
    use clap::CommandFactory;

    for (command, old_flags) in [
        (
            "submit-conv",
            &["--phase-centre-ra", "--phase-centre-dec"][..],
        ),
        (
            "submit-image",
            &["--custom-ra", "--custom-dec", "--phase-center"][..],
        ),
        ("list", &["--states", "--types"][..]),
    ] {
        let mut cli = Args::command();
        let help = cli
            .find_subcommand_mut(command)
            .unwrap_or_else(|| panic!("no {command} command"))
            .render_long_help()
            .to_string();
        for old in old_flags {
            assert!(
                !help.contains(&format!("{old} ")),
                "{command} --help shows {old}"
            );
        }
    }
}

#[test]
fn the_argument_placeholders_use_the_schema_names() {
    use clap::CommandFactory;

    for (command, placeholder) in [
        ("list", "[JOB_ID_OR_OBS_ID]..."),
        ("download", "[JOB_ID_OR_OBS_ID]..."),
        ("submit-vis", "[OBS_ID]..."),
        ("wait", "[JOB_ID]..."),
        ("cancel", "[JOB_ID]..."),
    ] {
        let mut cli = Args::command();
        let usage = cli
            .find_subcommand_mut(command)
            .unwrap_or_else(|| panic!("no {command} command"))
            .render_usage()
            .to_string();
        assert!(usage.contains(placeholder), "{command}: {usage}");
    }
}

// ---------------------------------------------------------------------------
// The job JSON: the OpenAPI names by default, the old format for
// --legacy-json
// ---------------------------------------------------------------------------

/// Three jobs that between them have every kind of value: files with a
/// URL and with a path, no files, an empty file list, a completion time,
/// and an error state with a message that needs escaping.
fn json_sample_jobs() -> crate::mwa_asvo::MwaAsvoJobVec {
    use crate::mwa_asvo::api::openapi::{DeliveryDestination, JobDetailResponse, JobProduct};
    use crate::mwa_asvo::{JobFile, JobState, MwaAsvoJobVec};
    let obs_id = crate::obs_id::ObsId::validate(1065880128).expect("a valid obsid");
    let completed: jiff::Timestamp = "2026-09-08T06:00:00Z".parse().expect("a valid time");
    let created: jiff::Timestamp = "2026-09-08T05:41:54Z".parse().expect("a valid time");
    let job_params =
        serde_json::from_value(serde_json::json!({"obs_id": 1065880128, "delivery": "acacia"}))
            .expect("job params are readable");
    MwaAsvoJobVec(vec![
        crate::test_config::mwa_asvo_job(
            obs_id,
            JobDetailResponse {
                job_id: crate::MwaAsvoJobId::new(101).expect("not zero"),
                obs_id: 0,
                job_type: crate::test_config::job_type("visibility"),
                job_state: JobState::Completed,
                product: Some(JobProduct {
                    files: vec![
                        JobFile {
                            type_: DeliveryDestination::Acacia,
                            url: Some("https://example.org/a.tar".to_string()),
                            path: None,
                            size: 1234,
                            sha1: Some("ab".repeat(20)),
                            format: None,
                        },
                        JobFile {
                            type_: DeliveryDestination::Scratch,
                            url: None,
                            path: Some("/scratch/mwa/x".to_string()),
                            size: 5,
                            sha1: None,
                            format: None,
                        },
                    ],
                }),
                created,
                started: Some(created),
                completed: Some(completed),
                modified: Some(completed),
                error_code: None,
                error_text: None,
                user_id: 4242,
                first_name: "Test".to_string(),
                last_name: "User".to_string(),
                job_params,
            },
        ),
        crate::test_config::mwa_asvo_job(
            obs_id,
            JobDetailResponse {
                job_id: crate::MwaAsvoJobId::new(102).expect("not zero"),
                obs_id: 0,
                job_type: crate::test_config::job_type("conversion"),
                job_state: JobState::Queued,
                product: None,
                created,
                started: None,
                completed: None,
                modified: None,
                error_code: None,
                error_text: None,
                user_id: 4242,
                first_name: "Test".to_string(),
                last_name: "User".to_string(),
                job_params: Default::default(),
            },
        ),
        crate::test_config::mwa_asvo_job(
            obs_id,
            JobDetailResponse {
                job_id: crate::MwaAsvoJobId::new(103).expect("not zero"),
                obs_id: 0,
                job_type: crate::test_config::job_type("imaging"),
                job_state: JobState::Error,
                product: Some(JobProduct { files: vec![] }),
                created,
                started: None,
                completed: None,
                modified: None,
                error_code: Some(12),
                error_text: Some("the \"conversion\" failed".to_string()),
                user_id: 4242,
                first_name: "Test".to_string(),
                last_name: "User".to_string(),
                job_params: Default::default(),
            },
        ),
    ])
}

/// `--legacy-json` prints exactly what `--json` of giant-squid 2.5.1 printed
/// for the same jobs: the 2.5.1 keys and their order, and no times.
#[test]
fn legacy_json_is_the_old_output_byte_for_byte() {
    let expected = r#"{"101":{"obsid":1065880128,"jobId":101,"jobType":"DownloadVisibilities","jobState":"Ready","files":[{"jobType":"Acacia","fileUrl":"https://example.org/a.tar","filePath":null,"fileSize":1234,"fileHash":"abababababababababababababababababababab"},{"jobType":"Scratch","fileUrl":null,"filePath":"/scratch/mwa/x","fileSize":5,"fileHash":null}]},"102":{"obsid":1065880128,"jobId":102,"jobType":"Conversion","jobState":"Queued","files":null},"103":{"obsid":1065880128,"jobId":103,"jobType":"Imaging","jobState":{"Error":"the \"conversion\" failed"},"files":[]}}"#;

    let output = super::legacy_json::to_legacy_json(&json_sample_jobs()).expect("serialises");

    assert_eq!(output, expected);
}

/// `--legacy-json` shows a completed job whose download links have expired
/// as `Expired`, as giant-squid 2.5.1 did. Before, it showed `Ready`.
#[test]
fn legacy_json_shows_an_expired_job_as_expired() {
    /// An `Expires` in the past (2001), as a signed URL has it.
    const EXPIRED_URL: &str = "https://example.org/a.tar?Expires=1000000000";

    let mut jobs = json_sample_jobs();
    let job = &mut jobs.0[0];
    let files = &mut job.detail_mut().product.as_mut().expect("a product").files;
    files[0].url = Some(EXPIRED_URL.to_string());
    assert!(job.is_expired(), "the sample job should now be expired");

    let output = super::legacy_json::to_legacy_json(&jobs).expect("serialises");

    assert!(
        output.starts_with(r#"{"101":{"obsid":1065880128,"jobId":101,"jobType":"DownloadVisibilities","jobState":"Expired","#),
        "{output}"
    );
}

/// `--legacy-json` names every job type as giant-squid 2.5.1 did, for the
/// integer code that the API sends: 0 to 6. The
/// expected text is what 2.5.1 (tag `v2.5.1`) printed for one job of each
/// type, made by its own `MwaAsvoJobVec::json`. A job in the sample above has
/// each of the other keys, so this one varies only the job type.
#[test]
fn legacy_json_names_every_job_type_as_giant_squid_2_5_1_did() {
    use crate::mwa_asvo::api::openapi::JobDetailResponse;
    use crate::mwa_asvo::api::schema_enums::SchemaEnum;
    use crate::mwa_asvo::{JobState, JobType, MwaAsvoJobVec};
    let obs_id = crate::obs_id::ObsId::validate(1065880128).expect("a valid obsid");
    let created: jiff::Timestamp = "2026-09-08T05:41:54Z".parse().expect("a valid time");
    let job_types: Vec<JobType> = JobType::VARIANTS.to_vec();
    let jobs = MwaAsvoJobVec(
        job_types
            .into_iter()
            .zip(201..)
            .map(|(job_type, id)| {
                crate::test_config::mwa_asvo_job(
                    obs_id,
                    JobDetailResponse {
                        job_id: crate::MwaAsvoJobId::new(id).expect("not zero"),
                        obs_id: 0,
                        job_type,
                        job_state: JobState::Completed,
                        product: None,
                        created,
                        started: None,
                        completed: None,
                        modified: None,
                        error_code: None,
                        error_text: None,
                        user_id: 4242,
                        first_name: "Test".to_string(),
                        last_name: "User".to_string(),
                        job_params: Default::default(),
                    },
                )
            })
            .collect(),
    );
    let expected = r#"{"201":{"obsid":1065880128,"jobId":201,"jobType":"Conversion","jobState":"Ready","files":null},"202":{"obsid":1065880128,"jobId":202,"jobType":"DownloadVisibilities","jobState":"Ready","files":null},"203":{"obsid":1065880128,"jobId":203,"jobType":"DownloadMetadata","jobState":"Ready","files":null},"204":{"obsid":1065880128,"jobId":204,"jobType":"DownloadVoltage","jobState":"Ready","files":null},"205":{"obsid":1065880128,"jobId":205,"jobType":"CancelJob","jobState":"Ready","files":null},"206":{"obsid":1065880128,"jobId":206,"jobType":"DownloadBeamformer","jobState":"Ready","files":null},"207":{"obsid":1065880128,"jobId":207,"jobType":"Imaging","jobState":"Ready","files":null}}"#;

    let output = super::legacy_json::to_legacy_json(&jobs).expect("serialises");

    assert_eq!(output, expected);
}

/// `--json` prints each job as the OpenAPI schema's `JobDetailResponse`, with
/// the schema's names and values (the job type is its name, as the API gives
/// it), and no key for a field that has no value.
#[test]
fn json_uses_the_openapi_names() {
    let expected = r#"{"101":{"completed":"2026-09-08T06:00:00Z","created":"2026-09-08T05:41:54Z","first_name":"Test","job_id":101,"job_params":{"allow_resubmit":false,"delivery":"acacia","delivery_format":"tar","obs_id":1065880128},"job_state":"completed","job_type":"visibility","last_name":"User","modified":"2026-09-08T06:00:00Z","obs_id":1065880128,"product":{"files":[{"sha1":"abababababababababababababababababababab","size":1234,"type":"acacia","url":"https://example.org/a.tar"},{"path":"/scratch/mwa/x","size":5,"type":"scratch"}]},"started":"2026-09-08T05:41:54Z","user_id":4242},"102":{"created":"2026-09-08T05:41:54Z","first_name":"Test","job_id":102,"job_params":{"allow_resubmit":false,"delivery":"acacia","delivery_format":"tar","obs_id":1065880128},"job_state":"queued","job_type":"conversion","last_name":"User","obs_id":1065880128,"user_id":4242},"103":{"created":"2026-09-08T05:41:54Z","error_code":12,"error_text":"the \"conversion\" failed","first_name":"Test","job_id":103,"job_params":{"allow_resubmit":false,"delivery":"acacia","delivery_format":"tar","obs_id":1065880128},"job_state":"error","job_type":"imaging","last_name":"User","obs_id":1065880128,"product":{"files":[]},"user_id":4242}}"#;

    let output = json_sample_jobs().json().expect("serialises");

    assert_eq!(output, expected);
}

#[test]
fn legacy_json_and_json_cannot_be_given_together() {
    for command in ["list", "wait"] {
        let err = parse_err(&[
            "giant-squid",
            command,
            "--json",
            "--legacy-json",
            TEST_JOB_ID,
        ]);
        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::ArgumentConflict,
            "{command}"
        );
    }
}

#[test]
fn submit_conv_sends_no_cable_delay_and_no_rfi() {
    let (args, _) = conv_args(&[
        "giant-squid",
        "submit-conv",
        "--no-cable-delay",
        "--no-rfi",
        TEST_OBS_ID,
    ]);
    let json = json_of(&args.to_params(crate::test_config::test_obs_id()));
    assert_eq!(json["no_cable_delay"], true);
    assert_eq!(json["no_rfi"], true);

    let (args, _) = conv_args(&["giant-squid", "submit-conv", TEST_OBS_ID]);
    let json = json_of(&args.to_params(crate::test_config::test_obs_id()));
    assert_eq!(json["no_cable_delay"], false);
    assert_eq!(json["no_rfi"], false);
}

/// The six correction and flagging switches of the conversion job are on the
/// imaging job too (schema v1.13). Each is off unless it is given, and the
/// body carries `false` then, the schema's default.
#[test]
fn submit_image_sends_the_six_correction_and_flagging_switches() {
    const SWITCHES: [&str; 6] = [
        "no_digital_gains",
        "no_flag_dc",
        "no_geometric_delay",
        "no_passband_gains",
        "no_cable_delay",
        "no_rfi",
    ];

    let (args, _) = image_args(&[
        "giant-squid",
        "submit-image",
        "--output-mode",
        "fits",
        TEST_OBS_ID,
    ]);
    let json = json_of(&args.to_params(crate::test_config::test_obs_id()));
    for name in SWITCHES {
        assert_eq!(json[name], false, "{name} is off by default");
    }

    let (args, _) = image_args(&[
        "giant-squid",
        "submit-image",
        "--output-mode",
        "fits",
        "--no-digital-gains",
        "--no-flag-dc",
        "--no-geometric-delay",
        "--no-passband-gains",
        "--no-cable-delay",
        "--no-rfi",
        TEST_OBS_ID,
    ]);
    let json = json_of(&args.to_params(crate::test_config::test_obs_id()));
    for name in SWITCHES {
        assert_eq!(json[name], true, "{name} is on when given");
    }
}

/// The imaging job from a conversion job does not take the switches: the
/// conversion job it starts from has already applied them or not.
#[test]
fn submit_image_from_job_does_not_take_the_conversion_switches() {
    for flag in ["--no-digital-gains", "--no-rfi"] {
        let result = Args::try_parse_from([
            "giant-squid",
            "submit-image-from-job",
            "--output-mode",
            "fits",
            "--source-job-id",
            "5",
            flag,
        ]);
        assert!(result.is_err(), "{flag} must be refused");
    }
}

/// A UTC time, for the expected values below.
fn utc(time: &str) -> jiff::Timestamp {
    time.parse().expect("a valid time")
}

#[test]
fn list_takes_the_date_and_sort_filters() {
    match parse(&[
        "giant-squid",
        "list",
        "--date-from",
        "2026-09-01",
        "--date-to",
        "2026-09-30T00:00:00Z",
        "--sort-by",
        "created",
    ]) {
        Args::List {
            date_from,
            date_to,
            sort_by,
            ..
        } => {
            assert_eq!(date_from, Some(utc("2026-09-01T00:00:00Z")));
            assert_eq!(date_to, Some(utc("2026-09-30T00:00:00Z")));
            assert_eq!(sort_by, Some(crate::mwa_asvo::SortBy::Created));
        }
        other => panic!("expected List, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// The help of `list --job-states` and `--job-types`
// ---------------------------------------------------------------------------

/// The names that the help of an option of `list` offers: its
/// `[possible values: ...]`.
fn names_offered_by_list_help(option: &str) -> Vec<String> {
    use clap::CommandFactory;

    let cli = Args::command();
    let list = cli.find_subcommand("list").expect("list exists");
    let names: Vec<String> = list
        .get_arguments()
        .find(|arg| arg.get_long() == Some(option))
        .unwrap_or_else(|| panic!("list has no --{option}"))
        .get_value_parser()
        .possible_values()
        .unwrap_or_else(|| panic!("--{option} lists no values"))
        .map(|value| value.get_name().to_string())
        .collect();
    names
}

/// The help must offer every job state that the parser accepts, and only
/// those: it once offered `retrieving`, which is not a state.
#[test]
fn the_help_of_job_states_offers_the_states_the_parser_accepts() {
    use crate::mwa_asvo::JobState;

    let offered = names_offered_by_list_help("job-states");

    assert_eq!(
        offered,
        [
            "preparing",
            "queued",
            "waitcal",
            "staging",
            "staged",
            "downloading",
            "preprocessing",
            "imaging",
            "delivering",
            "completed",
            "error",
            "cancelled",
        ]
    );
    for name in &offered {
        assert!(
            JobState::parse_name(name).is_ok(),
            "the help offers {name}, which is not a job state"
        );
    }
}

/// Every name in the help must be a job type that the parser accepts.
#[test]
fn the_help_of_job_types_offers_the_types_the_parser_accepts() {
    use crate::mwa_asvo::JobType;

    let offered = names_offered_by_list_help("job-types");

    assert_eq!(
        offered,
        [
            "conversion",
            "visibility",
            "metadata",
            "voltage",
            "cancel",
            "beamformer",
            "imaging",
        ]
    );
    for name in &offered {
        assert!(
            JobType::parse_name(name).is_ok(),
            "the help offers {name}, which is not a job type"
        );
    }
}

// ---------------------------------------------------------------------------
// `run_cli`: the exit codes
// ---------------------------------------------------------------------------

/// The help is a success. (The text goes to standard output.)
#[test]
fn run_cli_returns_zero_for_help_and_version() {
    use super::run::run_cli;

    assert_eq!(run_cli(["giant-squid", "--help"]), 0);
    assert_eq!(run_cli(["giant-squid", "--version"]), 0);
    assert_eq!(run_cli(["giant-squid", "list", "--help"]), 0);
}

/// `--version` prints the name of the program, not the name of the crate
/// (`mwa_giant_squid`), whatever name the program is started with.
#[test]
fn the_version_has_the_name_of_the_program() {
    use clap::CommandFactory;

    let expected = format!(
        "{} {}\nMWA ASVO API {}\nOpenAPI {}\n",
        super::PROGRAM_NAME,
        env!("CARGO_PKG_VERSION"),
        MWA_ASVO_API_VERSION,
        OPENAPI_VERSION
    );
    assert_eq!(Args::command().render_version(), expected);

    let err = Args::try_parse_from(["/some/dir/another-name", "--version"])
        .expect_err("--version stops the parse");
    assert_eq!(err.kind(), clap::error::ErrorKind::DisplayVersion);
    assert_eq!(err.to_string(), expected);
}

/// A bad argument is clap's usage error, code 2, and the command runs
/// nothing. (The text goes to standard error.) These cases stop before the
/// command installs the process's logger, which a test of the library must
/// not do.
#[test]
fn run_cli_returns_two_for_a_bad_argument() {
    use super::run::run_cli;

    assert_eq!(run_cli(["giant-squid", "no-such-command"]), 2);
    assert_eq!(run_cli(["giant-squid", "list", "--no-such-option"]), 2);
    assert_eq!(run_cli(["giant-squid", "list", "--days", "99"]), 2);
}

/// The help has colours on a terminal: clap's default styles have none.
#[test]
fn the_help_has_colours() {
    use clap::CommandFactory;

    let help = Args::command()
        .color(clap::ColorChoice::Always)
        .render_help()
        .ansi()
        .to_string();
    let green = super::HELP_STYLES.get_header().render().to_string();
    assert!(help.contains(&green), "no coloured header in: {help:?}");
}

/// With no options, every submit command sends the schema's defaults of its
/// own request type. The commands share argument groups that are generic
/// over the schema type, so this also checks that one job type does not
/// show (or send) the defaults of another: conversion is parsed first, and
/// its `apply_di_cal` default (false) differs from imaging's (true).
#[test]
fn each_submit_command_sends_the_defaults_of_its_own_schema_type() {
    use crate::mwa_asvo::api::openapi::{
        ConversionJobParams, DownloadJobParams, ImageFromJobParams, ImagingJobParams,
    };

    let obs_id: crate::obs_id::ObsId = TEST_OBS_ID.parse().expect("a valid Obs ID");

    let conv = match parse(&["giant-squid", "submit-conv", TEST_OBS_ID]) {
        Args::SubmitConv { conv, .. } => conv.to_params(obs_id),
        other => panic!("expected SubmitConv, got {other:?}"),
    };
    let expected: ConversionJobParams = ConversionJobParams::builder()
        .obs_id(TEST_OBS_ID_I64)
        .try_into()
        .expect("defaults build");
    assert_eq!(body_json(&conv), body_json(&expected));

    let image = match parse(&[
        "giant-squid",
        "submit-image",
        "--output-mode",
        "fits",
        TEST_OBS_ID,
    ]) {
        Args::SubmitImage { image, .. } => image.to_params(obs_id),
        other => panic!("expected SubmitImage, got {other:?}"),
    };
    let expected: ImagingJobParams = ImagingJobParams::builder()
        .obs_id(TEST_OBS_ID_I64)
        .output_mode(ImageOutputMode::Fits)
        .try_into()
        .expect("defaults build");
    assert!(
        expected.apply_di_cal,
        "the test needs imaging's default to differ"
    );
    assert_eq!(body_json(&image), body_json(&expected));

    let from_job = match parse(&[
        "giant-squid",
        "submit-image-from-job",
        "--output-mode",
        "fits",
        "--source-job-id",
        "5",
    ]) {
        Args::SubmitImageFromJob { image, .. } => image.to_params(),
        other => panic!("expected SubmitImageFromJob, got {other:?}"),
    };
    let expected: ImageFromJobParams = ImageFromJobParams::builder()
        .source_job_id(std::num::NonZeroU64::new(5).expect("not zero"))
        .output_mode(ImageOutputMode::Fits)
        .try_into()
        .expect("defaults build");
    assert_eq!(body_json(&from_job), body_json(&expected));

    let vis = match parse(&["giant-squid", "submit-vis", TEST_OBS_ID]) {
        Args::SubmitVis { download, .. } => download.to_vis_download().to_params(obs_id),
        other => panic!("expected SubmitVis, got {other:?}"),
    };
    let expected: DownloadJobParams = DownloadJobParams::builder()
        .obs_id(TEST_OBS_ID_I64)
        .try_into()
        .expect("defaults build");
    assert_eq!(body_json(&vis), body_json(&expected));

    let bf = match parse(&["giant-squid", "submit-bf", TEST_OBS_ID]) {
        Args::SubmitBf { bf, .. } => bf.to_vis_download().to_params(obs_id),
        other => panic!("expected SubmitBf, got {other:?}"),
    };
    let expected: DownloadJobParams = DownloadJobParams::builder()
        .obs_id(TEST_OBS_ID_I64)
        .try_into()
        .expect("defaults build");
    assert_eq!(body_json(&bf), body_json(&expected));
}

/// A request body as JSON, for the comparisons above.
fn body_json<T: serde::Serialize>(body: &T) -> Value {
    serde_json::to_value(body).expect("a request body serialises")
}

/// The job table is monochrome with `--no-colour`, or with `NO_COLOR` set to
/// any text that is not empty.
#[test]
fn the_job_table_is_monochrome_with_no_colour_or_no_color() {
    use super::table::monochrome;
    use std::ffi::OsString;

    assert!(!monochrome(false, None));
    assert!(monochrome(true, None));
    assert!(monochrome(false, Some(OsString::from("1"))));
    assert!(!monochrome(false, Some(OsString::new())));
}

// ---------------------------------------------------------------------------
// null
// ---------------------------------------------------------------------------

/// An option for a field that the schema allows to be `null` takes the word
/// `null`, which is sent as `null`; its default is the schema's value, and
/// the bare flag is `true`.
#[test]
fn a_nullable_option_takes_null() {
    let body = |extra: &[&str]| {
        let mut argv = vec!["giant-squid", "submit-conv"];
        argv.extend_from_slice(extra);
        argv.push(TEST_OBS_ID);
        let (args, _) = conv_args(&argv);
        json_of(&args.to_params(crate::test_config::test_obs_id()))
    };

    let defaults = body(&[]);
    assert_eq!(defaults["avg_freq_res"], 40.0);
    assert_eq!(defaults["no_rfi"], false);
    assert_eq!(defaults["allow_resubmit"], false);

    let nulls = body(&[
        "--avg-freq-res",
        "null",
        "--avg-time-res=NULL",
        "--no-rfi=null",
        "--allow-resubmit=null",
    ]);
    for field in ["avg_freq_res", "avg_time_res", "no_rfi", "allow_resubmit"] {
        assert_eq!(nulls[field], Value::Null, "{field}");
    }

    let flags = body(&["--no-rfi", "--no-flag-dc=false", "-r"]);
    assert_eq!(flags["no_rfi"], true);
    assert_eq!(flags["no_flag_dc"], false);
    assert_eq!(flags["allow_resubmit"], true);
}

/// The thresholds of an imaging job can be `null` too, and a value that is not
/// a number or `null` is refused.
#[test]
fn a_threshold_takes_null_and_a_bad_value_is_refused() {
    let (args, _) = image_args(&[
        "giant-squid",
        "submit-image",
        "--output-mode",
        "fits",
        "--abs-threshold",
        "null",
        TEST_OBS_ID,
    ]);
    let json = json_of(&args.to_params(crate::test_config::test_obs_id()));
    assert_eq!(json["abs_threshold"], Value::Null);
    assert_eq!(json["clean_threshold"], Value::Null);

    let err = parse_err(&[
        "giant-squid",
        "submit-conv",
        "--avg-freq-res",
        "nope",
        TEST_OBS_ID,
    ]);
    assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
    let err = parse_err(&["giant-squid", "submit-conv", "--no-rfi=maybe", TEST_OBS_ID]);
    assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
}

/// `--no-apply-amps`, `--join-polarizations` and `--multiscale` are booleans
/// of the schema that cannot be `null`: each takes `true` or `false` (the bare
/// flag is `true`), shows the schema default in `--help`, and refuses `null`.
#[test]
fn the_amps_polarization_and_multiscale_switches_take_true_or_false() {
    use clap::CommandFactory;

    let conv = |extra: &[&str]| {
        let mut argv = vec!["giant-squid", "submit-conv"];
        argv.extend_from_slice(extra);
        argv.push(TEST_OBS_ID);
        json_of(
            &conv_args(&argv)
                .0
                .to_params(crate::test_config::test_obs_id()),
        )
    };
    let image = |extra: &[&str]| {
        let mut argv = vec!["giant-squid", "submit-image", "--output-mode", "fits"];
        argv.extend_from_slice(extra);
        argv.push(TEST_OBS_ID);
        json_of(
            &image_args(&argv)
                .0
                .to_params(crate::test_config::test_obs_id()),
        )
    };

    assert_eq!(conv(&[])["no_apply_amps"], false, "default");
    assert_eq!(conv(&["--no-apply-amps"])["no_apply_amps"], true, "bare");
    assert_eq!(conv(&["--no-apply-amps=true"])["no_apply_amps"], true);
    assert_eq!(conv(&["--no-apply-amps=false"])["no_apply_amps"], false);
    for (flag, field) in [
        ("--no-apply-amps", "no_apply_amps"),
        ("--join-polarizations", "join_polarizations"),
        ("--multiscale", "multiscale"),
    ] {
        assert_eq!(image(&[])[field], false, "{field} default");
        assert_eq!(image(&[flag])[field], true, "{field} bare");
        assert_eq!(image(&[&format!("{flag}=true")])[field], true);
        assert_eq!(image(&[&format!("{flag}=false")])[field], false);
        for command in ["submit-conv", "submit-image"] {
            if command == "submit-conv" && flag != "--no-apply-amps" {
                continue;
            }
            let err = parse_err(&["giant-squid", command, &format!("{flag}=null"), TEST_OBS_ID]);
            assert_eq!(
                err.kind(),
                clap::error::ErrorKind::InvalidValue,
                "{command} {flag}=null"
            );
        }
    }

    // The same two WSClean switches of `submit-image-from-job`.
    for (flag, field) in [
        ("--join-polarizations", "join_polarizations"),
        ("--multiscale", "multiscale"),
    ] {
        let body = |extra: &[&str]| {
            let mut argv = vec![
                "giant-squid",
                "submit-image-from-job",
                "--output-mode",
                "fits",
                "--source-job-id",
                "1",
            ];
            argv.extend_from_slice(extra);
            json_of(&image_from_job_args(&argv).to_params())
        };
        assert_eq!(body(&[])[field], false, "{field} default");
        assert_eq!(body(&[flag])[field], true, "{field} bare");
        assert_eq!(body(&[&format!("{flag}=false")])[field], false);
    }

    // `--help` shows the schema default.
    let cli = Args::command();
    for (command, flag) in [
        ("submit-conv", "no_apply_amps"),
        ("submit-image", "no_apply_amps"),
        ("submit-image", "join_polarizations"),
        ("submit-image", "multiscale"),
        ("submit-image-from-job", "join_polarizations"),
        ("submit-image-from-job", "multiscale"),
    ] {
        let arg = cli
            .find_subcommand(command)
            .and_then(|c| c.get_arguments().find(|a| a.get_id() == flag))
            .unwrap_or_else(|| panic!("{command} has no {flag}"));
        let defaults: Vec<_> = arg.get_default_values().iter().collect();
        assert_eq!(defaults, ["false"], "{command} {flag}");
    }
}

/// The short flags of the commands, and what each means. `-f` is
/// `--delivery-format` and `-n` is `--dry-run` wherever they are used, and
/// the other letters are as they were in giant-squid 2.5.1 (`-d`, `-o` and
/// `-r` still mean more than one thing).
#[test]
fn the_short_flags_mean_what_the_table_says() {
    use clap::CommandFactory;
    use std::collections::BTreeMap;

    let mut meanings: BTreeMap<char, std::collections::BTreeSet<String>> = BTreeMap::new();
    let cli = Args::command();
    for command in cli.get_subcommands() {
        for arg in command.get_arguments() {
            if let (Some(short), Some(long)) = (arg.get_short(), arg.get_long()) {
                if !matches!(long, "help" | "version") {
                    meanings.entry(short).or_default().insert(long.to_string());
                }
            }
        }
    }
    let meanings: Vec<(char, Vec<String>)> = meanings
        .into_iter()
        .map(|(short, longs)| (short, longs.into_iter().collect()))
        .collect();

    let expected: [(char, &[&str]); 11] = [
        ('c', &["concurrent-downloads"]),
        ('d', &["delivery", "download-dir"]),
        ('f', &["delivery-format"]),
        ('j', &["json"]),
        ('k', &["keep-tar"]),
        ('n', &["dry-run"]),
        ('o', &["offset", "output", "output-mode"]),
        ('r', &["allow-resubmit", "no-resume"]),
        ('u', &["duration"]),
        ('v', &["verbosity"]),
        ('w', &["wait"]),
    ];
    let expected: Vec<(char, Vec<String>)> = expected
        .iter()
        .map(|(short, longs)| (*short, longs.iter().map(|l| l.to_string()).collect()))
        .collect();
    assert_eq!(meanings, expected);
}

/// The options that lost their short flag are still there with their long
/// name, and the short flag is refused.
#[test]
fn the_options_without_a_short_flag_still_have_a_long_name() {
    let (args, _) = volt_args(&[
        "giant-squid",
        "submit-volt",
        "--offset",
        "0",
        "--duration",
        "8",
        "--from-channel",
        "1",
        "--to-channel",
        "2",
        TEST_OBS_ID,
    ]);
    assert_eq!(args.from_channel, Some(1));
    assert_eq!(args.to_channel, Some(2));

    for argv in [
        ["giant-squid", "submit-volt", "-f", "1", TEST_OBS_ID],
        ["giant-squid", "submit-volt", "-t", "2", TEST_OBS_ID],
        ["giant-squid", "list", "-n", "-n", "-n"],
        ["giant-squid", "wait", "-n", TEST_JOB_ID, "-n"],
    ] {
        assert!(Args::try_parse_from(argv).is_err(), "{argv:?}");
    }
    match parse(&["giant-squid", "list", "--no-colour"]) {
        Args::List { no_colour, .. } => assert!(no_colour),
        other => panic!("expected List, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// delivery and format, and days and dates
// ---------------------------------------------------------------------------

/// The jobs of `list` are selected by `--days` or by the dates: clap refuses
/// `--days` given with a date, and a date does not conflict with the default
/// of `--days`.
#[test]
fn list_days_conflicts_with_the_dates() {
    for date in [
        ["--date-from", "2026-09-01"],
        ["--date-to", "2026-09-30T00:00:00Z"],
    ] {
        let err = parse_err(&["giant-squid", "list", "--days", "5", date[0], date[1]]);
        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::ArgumentConflict,
            "{date:?}"
        );

        // A date alone is fine, and `days` has its default.
        let args = Args::try_parse_from(["giant-squid", "list", date[0], date[1]])
            .expect("a date without --days is valid");
        let Args::List { days, .. } = args else {
            panic!("expected list");
        };
        assert_eq!(days, Some(super::params::list_days_default()));
    }
}

/// With a date, `list` does not send the default `days`: the library gets no
/// `days`, so that the request has `"days": null` and the dates select the
/// jobs. Without a date, `days` is kept.
#[test]
fn list_with_a_date_has_no_days() {
    use super::run::list_days;

    let days = Some(super::params::list_days_default());
    let time: jiff::Timestamp = "2026-09-01T00:00:00Z".parse().expect("a valid time");
    assert_eq!(list_days(days, None, None), days);
    assert_eq!(list_days(days, Some(time), None), None);
    assert_eq!(list_days(days, None, Some(time)), None);
    assert_eq!(list_days(days, Some(time), Some(time)), None);
}

// ---------------------------------------------------------------------------
// status of the JSON lines
// ---------------------------------------------------------------------------

/// An error line has `"status": "failed"` as its first key, whatever the
/// error is, and the key of the `ErrorResponse` follow it.
#[test]
fn an_error_line_has_the_failed_status_first() {
    use super::json_output::JsonError;

    let line = |error: &anyhow::Error| serde_json::to_string(&JsonError::new(error)).expect("JSON");

    let plain = line(&anyhow::anyhow!("something broke"));
    assert!(
        plain.starts_with(r#"{"status":"failed","error_code":"#),
        "{plain}"
    );

    // With the Job ID and the Obs ID that the error is about.
    let with_ids = serde_json::to_string(
        &JsonError::new(&anyhow::anyhow!("not found"))
            .with_job_id(crate::test_config::nonzero(12345))
            .with_obs_id(crate::test_config::test_obs_id()),
    )
    .expect("JSON");
    assert!(with_ids.starts_with(r#"{"status":"failed","#), "{with_ids}");
    let json: Value = serde_json::from_str(&with_ids).expect("JSON");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["job_id"], 12345);
    assert_eq!(json["obs_id"], TEST_OBS_ID_I64);

    // A usage error found by clap is an error line too.
    let usage = parse_err(&["giant-squid", "list", "--days", "99"]);
    let usage = serde_json::to_string(&JsonError::usage(&usage)).expect("JSON");
    assert!(usage.starts_with(r#"{"status":"failed","#), "{usage}");
}

/// The reply of the MWA ASVO has `status` first, as the MWA ASVO sent it
/// (`success` or `failed`), and all its other keys.
#[test]
fn a_reply_line_has_the_status_first() {
    use super::json_output::StatusFirst;
    use crate::mwa_asvo::api::openapi::{JobCancelledResponse, JobSubmittedResponse, Status};

    for status in [Status::Success, Status::Failed] {
        let submitted = JobSubmittedResponse {
            job_id: crate::test_config::nonzero(4321),
            message: "Job submitted".to_string(),
            status,
        };
        let line = serde_json::to_string(&StatusFirst(&submitted)).expect("JSON");
        assert!(
            line.starts_with(&format!(r#"{{"status":"{status}","#)),
            "{line}"
        );
        let json: Value = serde_json::from_str(&line).expect("JSON");
        assert_eq!(json["job_id"], 4321);
        assert_eq!(json["message"], "Job submitted");
        assert_eq!(json.as_object().expect("an object").len(), 3);

        let cancelled = JobCancelledResponse {
            job_id: crate::test_config::nonzero(12345),
            message: "Job cancelled".to_string(),
            status,
        };
        let line = serde_json::to_string(&StatusFirst(&cancelled)).expect("JSON");
        assert!(
            line.starts_with(&format!(r#"{{"status":"{status}","#)),
            "{line}"
        );
    }
}

/// The report of a successful download has `status` first too.
#[test]
fn a_download_report_has_the_status_first() {
    use super::run::DownloadReport;
    use crate::mwa_asvo::api::openapi::Status;

    let report = DownloadReport {
        status: Status::Success,
        job_id: crate::test_config::nonzero(12345),
        obs_id: crate::test_config::test_obs_id(),
        message: "Downloaded".to_string(),
    };
    let line = serde_json::to_string(&report).expect("JSON");
    assert!(
        line.starts_with(r#"{"status":"success","job_id":12345,"obs_id":"#),
        "{line}"
    );
}

/// An infinite number is refused by the option, as any value outside the
/// schema's limits is: serde_json would send it as `null`.
#[test]
fn an_infinite_number_is_refused() {
    for bad in [
        vec![
            "giant-squid",
            "submit-conv",
            "--avg-time-res",
            "inf",
            TEST_OBS_ID,
        ],
        vec![
            "giant-squid",
            "submit-image",
            "--output-mode",
            "fits",
            "--uv-min=-inf",
            TEST_OBS_ID,
        ],
    ] {
        let err = parse_err(&bad);
        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::ValueValidation,
            "{bad:?}"
        );
    }
}

/// `download` fails, before any request, when a file of IDs has none, or
/// when the download directory is not a directory.
#[test]
fn download_refuses_no_ids_and_a_download_dir_that_is_a_file() {
    use super::run::run_cli;

    let empty = NamedTempFile::new().expect("could not create a file");
    let empty_path = empty.path().display().to_string();
    assert_eq!(
        run_cli(["giant-squid", "download", "-n", empty_path.as_str()]),
        1
    );

    let file = NamedTempFile::new().expect("could not create a file");
    let file_path = file.path().display().to_string();
    assert_eq!(
        run_cli([
            "giant-squid",
            "download",
            "-n",
            "-d",
            file_path.as_str(),
            TEST_OBS_ID
        ]),
        1
    );
}

/// `--wait` with no job submitted makes no request, and returns the result
/// of the submissions.
#[test]
fn waiting_for_no_submitted_job_makes_no_request() {
    use super::json_output::ReportedFailures;
    use super::run::wait_for_submitted;
    use crate::test_common::TestEnv;

    let env = TestEnv::with_session();
    let client = crate::MwaAsvoClient::new(crate::test_config::client_config(&env))
        .expect("client should be created");

    assert!(wait_for_submitted(&client, &[], Ok(())).is_ok());
    let failed = wait_for_submitted(
        &client,
        &[],
        Err(ReportedFailures("1 of 1 job submissions failed".to_string()).into()),
    );
    assert!(failed.is_err());
}

/// A wait that fails after some submissions failed keeps both: the wait's
/// error, with the summary of the submissions as its context.
#[test]
fn a_failed_wait_keeps_the_summary_of_the_failed_submissions() {
    use super::json_output::ReportedFailures;
    use super::run::wait_for_submitted;
    use crate::test_common::TestEnv;

    let env = TestEnv::with_session();
    // An empty job list: the submitted job is not in it.
    env.mock_get_jobs(vec![]);
    let client = crate::MwaAsvoClient::new(crate::test_config::client_config(&env))
        .expect("client should be created");
    let summary = "1 of 2 job submissions failed";

    let err = wait_for_submitted(
        &client,
        &[crate::test_config::TEST_MWA_ASVO_JOB_ID],
        Err(ReportedFailures(summary.to_string()).into()),
    )
    .expect_err("the wait should fail");

    let text = format!("{err:#}");
    assert!(text.contains(summary), "{text}");
    assert!(
        err.chain()
            .any(|cause| cause.downcast_ref::<crate::MwaAsvoError>().is_some()),
        "{text}"
    );
}

// ---------------------------------------------------------------------------
// submit-csv
// ---------------------------------------------------------------------------

#[test]
fn submit_csv_takes_a_file_and_the_submit_options() {
    match parse(&[
        "giant-squid",
        "submit-csv",
        "-n",
        "--json",
        "-r",
        "jobs.csv",
    ]) {
        Args::SubmitCsv {
            submit,
            allow_resubmit,
            file,
        } => {
            assert!(submit.dry_run);
            assert!(submit.json);
            assert!(allow_resubmit);
            assert_eq!(file, std::path::PathBuf::from("jobs.csv"));
        }
        other => panic!("expected SubmitCsv, got {other:?}"),
    }
}

#[test]
fn submit_csv_does_not_allow_resubmit_unless_it_is_asked() {
    match parse(&["giant-squid", "scsv", "jobs.csv"]) {
        Args::SubmitCsv {
            submit,
            allow_resubmit,
            ..
        } => {
            assert!(!allow_resubmit);
            assert!(!submit.wait);
            assert!(!submit.dry_run);
        }
        other => panic!("expected SubmitCsv, got {other:?}"),
    }
}

#[test]
fn submit_csv_needs_a_file() {
    let err = parse_err(&["giant-squid", "submit-csv"]);

    assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
}

#[test]
fn submit_csv_refuses_verbosity_with_json() {
    let err = parse_err(&["giant-squid", "submit-csv", "-v", "--json", "jobs.csv"]);

    assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
}

// ---------------------------------------------------------------------------
// download: the directory of each job
// ---------------------------------------------------------------------------

#[test]
fn download_puts_each_job_in_a_directory_unless_it_is_told_not_to() {
    match parse(&["giant-squid", "download", "12345"]) {
        Args::Download { no_job_subdir, .. } => assert!(!no_job_subdir),
        other => panic!("expected Download, got {other:?}"),
    }
    match parse(&["giant-squid", "download", "--no-job-subdir", "12345"]) {
        Args::Download { no_job_subdir, .. } => assert!(no_job_subdir),
        other => panic!("expected Download, got {other:?}"),
    }
}

#[test]
fn list_fields_are_comma_separated_names_of_job_fields() {
    match parse(&["giant-squid", "list", "--fields", "avg_time_res,Error-Text"]) {
        Args::List { fields, .. } => assert_eq!(fields, ["avg_time_res", "error_text"]),
        other => panic!("expected List, got {other:?}"),
    }
}

#[test]
fn list_fields_refuses_a_name_that_is_not_a_field() {
    let err = parse_err(&["giant-squid", "list", "--fields", "avg_time_res,nonsense"]);

    assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
    let text = err.to_string();
    assert!(text.contains("`nonsense` is not a job field: expected one of: user_id,"));
}

#[test]
fn list_fields_cannot_be_used_with_the_other_outputs() {
    for other in ["--json", "--legacy-json", "--ids"] {
        let err = parse_err(&["giant-squid", "list", "--fields", "user_id", other]);

        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::ArgumentConflict,
            "{other}"
        );
    }
}

#[test]
fn list_help_names_every_field() {
    let err = parse_err(&["giant-squid", "list", "--help"]);
    let help = err.to_string();

    for name in crate::job_field_names() {
        assert!(help.contains(name.as_str()), "{name} is in the help");
    }
}

/// The help of every option of a submit command that is a field of the
/// command's request body is the schema's `x-cmd-help` of that field, then
/// its range when it has one (`[range: ...]`). Every field of the body that
/// is an option has its help from the schema.
#[test]
fn the_help_of_a_body_option_is_the_schema_help() {
    use clap::CommandFactory;

    let schema: Value = serde_json::from_str(include_str!("../mwa_asvo/api/openapi-schema.json"))
        .expect("the schema is JSON");
    let commands = [
        ("submit-download", "DownloadJobParams"),
        ("submit-preprocess", "ConversionJobParams"),
        ("submit-image", "ImagingJobParams"),
        ("submit-image-from-job", "ImageFromJobParams"),
    ];
    let command = Args::command();
    for (name, body) in commands {
        let sub = command
            .find_subcommand(name)
            .unwrap_or_else(|| panic!("no command {name}"));
        let properties = schema["definitions"][body]["properties"]
            .as_object()
            .unwrap_or_else(|| panic!("{body} is not in the schema"));
        let mut checked = 0;
        for arg in sub.get_arguments() {
            let field = arg.get_id().as_str();
            let Some(property) = properties.get(field) else {
                continue;
            };
            let text = property["x-cmd-help"]
                .as_str()
                .unwrap_or_else(|| panic!("{body}.{field} has no x-cmd-help"))
                .trim();
            let help = arg
                .get_help()
                .unwrap_or_else(|| panic!("{name} --{field} has no help"))
                .to_string();
            let rest = help.strip_prefix(text).unwrap_or_else(|| {
                panic!("{name} --{field}: {help:?} is not the schema's {text:?}")
            });
            assert!(
                rest.is_empty() || (rest.starts_with(" [range: ") && rest.ends_with(']')),
                "{name} --{field}: {rest:?} after the schema's help"
            );
            checked += 1;
        }
        assert!(
            checked > 0,
            "{name} has no option that is a field of {body}"
        );
    }
}

/// An option with schema limits shows them in its help, from the library's
/// bounds.
#[test]
fn the_help_of_a_limited_option_shows_its_range() {
    use clap::CommandFactory;

    let command = Args::command();
    let help = |name: &str, field: &str| {
        command
            .find_subcommand(name)
            .and_then(|sub| sub.get_arguments().find(|arg| arg.get_id() == field))
            .and_then(|arg| arg.get_help())
            .unwrap_or_else(|| panic!("{name} --{field} has no help"))
            .to_string()
    };
    assert!(help("submit-download", "offset").ends_with(" [range: 0 to 7200]"));
    assert!(help("submit-download", "from_channel").ends_with(" [range: 0 to 255]"));
    assert!(help("submit-image", "channels_out").ends_with(" [range: at least 1]"));
    assert!(help("submit-volt", "duration").ends_with(" [range: 0 to 7200]"));
}
