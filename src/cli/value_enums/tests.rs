// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Tests of the schema enum value parser: the allowed values that `--help`
//! lists, and the error for a value that is not allowed.

use std::fmt::Debug;

use clap::error::ErrorKind;
use clap::{CommandFactory, Parser};

use super::*;
use crate::cli::Args;
use crate::mwa_asvo::api::openapi::{
    Centre, Delivery, DeliveryFormat, Output, OutputMode, Polarization, Weighting,
};

// The values in the schema's order, which `--help` and the messages use.
const DELIVERY: &[&str] = &["acacia", "scratch", "dug"];
const DELIVERY_FORMAT: &[&str] = &["tar", "files"];
const OUTPUT: &[&str] = &["ms", "uvfits"];
const CENTRE: &[&str] = &["phase", "pointing", "custom"];
const OUTPUT_MODE: &[&str] = &["fits", "all_fits", "all_files"];
const POLARIZATION: &[&str] = &["XX", "YY", "XXYY"];
const WEIGHTING: &[&str] = &["briggs", "uniform", "natural"];

/// Every option that takes a schema enum: the command, the option and the
/// values that `--help` must list for it. The voltage command is not here:
/// its `--delivery` is a text option, and its help says that only "scratch"
/// is valid.
const OPTIONS: &[(&str, &str, &[&str])] = &[
    ("submit-vis", "delivery", DELIVERY),
    ("submit-vis", "delivery-format", DELIVERY_FORMAT),
    ("submit-meta", "delivery", DELIVERY),
    ("submit-meta", "delivery-format", DELIVERY_FORMAT),
    ("submit-bf", "delivery", DELIVERY),
    ("submit-bf", "delivery-format", DELIVERY_FORMAT),
    ("submit-conv", "delivery", DELIVERY),
    ("submit-conv", "delivery-format", DELIVERY_FORMAT),
    ("submit-conv", "output", OUTPUT),
    ("submit-conv", "centre", CENTRE),
    ("submit-image", "delivery", DELIVERY),
    ("submit-image", "delivery-format", DELIVERY_FORMAT),
    ("submit-image", "centre", CENTRE),
    ("submit-image", "output-mode", OUTPUT_MODE),
    ("submit-image", "pol", POLARIZATION),
    ("submit-image", "weighting", WEIGHTING),
    ("submit-image-from-job", "delivery", DELIVERY),
    ("submit-image-from-job", "delivery-format", DELIVERY_FORMAT),
    ("submit-image-from-job", "output-mode", OUTPUT_MODE),
    ("submit-image-from-job", "pol", POLARIZATION),
    ("submit-image-from-job", "weighting", WEIGHTING),
];

/// The values that the parser of `option` of `command` lists for `--help`.
fn listed_values(command: &str, option: &str) -> Vec<String> {
    let cli = Args::command();
    let subcommand = cli
        .find_subcommand(command)
        .unwrap_or_else(|| panic!("no command {command}"));
    let arg = subcommand
        .get_arguments()
        .find(|arg| arg.get_long() == Some(option))
        .unwrap_or_else(|| panic!("no option --{option} on {command}"));
    arg.get_value_parser()
        .possible_values()
        .unwrap_or_else(|| panic!("--{option} on {command} lists no values"))
        .map(|value| value.get_name().to_string())
        .collect()
}

/// Checks one enum: each variant is listed once, the parser offers the
/// values in the schema's order (the library's list), and each API value
/// parses back to the variant.
fn check_enum<T: SchemaEnum + PartialEq + Debug>() {
    let names: Vec<String> = T::VARIANTS.iter().map(ToString::to_string).collect();
    let mut unique = names.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(names.len(), unique.len(), "a variant twice");
    assert_eq!(T::value_names(), names);

    for variant in T::VARIANTS {
        assert_eq!(
            variant.to_string().parse::<T>().ok().as_ref(),
            Some(variant)
        );
    }
}

#[test]
fn every_schema_enum_is_listed_once_and_in_order() {
    check_enum::<Centre>();
    check_enum::<Delivery>();
    check_enum::<DeliveryFormat>();
    check_enum::<Output>();
    check_enum::<OutputMode>();
    check_enum::<Polarization>();
    check_enum::<Weighting>();
}

#[test]
fn every_enum_option_lists_the_values_of_the_schema() {
    for (command, option, expected) in OPTIONS {
        assert_eq!(
            listed_values(command, option),
            *expected,
            "--{option} of {command}"
        );
    }
}

#[test]
fn the_long_help_shows_the_values() {
    let mut cli = Args::command();
    let help = cli
        .find_subcommand_mut("submit-conv")
        .expect("submit-conv exists")
        .render_long_help()
        .to_string();

    assert!(
        help.contains("[possible values: acacia, scratch, dug]"),
        "help: {help}"
    );
    assert!(
        help.contains("[possible values: tar, files]"),
        "help: {help}"
    );
    assert!(
        help.contains("[possible values: ms, uvfits]"),
        "help: {help}"
    );
}

#[test]
fn a_value_that_is_not_allowed_names_the_allowed_values() {
    let err = Args::try_parse_from([
        "giant-squid",
        "submit-vis",
        "--delivery",
        "tape",
        "1065880128",
    ])
    .expect_err("tape is not a delivery");

    // The kind is the one that clap gives a `FromStr` type, as before.
    assert_eq!(err.kind(), ErrorKind::ValueValidation);
    let text = err.to_string();
    assert!(text.contains("--delivery"), "error: {text}");
    assert!(text.contains("tape"), "error: {text}");
    assert!(text.contains("acacia, scratch, dug"), "error: {text}");
}

/// The polarisation stays the text that the request body carries.
#[test]
fn the_polarisation_option_gives_the_api_text() {
    let args = Args::try_parse_from(["giant-squid", "submit-image", "--pol", "XXYY", "1065880128"])
        .expect("XXYY is a polarisation");
    match args {
        Args::SubmitImage { image, .. } => assert_eq!(image.wsclean.pol.to_string(), "XXYY"),
        other => panic!("expected SubmitImage, got {other:?}"),
    }
}

/// Every named value is parsed in the same way: the case, hyphens and
/// underscores do not matter, for a schema enum, a job state and a job type.
#[test]
fn every_named_value_is_parsed_without_regard_to_case_or_separators() {
    let args = Args::try_parse_from([
        "giant-squid",
        "submit-image",
        "--delivery",
        "ACACIA",
        "--output-mode",
        "All-Fits",
        "--pol",
        "xxyy",
        "1065880128",
    ])
    .expect("the values parse");
    match args {
        Args::SubmitImage { image, .. } => {
            assert_eq!(image.delivery_args.delivery, Delivery::Acacia);
            assert_eq!(image.wsclean.output_mode, OutputMode::AllFits);
            assert_eq!(image.wsclean.pol, Polarization::Xxyy);
        }
        other => panic!("expected SubmitImage, got {other:?}"),
    }

    let args = Args::try_parse_from([
        "giant-squid",
        "list",
        "--job-states",
        "WAIT-CAL,Completed",
        "--job-types",
        "VISIBILITY",
    ])
    .expect("the values parse");
    match args {
        Args::List {
            job_states,
            job_types,
            ..
        } => {
            assert_eq!(
                job_states,
                [
                    crate::mwa_asvo::JobState::Waitcal,
                    crate::mwa_asvo::JobState::Completed
                ]
            );
            assert_eq!(job_types, [crate::test_config::job_type("visibility")]);
        }
        other => panic!("expected List, got {other:?}"),
    }
}
