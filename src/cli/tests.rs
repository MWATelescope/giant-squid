// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Tests for the CLI definition and the argument-to-request-body mapping.
//!
//! Everything here is offline: no MWA ASVO server is contacted, no job is
//! submitted and no environment variable is read or written. See
//! `docs/TESTING.md` for the wider plan.

use std::io::Write;

use clap::Parser;
use serde_json::Value;
use tempfile::NamedTempFile;

use super::params::{
    beamformer_defaults, conversion_defaults, download_defaults, imaging1_defaults,
    imaging2_defaults, voltage_defaults, BeamformerJobArgs, ConversionJobArgs, DownloadJobArgs,
    ImagingFromJobArgs, ImagingJobArgs, VoltageJobArgs,
};
use super::Args;
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

fn image_from_job_args(args: &[&str]) -> (ImagingFromJobArgs, Vec<String>) {
    match parse(args) {
        Args::SubmitImageFromJob { image, obs_ids, .. } => (image, obs_ids),
        other => panic!("expected SubmitImageFromJob, got {other:?}"),
    }
}

fn volt_args(args: &[&str]) -> (VoltageJobArgs, Vec<String>) {
    match parse(args) {
        Args::SubmitVolt { volt, obs_ids, .. } => (volt, obs_ids),
        other => panic!("expected SubmitVolt, got {other:?}"),
    }
}

