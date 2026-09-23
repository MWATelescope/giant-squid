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
use crate::parse_many_jobids_or_obsids;

/// An obsid used throughout these tests, and its `i64` form as the request
/// bodies carry it.
const TEST_OBSID: &str = "1065880128";
const TEST_OBSID_I64: i64 = 1065880128;

/// A job ID, which is distinguishable from an obsid by not having 10 digits.
const TEST_JOBID: &str = "12345";

fn parse(args: &[&str]) -> Args {
    Args::try_parse_from(args).expect("expected these arguments to parse")
}

fn parse_err(args: &[&str]) -> clap::Error {
    Args::try_parse_from(args).expect_err("expected these arguments to be rejected")
}

fn vis_args(args: &[&str]) -> (DownloadJobArgs, Vec<String>) {
    match parse(args) {
        Args::SubmitVis {
            download, obsids, ..
        } => (download, obsids),
        other => panic!("expected SubmitVis, got {other:?}"),
    }
}

fn meta_args(args: &[&str]) -> (DownloadJobArgs, Vec<String>) {
    match parse(args) {
        Args::SubmitMeta {
            download, obsids, ..
        } => (download, obsids),
        other => panic!("expected SubmitMeta, got {other:?}"),
    }
}

fn conv_args(args: &[&str]) -> (ConversionJobArgs, Vec<String>) {
    match parse(args) {
        Args::SubmitConv { conv, obsids, .. } => (conv, obsids),
        other => panic!("expected SubmitConv, got {other:?}"),
    }
}

fn image_args(args: &[&str]) -> (ImagingJobArgs, Vec<String>) {
    match parse(args) {
        Args::SubmitImage { image, obsids, .. } => (image, obsids),
        other => panic!("expected SubmitImage, got {other:?}"),
    }
}

fn image_from_job_args(args: &[&str]) -> (ImagingFromJobArgs, Vec<String>) {
    match parse(args) {
        Args::SubmitImageFromJob { image, obsids, .. } => (image, obsids),
        other => panic!("expected SubmitImageFromJob, got {other:?}"),
    }
}

fn volt_args(args: &[&str]) -> (VoltageJobArgs, Vec<String>) {
    match parse(args) {
        Args::SubmitVolt { volt, obsids, .. } => (volt, obsids),
        other => panic!("expected SubmitVolt, got {other:?}"),
    }
}

fn bf_args(args: &[&str]) -> (BeamformerJobArgs, Vec<String>) {
    match parse(args) {
        Args::SubmitBf { bf, obsids, .. } => (bf, obsids),
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
        parse(&["giant-squid", "d", TEST_JOBID]),
        Args::Download { .. }
    ));
    assert!(matches!(
        parse(&["giant-squid", "sv", TEST_OBSID]),
        Args::SubmitVis { .. }
    ));
    assert!(matches!(
        parse(&["giant-squid", "sc", TEST_OBSID]),
        Args::SubmitConv { .. }
    ));
    assert!(matches!(
        parse(&["giant-squid", "si", TEST_OBSID]),
        Args::SubmitImage { .. }
    ));
    assert!(matches!(
        parse(&[
            "giant-squid",
            "sifj",
            "--source-job-id",
            TEST_JOBID,
            TEST_OBSID
        ]),
        Args::SubmitImageFromJob { .. }
    ));
    assert!(matches!(
        parse(&["giant-squid", "sm", TEST_OBSID]),
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
            TEST_OBSID
        ]),
        Args::SubmitVolt { .. }
    ));
    assert!(matches!(
        parse(&["giant-squid", "sb", TEST_OBSID]),
        Args::SubmitBf { .. }
    ));
    assert!(matches!(
        parse(&["giant-squid", "w", TEST_JOBID]),
        Args::Wait { .. }
    ));
    assert!(matches!(
        parse(&["giant-squid", "c", TEST_JOBID]),
        Args::Cancel { .. }
    ));
}

#[test]
fn unknown_command_is_rejected() {
    let err = parse_err(&["giant-squid", "submit-nothing", TEST_OBSID]);
    assert_eq!(err.kind(), clap::error::ErrorKind::InvalidSubcommand);
}

#[test]
fn verbosity_counts_repeats() {
    match parse(&["giant-squid", "submit-vis", "-vv", TEST_OBSID]) {
        Args::SubmitVis { verbosity, .. } => assert_eq!(verbosity, 2),
        other => panic!("expected SubmitVis, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Obsid arguments: one, many, and from a file
// ---------------------------------------------------------------------------

#[test]
fn single_obsid_is_collected() {
    let (_, obsids) = vis_args(&["giant-squid", "submit-vis", TEST_OBSID]);
    assert_eq!(obsids, vec![TEST_OBSID.to_string()]);
}

#[test]
fn many_obsids_are_collected() {
    let (_, obsids) = vis_args(&[
        "giant-squid",
        "submit-vis",
        "1061311664",
        "1061311784",
        "1061312032",
    ]);
    assert_eq!(obsids.len(), 3);

    let (jobids, parsed) = parse_many_jobids_or_obsids(&obsids).expect("obsids should parse");
    assert!(jobids.is_empty());
    assert_eq!(
        parsed.iter().map(|o| o.get()).collect::<Vec<_>>(),
        vec![1061311664, 1061311784, 1061312032]
    );
}

#[test]
fn obsids_can_come_from_a_file() {
    let mut file = NamedTempFile::new().expect("could not create tmp file");
    writeln!(file, "1061311664 1061311784\n1061312032").expect("could not write tmp file");
    file.flush().expect("could not flush tmp file");
    let path = file.path().display().to_string();

    let (_, obsids) = vis_args(&["giant-squid", "submit-vis", &path]);
    let (jobids, parsed) = parse_many_jobids_or_obsids(&obsids).expect("file should parse");
    assert!(jobids.is_empty());
    assert_eq!(parsed.len(), 3);
}

#[test]
fn a_job_id_is_distinguished_from_an_obsid() {
    // The submit commands reject job IDs; the check itself lives in the
    // binary, but it relies on this classification.
    let (jobids, obsids) =
        parse_many_jobids_or_obsids(&[TEST_JOBID.to_string()]).expect("job ID should parse");
    assert_eq!(jobids.len(), 1);
    assert!(obsids.is_empty());
}

// ---------------------------------------------------------------------------
// submit-vis / submit-meta
// ---------------------------------------------------------------------------

#[test]
fn submit_vis_defaults_come_from_the_schema() {
    let (args, _) = vis_args(&["giant-squid", "submit-vis", TEST_OBSID]);
    assert_eq!(args.delivery, download_defaults().delivery);
    assert_eq!(args.delivery_format, download_defaults().delivery_format);
    assert!(!args.allow_resubmit);
}

#[test]
fn submit_vis_builds_a_vis_download_body() {
    let (args, _) = vis_args(&["giant-squid", "submit-vis", TEST_OBSID]);
    let params = args
        .to_vis_params(TEST_OBSID_I64)
        .expect("params should build");
    let json = json_of(&params);

    assert_eq!(json["obs_id"], TEST_OBSID_I64);
    assert_eq!(json["download_type"], "vis");
    assert_eq!(json["allow_resubmit"], false);
}

#[test]
fn submit_meta_builds_a_meta_download_body() {
    let (args, _) = meta_args(&["giant-squid", "submit-meta", TEST_OBSID]);
    let params = args
        .to_meta_params(TEST_OBSID_I64)
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
        TEST_OBSID,
    ]);
    let params = args
        .to_vis_params(TEST_OBSID_I64)
        .expect("params should build");
    assert_eq!(json_of(&params)["delivery"], "scratch");
}

#[test]
fn allow_resubmit_reaches_the_request_body() {
    let (args, _) = vis_args(&["giant-squid", "submit-vis", "-r", TEST_OBSID]);
    assert!(args.allow_resubmit);
    let params = args
        .to_vis_params(TEST_OBSID_I64)
        .expect("params should build");
    assert_eq!(json_of(&params)["allow_resubmit"], true);
}

// ---------------------------------------------------------------------------
// submit-conv
// ---------------------------------------------------------------------------

#[test]
fn submit_conv_defaults_come_from_the_schema() {
    let (args, _) = conv_args(&["giant-squid", "submit-conv", TEST_OBSID]);
    assert_eq!(args.output, conversion_defaults().output);
    assert_eq!(args.centre, conversion_defaults().centre);
    assert_eq!(args.delivery, conversion_defaults().delivery);
    assert_eq!(args.delivery_format, conversion_defaults().delivery_format);

    // The numeric defaults are checked through the request body, so that
    // floats are compared as JSON values rather than directly.
    let json = json_of(&args.to_params(TEST_OBSID_I64).expect("params should build"));
    assert_eq!(json["avg_freq_res"], conversion_defaults().avg_freq_res);
    assert_eq!(json["avg_time_res"], conversion_defaults().avg_time_res);
    assert_eq!(
        json["flag_edge_width"],
        conversion_defaults().flag_edge_width
    );
}

#[test]
fn submit_conv_builds_a_conversion_body() {
    let (args, _) = conv_args(&["giant-squid", "submit-conv", TEST_OBSID]);
    let params = args.to_params(TEST_OBSID_I64).expect("params should build");
    let json = json_of(&params);

    assert_eq!(json["obs_id"], TEST_OBSID_I64);
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
        TEST_OBSID,
    ]);
    let params = args.to_params(TEST_OBSID_I64).expect("params should build");
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
        TEST_OBSID,
    ]);
    let json = json_of(&args.to_params(TEST_OBSID_I64).expect("params should build"));
    assert_eq!(json["output"], "ms");
    assert_eq!(json["avg_freq_res"], 40.0);
}