fn bf_args(args: &[&str]) -> (BeamformerJobArgs, Vec<String>) {
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
        parse(&["giant-squid", "si", TEST_OBS_ID]),
        Args::SubmitImage { .. }
    ));
    assert!(matches!(
        parse(&[
            "giant-squid",
            "sifj",
            "--source-job-id",
            TEST_JOB_ID,
            TEST_OBS_ID
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

#[test]
fn unknown_command_is_rejected() {
    let err = parse_err(&["giant-squid", "submit-nothing", TEST_OBS_ID]);
    assert_eq!(err.kind(), clap::error::ErrorKind::InvalidSubcommand);
}

#[test]
fn verbosity_counts_repeats() {
    match parse(&["giant-squid", "submit-vis", "-vv", TEST_OBS_ID]) {
        Args::SubmitVis { verbosity, .. } => assert_eq!(verbosity, 2),
        other => panic!("expected SubmitVis, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Obsid arguments: one, many, and from a file
// ---------------------------------------------------------------------------

#[test]
fn single_obs_id_is_collected() {
    let (_, obs_ids) = vis_args(&["giant-squid", "submit-vis", TEST_OBS_ID]);
    assert_eq!(obs_ids, vec![TEST_OBS_ID.to_string()]);
}

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
    assert_eq!(args.delivery, download_defaults().delivery);
    assert_eq!(args.delivery_format, download_defaults().delivery_format);
    assert!(!args.allow_resubmit);
}

#[test]
fn submit_vis_builds_a_vis_download_body() {
    let (args, _) = vis_args(&["giant-squid", "submit-vis", TEST_OBS_ID]);
    let params = args
        .to_vis_params(TEST_OBS_ID_I64)
        .expect("params should build");
    let json = json_of(&params);

    assert_eq!(json["obs_id"], TEST_OBS_ID_I64);
    assert_eq!(json["download_type"], "vis");
    assert_eq!(json["allow_resubmit"], false);
}

#[test]
fn submit_meta_builds_a_meta_download_body() {
    let (args, _) = meta_args(&["giant-squid", "submit-meta", TEST_OBS_ID]);
    let params = args
        .to_meta_params(TEST_OBS_ID_I64)
        .expect("params should build");
    assert_eq!(json_of(&params)["download_type"], "meta");
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
        .to_vis_params(TEST_OBS_ID_I64)
        .expect("params should build");
    assert_eq!(json_of(&params)["delivery"], "scratch");
}

#[test]
fn allow_resubmit_reaches_the_request_body() {
    let (args, _) = vis_args(&["giant-squid", "submit-vis", "-r", TEST_OBS_ID]);
    assert!(args.allow_resubmit);
    let params = args
        .to_vis_params(TEST_OBS_ID_I64)
        .expect("params should build");
    assert_eq!(json_of(&params)["allow_resubmit"], true);
}

// ---------------------------------------------------------------------------
// submit-conv
// ---------------------------------------------------------------------------

#[test]
fn submit_conv_defaults_come_from_the_schema() {
    let (args, _) = conv_args(&["giant-squid", "submit-conv", TEST_OBS_ID]);
    assert_eq!(args.output, conversion_defaults().output);
    assert_eq!(args.centre, conversion_defaults().centre);
    assert_eq!(args.delivery, conversion_defaults().delivery);
    assert_eq!(args.delivery_format, conversion_defaults().delivery_format);

    // The numeric defaults are checked through the request body, so that
    // floats are compared as JSON values rather than directly.
    let json = json_of(
        &args
            .to_params(TEST_OBS_ID_I64)
            .expect("params should build"),
    );
    assert_eq!(json["avg_freq_res"], conversion_defaults().avg_freq_res);
    assert_eq!(json["avg_time_res"], conversion_defaults().avg_time_res);
    assert_eq!(
        json["flag_edge_width"],
        conversion_defaults().flag_edge_width
    );
}

#[test]
fn submit_conv_builds_a_conversion_body() {
    let (args, _) = conv_args(&["giant-squid", "submit-conv", TEST_OBS_ID]);
    let params = args
        .to_params(TEST_OBS_ID_I64)
        .expect("params should build");
    let json = json_of(&params);

    assert_eq!(json["obs_id"], TEST_OBS_ID_I64);
    assert_eq!(json["avg_freq_res"], conversion_defaults().avg_freq_res);
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
    let params = args
        .to_params(TEST_OBS_ID_I64)
        .expect("params should build");
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
    let json = json_of(
        &args
            .to_params(TEST_OBS_ID_I64)
            .expect("params should build"),
    );
    assert_eq!(json["output"], "ms");
    assert_eq!(json["avg_freq_res"], 40.0);
}

// ---------------------------------------------------------------------------
// submit-image
// ---------------------------------------------------------------------------

#[test]
fn submit_image_defaults_come_from_the_schema() {
    let (args, _) = image_args(&["giant-squid", "submit-image", TEST_OBS_ID]);
    assert_eq!(args.image_size, *imaging1_defaults().image_size);
    assert_eq!(args.weighting, imaging1_defaults().weighting);
    assert_eq!(args.output_mode, imaging1_defaults().output_mode);
    assert_eq!(args.auto_mask, imaging1_defaults().auto_mask);
    assert_eq!(args.nmiter, imaging1_defaults().nmiter.get() as i64);
}

#[test]
fn submit_image_builds_an_imaging_body() {
    let (args, _) = image_args(&["giant-squid", "submit-image", TEST_OBS_ID]);
    let json = json_of(
        &args
            .to_params(TEST_OBS_ID_I64)
            .expect("params should build"),
    );

    assert_eq!(json["obs_id"], TEST_OBS_ID_I64);
    assert_eq!(json["image_size"], *imaging1_defaults().image_size);
    assert_eq!(json["pol"], imaging1_defaults().pol.to_string());
}

/// `--pol` is sourced from the schema, which accepts only XX, YY or XXYY on
/// this endpoint. A hardcoded default of "XX,YY" previously made every
/// submit-image run fail when the request body was built.
#[test]
fn submit_image_default_pol_comes_from_the_schema() {
    let (args, _) = image_args(&["giant-squid", "submit-image", TEST_OBS_ID]);
    assert_eq!(args.pol, imaging1_defaults().pol.to_string());
    assert!(args.to_params(TEST_OBS_ID_I64).is_ok());
}

#[test]
fn submit_image_accepts_each_supported_polarisation() {
    for pol in ["XX", "YY", "XXYY"] {
        let (args, _) = image_args(&["giant-squid", "submit-image", "--pol", pol, TEST_OBS_ID]);
        let json = json_of(
            &args
                .to_params(TEST_OBS_ID_I64)
                .expect("params should build"),
        );
        assert_eq!(json["pol"], pol);
    }
}

#[test]
fn submit_image_rejects_an_unsupported_polarisation() {
    for bad in ["XX,YY", "xx", "Q"] {
        let err = parse_err(&["giant-squid", "submit-image", "--pol", bad, TEST_OBS_ID]);
        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::ValueValidation,
            "expected --pol {bad} to be rejected"
        );
    }
}

#[test]
fn submit_image_custom_centre_is_renamed_for_the_api() {
    let (args, _) = image_args(&[
        "giant-squid",
        "submit-image",
        "--phase-center",
        "custom",
        "--custom-ra",
        "10.5",
        "--custom-dec",
        "-26.7",
        TEST_OBS_ID,
    ]);
    let json = json_of(
        &args
            .to_params(TEST_OBS_ID_I64)
            .expect("params should build"),
    );

    assert_eq!(json["custom_centre_ra"], 10.5);
    assert_eq!(json["custom_centre_dec"], -26.7);
    assert_eq!(json["centre"], "custom");
}

/// A negative value has to work both as `--flag value` and `--flag=value`.
/// clap reads a leading '-' as a flag unless the command opts in, so
/// `--custom-dec -26.7` used to fail with "unknown argument '-2'" - a
/// defect for any southern declination, a negative robustness, or a
/// negative uvw_min.
#[test]
fn negative_values_are_accepted_in_either_form() {
    for argv in [
        vec![
            "giant-squid",
            "submit-image",
            "--custom-dec",
            "-26.7",
            "--robust",
            "-1.5",
            TEST_OBS_ID,
        ],
        vec![
            "giant-squid",
            "submit-image",
            "--custom-dec=-26.7",
            "--robust=-1.5",
            TEST_OBS_ID,
        ],
    ] {
        let (args, _) = image_args(&argv);
        let json = json_of(
            &args
                .to_params(TEST_OBS_ID_I64)
                .expect("params should build"),
        );
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
        let json = json_of(
            &args
                .to_params(TEST_OBS_ID_I64)
                .expect("params should build"),
        );
        assert_eq!(json["custom_centre_dec"], -26.7, "argv: {argv:?}");
    }
}

/// Negative numbers being allowed must not swallow the short flags on
/// those same commands.
#[test]
fn allowing_negative_numbers_does_not_break_short_flags() {
    match parse(&["giant-squid", "submit-image", "-n", "-vv", TEST_OBS_ID]) {
        Args::SubmitImage {
            dry_run,
            verbosity,
            wait,
            ..
        } => {
            assert!(dry_run);
            assert_eq!(verbosity, 2);
            assert!(!wait);
        }
        other => panic!("expected SubmitImage, got {other:?}"),
    }
}

#[test]
fn submit_image_optional_fields_are_omitted_when_unset() {
    let (args, _) = image_args(&["giant-squid", "submit-image", TEST_OBS_ID]);
    let json = json_of(
        &args
            .to_params(TEST_OBS_ID_I64)
            .expect("params should build"),
    );

    assert!(json.get("nwlayers").is_none());
    assert!(json.get("uvw_max").is_none());
    assert!(json.get("wstack_nwlayers").is_none());
}

#[test]
fn submit_image_boolean_flags_require_equals() {
    let (args, _) = image_args(&[
        "giant-squid",
        "submit-image",
        "--apply-di-cal=false",
        "--join-channels=false",
        TEST_OBS_ID,
    ]);
    assert!(!args.apply_di_cal);
    assert!(!args.join_channels);

    // Given without a value, the flag takes its default_missing_value.
    let (args, _) = image_args(&["giant-squid", "submit-image", "--apply-di-cal", TEST_OBS_ID]);
    assert!(args.apply_di_cal);
}

#[test]
fn submit_image_rejects_an_unsupported_image_size() {
    let err = parse_err(&[
        "giant-squid",
        "submit-image",
        "--image-size",
        "1000",
        TEST_OBS_ID,
    ]);
    assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
}

#[test]
fn submit_image_accepts_every_supported_image_size() {
    for size in ["512", "1024", "2048", "3072", "4096", "8192"] {
        let (args, _) = image_args(&[
            "giant-squid",
            "submit-image",
            "--image-size",
            size,
            TEST_OBS_ID,
        ]);
        assert_eq!(args.image_size.to_string(), size);
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
        vec!["--nwlayers", "16"],
    ] {
        let mut argv = vec!["giant-squid", "submit-image"];
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

/// The CLI's range errors are the library's, so they read the same in the
/// CLI, the Rust client and the Python module.
#[test]
fn submit_image_range_errors_carry_the_librarys_message() {
    for (flag, value, expected) in [
        ("--mgain", "1.5", "must be between 0.1 and 1 (got 1.5)"),
        ("--uvw-min", "101", "must be at most 100 (got 101)"),
        ("--avg-time-res", "-1", "must be at least 0 (got -1)"),
        (
            "--image-size",
            "1000",
            "must be one of 512, 1024, 2048, 3072, 4096, 8192",
        ),
    ] {
        let err = parse_err(&["giant-squid", "submit-image", flag, value, TEST_OBS_ID]);
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
    let err = parse_err(&["giant-squid", "submit-image-from-job", TEST_OBS_ID]);
    assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
}

#[test]
fn submit_image_from_job_builds_a_flow2_body() {
    let (args, obs_ids) = image_from_job_args(&[
        "giant-squid",
        "submit-image-from-job",
        "--source-job-id",
        "4242",
        TEST_OBS_ID,
    ]);
    assert_eq!(obs_ids.len(), 1);
    assert_eq!(args.source_job_id.get(), 4242);
    assert_eq!(args.pol, imaging2_defaults().pol.to_string());

    let json = json_of(
        &args
            .to_params(TEST_OBS_ID_I64)
            .expect("params should build"),
    );
    assert_eq!(json["source_job_id"], 4242);
    assert_eq!(json["obs_id"], TEST_OBS_ID_I64);
    // Since schema v1.11 clean_threshold has a default here, as in the flow
    // 1 imaging job, and the CLI sends it.
    assert_eq!(
        json["clean_threshold"],
        imaging2_defaults().clean_threshold.unwrap()
    );
}

#[test]
fn submit_image_from_job_rejects_a_zero_source_job_id() {
    let err = parse_err(&[
        "giant-squid",
        "submit-image-from-job",
        "--source-job-id",
        "0",
        TEST_OBS_ID,
    ]);
    assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
}

// ---------------------------------------------------------------------------
// submit-volt
// ---------------------------------------------------------------------------

#[test]
fn submit_volt_requires_offset_and_duration() {
    let err = parse_err(&["giant-squid", "submit-volt", TEST_OBS_ID]);
    assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
}

#[test]
fn submit_volt_delivery_default_comes_from_the_schema() {
    let (args, _) = volt_args(&[
        "giant-squid",
        "submit-volt",
        "--offset",
        "0",
        "--duration",
        "8",
        TEST_OBS_ID,
    ]);
    assert_eq!(args.delivery, voltage_defaults().delivery);
}

#[test]
fn submit_volt_channel_range_is_derived_from_the_channel_bounds() {
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
            .to_params(TEST_OBS_ID_I64)
            .expect("params should build"),
    );
    assert_eq!(json["channel_range"], false);
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
            .to_params(TEST_OBS_ID_I64)
            .expect("params should build"),
    );
    assert_eq!(json["channel_range"], true);
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
    assert_eq!(args.delivery, beamformer_defaults().delivery);
    assert_eq!(args.delivery_format, beamformer_defaults().delivery_format);

    let json = json_of(
        &args
            .to_params(TEST_OBS_ID_I64)
            .expect("params should build"),
    );
    assert_eq!(json["obs_id"], TEST_OBS_ID_I64);
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
        "queued,ready",
        "--types",
        "conversion,download_visibilities",
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
            assert_eq!(days, Some(7));
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
        vec!["--nwlayers", "16"],
        vec!["--uvw-max", "0.5"],
        vec!["--image-size", "1000"],
    ] {
        let mut argv = vec![
            "giant-squid",
            "submit-image-from-job",
            "--source-job-id",
            TEST_JOB_ID,
        ];
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
    for offset in ["--offset=-1", "--offset=5401"] {
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
        "--source-job-id",
        TEST_JOB_ID,
        "--wstack-nwlayers",
        "16",
        TEST_OBS_ID,
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
            .contains("must be between 0 and 5400 (got -1)"),
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
    let json = json_of(
        &args
            .to_params(TEST_OBS_ID_I64)
            .expect("params should build"),
    );

    assert_eq!(json["centre"], "custom");
    assert_eq!(json["custom_centre_ra"], 10.5);
    assert_eq!(json["custom_centre_dec"], -26.7);
}

#[test]
fn submit_image_takes_the_schema_names_for_the_centre() {
    let (args, _) = image_args(&[
        "giant-squid",
        "submit-image",
        "--centre",
        "custom",
        "--custom-centre-ra",
        "10.5",
        "--custom-centre-dec",
        "-26.7",
        TEST_OBS_ID,
    ]);
    let json = json_of(
        &args
            .to_params(TEST_OBS_ID_I64)
            .expect("params should build"),
    );

    assert_eq!(json["centre"], "custom");
    assert_eq!(json["custom_centre_ra"], 10.5);
    assert_eq!(json["custom_centre_dec"], -26.7);
}

#[test]
fn list_takes_the_schema_names_for_its_filters() {
    match parse(&[
        "giant-squid",
        "list",
        "--job-states",
        "queued,error",
        "--job-types",
        "conversion",
    ]) {
        Args::List {
            job_states,
            job_types,
            ..
        } => {
            assert_eq!(job_states.len(), 2);
            assert_eq!(job_types.len(), 1);
        }
        other => panic!("expected List, got {other:?}"),
    }
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

/// The singular name works too, as well as the plural that the help lists.
#[test]
fn list_takes_the_singular_and_the_plural_voltage_type() {
    use crate::asvo::AsvoJobType;

    for name in ["download_voltage", "download_voltages", "DownloadVoltage"] {
        match parse(&["giant-squid", "list", "--job-types", name]) {
            Args::List { job_types, .. } => {
                assert_eq!(job_types, [AsvoJobType::DownloadVoltage], "{name}");
            }
            other => panic!("expected List, got {other:?}"),
        }
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
fn json_sample_jobs() -> crate::asvo::AsvoJobVec {
    use crate::asvo::{
        AsvoFilesArray, AsvoJob, AsvoJobProduct, AsvoJobState, AsvoJobType, AsvoJobVec, Delivery,
    };
    let obs_id = crate::obs_id::ObsId::validate(1065880128).expect("a valid obsid");
    let completed: jiff::Timestamp = "2026-09-08T06:00:00Z".parse().expect("a valid time");
    let created: jiff::Timestamp = "2026-09-08T05:41:54Z".parse().expect("a valid time");
    let mut job_params = serde_json::Map::new();
    job_params.insert("obs_id".to_string(), serde_json::json!(1065880128));
    job_params.insert("delivery".to_string(), serde_json::json!("acacia"));
    AsvoJobVec(vec![
        AsvoJob {
            obs_id,
            job_id: 101,
            job_type: AsvoJobType::DownloadVisibilities,
            job_state: AsvoJobState::Ready,
            product: Some(AsvoJobProduct {
                files: vec![
                    AsvoFilesArray {
                        r#type: Delivery::Acacia,
                        url: Some("https://example.org/a.tar".to_string()),
                        path: None,
                        size: 1234,
                        sha1: Some("ab".repeat(20)),
                        format: None,
                    },
                    AsvoFilesArray {
                        r#type: Delivery::Scratch,
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
        AsvoJob {
            obs_id,
            job_id: 102,
            job_type: AsvoJobType::Conversion,
            job_state: AsvoJobState::Queued,
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
            job_params: serde_json::Map::new(),
        },
        AsvoJob {
            obs_id,
            job_id: 103,
            job_type: AsvoJobType::Imaging,
            job_state: AsvoJobState::Error("the \"conversion\" failed".to_string()),
            product: Some(AsvoJobProduct { files: vec![] }),
            created,
            started: None,
            completed: None,
            modified: None,
            error_code: Some(12),
            error_text: Some("the \"conversion\" failed".to_string()),
            user_id: 4242,
            first_name: "Test".to_string(),
            last_name: "User".to_string(),
            job_params: serde_json::Map::new(),
        },
    ])
}

/// `--legacy-json` prints exactly what `--json` printed before 3.0.0. The
/// expected text was captured from the old code for the same jobs.
#[test]
fn legacy_json_is_the_old_output_byte_for_byte() {
    let expected = r#"{"101":{"obsid":1065880128,"jobId":101,"jobType":"DownloadVisibilities","jobState":"Ready","files":[{"jobType":"Acacia","fileUrl":"https://example.org/a.tar","filePath":null,"fileSize":1234,"fileHash":"abababababababababababababababababababab"},{"jobType":"Scratch","fileUrl":null,"filePath":"/scratch/mwa/x","fileSize":5,"fileHash":null}],"completed":"2026-09-08T06:00:00Z"},"102":{"obsid":1065880128,"jobId":102,"jobType":"Conversion","jobState":"Queued","files":null,"completed":null},"103":{"obsid":1065880128,"jobId":103,"jobType":"Imaging","jobState":{"Error":"the \"conversion\" failed"},"files":[],"completed":null}}"#;

    let output = super::legacy_json::to_legacy_json(&json_sample_jobs()).expect("serialises");

    assert_eq!(output, expected);
}

/// `--json` prints the OpenAPI names, with the same values.
#[test]
fn json_uses_the_openapi_names() {
    let expected = r#"{"101":{"obs_id":1065880128,"job_id":101,"job_type":"DownloadVisibilities","job_state":"Ready","product":{"files":[{"type":"Acacia","url":"https://example.org/a.tar","path":null,"size":1234,"sha1":"abababababababababababababababababababab","format":null},{"type":"Scratch","url":null,"path":"/scratch/mwa/x","size":5,"sha1":null,"format":null}]},"created":"2026-09-08T05:41:54Z","started":"2026-09-08T05:41:54Z","completed":"2026-09-08T06:00:00Z","modified":"2026-09-08T06:00:00Z","error_code":null,"error_text":null,"user_id":4242,"first_name":"Test","last_name":"User","job_params":{"delivery":"acacia","obs_id":1065880128}},"102":{"obs_id":1065880128,"job_id":102,"job_type":"Conversion","job_state":"Queued","product":null,"created":"2026-09-08T05:41:54Z","started":null,"completed":null,"modified":null,"error_code":null,"error_text":null,"user_id":4242,"first_name":"Test","last_name":"User","job_params":{}},"103":{"obs_id":1065880128,"job_id":103,"job_type":"Imaging","job_state":{"Error":"the \"conversion\" failed"},"product":{"files":[]},"created":"2026-09-08T05:41:54Z","started":null,"completed":null,"modified":null,"error_code":12,"error_text":"the \"conversion\" failed","user_id":4242,"first_name":"Test","last_name":"User","job_params":{}}}"#;

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
    let json = json_of(
        &args
            .to_params(TEST_OBS_ID_I64)
            .expect("params should build"),
    );
    assert_eq!(json["no_cable_delay"], true);
    assert_eq!(json["no_rfi"], true);

    let (args, _) = conv_args(&["giant-squid", "submit-conv", TEST_OBS_ID]);
    let json = json_of(
        &args
            .to_params(TEST_OBS_ID_I64)
            .expect("params should build"),
    );
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
        "no_geometry_delay",
        "no_passband_gains",
        "no_cable_delay",
        "no_rfi",
    ];

    let (args, _) = image_args(&["giant-squid", "submit-image", TEST_OBS_ID]);
    let json = json_of(
        &args
            .to_params(TEST_OBS_ID_I64)
            .expect("params should build"),
    );
    for name in SWITCHES {
        assert_eq!(json[name], false, "{name} is off by default");
    }

    let (args, _) = image_args(&[
        "giant-squid",
        "submit-image",
        "--no-digital-gains",
        "--no-flag-dc",
        "--no-geometry-delay",
        "--no-passband-gains",
        "--no-cable-delay",
        "--no-rfi",
        TEST_OBS_ID,
    ]);
    let json = json_of(
        &args
            .to_params(TEST_OBS_ID_I64)
            .expect("params should build"),
    );
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
            "--source-job-id",
            "5",
            flag,
            TEST_OBS_ID,
        ]);
        assert!(result.is_err(), "{flag} must be refused");
    }
}

/// A UTC time, for the expected values below.
fn utc(time: &str) -> jiff::Timestamp {
    time.parse().expect("a valid time")
}

#[test]
fn a_list_time_is_rfc3339_or_a_date() {
    use super::params::parse_utc_time;

    assert_eq!(
        parse_utc_time("2026-09-01T12:30:00+08:00").expect("RFC 3339"),
        utc("2026-09-01T04:30:00Z")
    );
    assert_eq!(
        parse_utc_time("2026-09-01").expect("a date"),
        utc("2026-09-01T00:00:00Z")
    );
    assert!(parse_utc_time("yesterday").is_err());
    // A date and time with no offset is refused, not read as midnight.
    assert!(parse_utc_time("2026-09-01T12:00:00").is_err());
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
            assert_eq!(sort_by.as_deref(), Some("created"));
        }
        other => panic!("expected List, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// `staging_count` is not an option
// ---------------------------------------------------------------------------

/// `staging_count` is for the MWA ASVO's processors and the API will remove
/// it, so no command has an option for it, and no body built from the default
/// options has it.
#[test]
fn staging_count_is_not_an_option_and_not_in_any_body() {
    use clap::CommandFactory;

    let cli = Args::command();
    for command in cli.get_subcommands() {
        for arg in command.get_arguments() {
            let names = format!(
                "{} {:?} {:?}",
                arg.get_id(),
                arg.get_long(),
                arg.get_aliases()
            );
            assert!(
                !names.to_lowercase().contains("staging"),
                "{} has an option for staging: {names}",
                command.get_name()
            );
        }
    }

    let bodies = [
        json_of(
            &vis_args(&["giant-squid", "submit-vis", TEST_OBS_ID])
                .0
                .to_vis_params(TEST_OBS_ID_I64)
                .expect("vis body"),
        ),
        json_of(
            &conv_args(&["giant-squid", "submit-conv", TEST_OBS_ID])
                .0
                .to_params(TEST_OBS_ID_I64)
                .expect("conversion body"),
        ),
        json_of(
            &image_args(&["giant-squid", "submit-image", TEST_OBS_ID])
                .0
                .to_params(TEST_OBS_ID_I64)
                .expect("imaging body"),
        ),
    ];
    for body in bodies {
        assert!(body.get("staging_count").is_none(), "{body}");
    }
}

// ---------------------------------------------------------------------------
// `wait` and `cancel` take job IDs only
// ---------------------------------------------------------------------------

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(ToString::to_string).collect()
}

#[test]
fn job_ids_only_accepts_job_ids() {
    use crate::parse_job_ids_only;

    let job_ids = parse_job_ids_only(&strings(&["31", TEST_JOB_ID])).expect("job IDs parse");

    assert_eq!(job_ids, [31, 12345]);
}

#[test]
fn job_ids_only_reads_job_ids_from_a_file() {
    use crate::parse_job_ids_only;

    let mut file = NamedTempFile::new().expect("a temporary file");
    writeln!(file, "31 32").expect("write the file");
    let path = file.path().to_string_lossy().to_string();

    let job_ids = parse_job_ids_only(&[path]).expect("the file parses");

    assert_eq!(job_ids, [31, 32]);
}

/// An obsid is an error that names the obsid, whether it is alone, with job
/// IDs, or in a file: it is never ignored.
#[test]
fn job_ids_only_refuses_an_obsid() {
    use crate::parse_job_ids_only;

    let mut file = NamedTempFile::new().expect("a temporary file");
    writeln!(file, "31 {TEST_OBS_ID}").expect("write the file");
    let path = file.path().to_string_lossy().to_string();

    for arguments in [
        strings(&[TEST_OBS_ID]),
        strings(&["31", TEST_OBS_ID]),
        strings(&[TEST_OBS_ID, "31"]),
        vec![path],
    ] {
        let err = parse_job_ids_only(&arguments).expect_err("an obsid must be refused");
        let text = err.to_string();

        assert!(
            text.starts_with("Expected only job IDs, but found these obsids: 1065880128."),
            "{arguments:?}: {text}"
        );
        assert!(text.contains("giant-squid list <obsid>"), "{text}");
    }
}

#[test]
fn job_ids_only_names_every_obsid() {
    use crate::parse_job_ids_only;

    let err = parse_job_ids_only(&strings(&[TEST_OBS_ID, "1065880248", "31"]))
        .expect_err("obsids must be refused");

    assert!(
        err.to_string()
            .contains("found these obsids: 1065880128, 1065880248."),
        "{err}"
    );
}

#[test]
fn job_ids_only_needs_a_job_id() {
    use crate::parse_job_ids_only;

    let err = parse_job_ids_only(&[]).expect_err("no job ID is an error");

    assert_eq!(err.to_string(), "No jobids specified!");
}

// ---------------------------------------------------------------------------
// The help of `list --job-states` and `--job-types`
// ---------------------------------------------------------------------------

/// The names that the help of an option of `list` offers: the text after
/// "Options:", split at the commas and the last "or".
fn names_offered_by_list_help(option: &str) -> Vec<String> {
    use clap::CommandFactory;

    let cli = Args::command();
    let list = cli.find_subcommand("list").expect("list exists");
    let help = list
        .get_arguments()
        .find(|arg| arg.get_long() == Some(option))
        .unwrap_or_else(|| panic!("list has no --{option}"))
        .get_help()
        .unwrap_or_else(|| panic!("--{option} has no help"))
        .to_string();
    let (_, names) = help
        .split_once("Options:")
        .unwrap_or_else(|| panic!("the help of --{option} has no list of options: {help}"));

    names
        .replace(" or ", ", ")
        .split(',')
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect()
}

/// The help must offer every job state that the parser accepts, and only
/// those: it once offered `retrieving`, which is not a state.
#[test]
fn the_help_of_job_states_offers_the_states_the_parser_accepts() {
    use crate::asvo::AsvoJobState;
    use std::str::FromStr;

    let offered = names_offered_by_list_help("job-states");

    assert_eq!(
        offered,
        [
            "queued",
            "waitcal",
            "staging",
            "staged",
            "downloading",
            "preparing",
            "preprocessing",
            "imaging",
            "delivering",
            "ready",
            "error",
            "expired",
            "cancelled",
        ]
    );
    for name in &offered {
        assert!(
            AsvoJobState::from_str(name).is_ok(),
            "the help offers {name}, which is not a job state"
        );
    }
}

/// Every name in the help must be a job type that the parser accepts.
#[test]
fn the_help_of_job_types_offers_the_types_the_parser_accepts() {
    use crate::asvo::AsvoJobType;
    use std::str::FromStr;

    let offered = names_offered_by_list_help("job-types");

    assert_eq!(
        offered,
        [
            "conversion",
            "download_visibilities",
            "download_metadata",
            "download_voltages",
            "download_beamformer",
            "imaging",
            "cancel_job",
        ]
    );
    for name in &offered {
        assert!(
            AsvoJobType::from_str(name).is_ok(),
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