// ---------------------------------------------------------------------------
// submit-image
// ---------------------------------------------------------------------------

#[test]
fn submit_image_defaults_come_from_the_schema() {
    let (args, _) = image_args(&["giant-squid", "submit-image", TEST_OBSID]);
    assert_eq!(args.image_size, *imaging1_defaults().image_size);
    assert_eq!(args.weighting, imaging1_defaults().weighting);
    assert_eq!(args.output_mode, imaging1_defaults().output_mode);
    assert_eq!(args.auto_mask, imaging1_defaults().auto_mask);
    assert_eq!(args.nmiter, imaging1_defaults().nmiter.get() as i64);
}

#[test]
fn submit_image_builds_an_imaging_body() {
    let (args, _) = image_args(&["giant-squid", "submit-image", "--pol", "XXYY", TEST_OBSID]);
    let json = json_of(&args.to_params(TEST_OBSID_I64).expect("params should build"));

    assert_eq!(json["obs_id"], TEST_OBSID_I64);
    assert_eq!(json["image_size"], *imaging1_defaults().image_size);
    assert_eq!(json["pol"], "XXYY");
}

/// The clap default for `--pol` on submit-image is "XX,YY", which the
/// schema's `Polarization` type rejects - it only accepts XX, YY and XXYY.
/// This test pins that known defect so it is visible rather than silent;
/// update it when the default is corrected.
#[test]
fn submit_image_default_pol_is_rejected_by_the_schema() {
    let (args, _) = image_args(&["giant-squid", "submit-image", TEST_OBSID]);
    assert!(args.to_params(TEST_OBSID_I64).is_err());
}

#[test]
fn submit_image_custom_centre_is_renamed_for_the_api() {
    let (args, _) = image_args(&[
        "giant-squid",
        "submit-image",
        "--pol",
        "XXYY",
        "--phase-center",
        "custom",
        "--custom-ra",
        "10.5",
        "--custom-dec",
        "-26.7",
        TEST_OBSID,
    ]);
    let json = json_of(&args.to_params(TEST_OBSID_I64).expect("params should build"));

    assert_eq!(json["custom_centre_ra"], 10.5);
    assert_eq!(json["custom_centre_dec"], -26.7);
    assert_eq!(json["centre"], "custom");
}

#[test]
fn submit_image_optional_fields_are_omitted_when_unset() {
    let (args, _) = image_args(&["giant-squid", "submit-image", "--pol", "XXYY", TEST_OBSID]);
    let json = json_of(&args.to_params(TEST_OBSID_I64).expect("params should build"));

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
        "--pol",
        "XXYY",
        TEST_OBSID,
    ]);
    assert!(!args.apply_di_cal);
    assert!(!args.join_channels);

    // Given without a value, the flag takes its default_missing_value.
    let (args, _) = image_args(&[
        "giant-squid",
        "submit-image",
        "--apply-di-cal",
        "--pol",
        "XXYY",
        TEST_OBSID,
    ]);
    assert!(args.apply_di_cal);
}

#[test]
fn submit_image_rejects_an_unsupported_image_size() {
    let err = parse_err(&[
        "giant-squid",
        "submit-image",
        "--image-size",
        "1000",
        TEST_OBSID,
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
            TEST_OBSID,
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
        argv.push(TEST_OBSID);
        let err = parse_err(&argv);
        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::ValueValidation,
            "expected {bad:?} to be rejected"
        );
    }
}

// ---------------------------------------------------------------------------
// submit-image-from-job
// ---------------------------------------------------------------------------

#[test]
fn submit_image_from_job_requires_a_source_job_id() {
    let err = parse_err(&["giant-squid", "submit-image-from-job", TEST_OBSID]);
    assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
}

#[test]
fn submit_image_from_job_builds_a_flow2_body() {
    let (args, obsids) = image_from_job_args(&[
        "giant-squid",
        "submit-image-from-job",
        "--source-job-id",
        "4242",
        TEST_OBSID,
    ]);
    assert_eq!(obsids.len(), 1);
    assert_eq!(args.source_job_id.get(), 4242);
    assert_eq!(args.pol, imaging2_defaults().pol);

    let json = json_of(&args.to_params(TEST_OBSID_I64).expect("params should build"));
    assert_eq!(json["source_job_id"], 4242);
    assert_eq!(json["obs_id"], TEST_OBSID_I64);
    // clean_threshold is optional here, unlike the flow 1 imaging job.
    assert!(json.get("clean_threshold").is_none());
}

#[test]
fn submit_image_from_job_rejects_a_zero_source_job_id() {
    let err = parse_err(&[
        "giant-squid",
        "submit-image-from-job",
        "--source-job-id",
        "0",
        TEST_OBSID,
    ]);
    assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
}

// ---------------------------------------------------------------------------
// submit-volt
// ---------------------------------------------------------------------------

#[test]
fn submit_volt_requires_offset_and_duration() {
    let err = parse_err(&["giant-squid", "submit-volt", TEST_OBSID]);
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
        TEST_OBSID,
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
        TEST_OBSID,
    ]);
    let json = json_of(&args.to_params(TEST_OBSID_I64).expect("params should build"));
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
        TEST_OBSID,
    ]);
    let json = json_of(&args.to_params(TEST_OBSID_I64).expect("params should build"));
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
        TEST_OBSID,
    ]);
    assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
}

// ---------------------------------------------------------------------------
// submit-bf
// ---------------------------------------------------------------------------

#[test]
fn submit_bf_builds_a_beamformer_body() {
    let (args, _) = bf_args(&["giant-squid", "submit-bf", TEST_OBSID]);
    assert_eq!(args.delivery, beamformer_defaults().delivery);
    assert_eq!(args.delivery_format, beamformer_defaults().delivery_format);

    let json = json_of(&args.to_params(TEST_OBSID_I64).expect("params should build"));
    assert_eq!(json["obs_id"], TEST_OBSID_I64);
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
        TEST_OBSID,
    ]) {
        Args::List {
            states,
            types,
            days,
            json,
            jobids_or_obsids,
            ..
        } => {
            assert_eq!(states.len(), 2);
            assert_eq!(types.len(), 2);
            assert_eq!(days, Some(7));
            assert!(json);
            assert_eq!(jobids_or_obsids, vec![TEST_OBSID.to_string()]);
        }
        other => panic!("expected List, got {other:?}"),
    }
}

#[test]
fn list_rejects_an_unknown_state() {
    let err = parse_err(&["giant-squid", "list", "--states", "nonsense"]);
    assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
}

#[test]
fn download_defaults_and_aliases_parse() {
    match parse(&["giant-squid", "download", TEST_JOBID]) {
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
        TEST_JOBID,
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
fn download_accepts_obsids_as_well_as_job_ids() {
    match parse(&["giant-squid", "download", TEST_OBSID, TEST_JOBID]) {
        Args::Download {
            jobids_or_obsids, ..
        } => {
            let (jobids, obsids) =
                parse_many_jobids_or_obsids(&jobids_or_obsids).expect("arguments should parse");
            assert_eq!(jobids.len(), 1);
            assert_eq!(obsids.len(), 1);
        }
        other => panic!("expected Download, got {other:?}"),
    }
}

#[test]
fn wait_and_cancel_take_job_ids() {
    match parse(&["giant-squid", "wait", "--json", TEST_JOBID]) {
        Args::Wait { jobs, json, .. } => {
            assert_eq!(jobs, vec![TEST_JOBID.to_string()]);
            assert!(json);
        }
        other => panic!("expected Wait, got {other:?}"),
    }

    match parse(&["giant-squid", "cancel", "-n", TEST_JOBID]) {
        Args::Cancel { jobs, dry_run, .. } => {
            assert_eq!(jobs, vec![TEST_JOBID.to_string()]);
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
