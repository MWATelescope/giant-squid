//! The parser of the CSV files of manta-ray-client.
//!
//! The six files in `tests/files/manta-ray-client` are the ones that
//! manta-ray-client users have: each is read, and the request bodies are
//! compared with what the `submit-*` commands send for the same options.

use serde_json::{json, Value};

use super::{parse_manta_ray_csv, CsvDefaults, CsvJob, CsvJobParams};
use crate::mwa_asvo::api::openapi::{Delivery, DeliveryFormat};

const BEAMFORMER_FILE: &str =
    include_str!("../../../tests/files/manta-ray-client/beamformer_jobs.csv");
const CONVERSION_FILE: &str =
    include_str!("../../../tests/files/manta-ray-client/conversion_jobs.csv");
const IMAGING_FILE: &str = include_str!("../../../tests/files/manta-ray-client/imaging_jobs.csv");
const META_FILE: &str = include_str!("../../../tests/files/manta-ray-client/meta_jobs.csv");
const VISIBILITY_FILE: &str =
    include_str!("../../../tests/files/manta-ray-client/visibility_jobs.csv");
const VOLTAGE_FILE: &str = include_str!("../../../tests/files/manta-ray-client/voltage_jobs.csv");

/// The jobs of `text`, with no defaults. The test fails if it has an error.
fn parse(text: &str) -> Vec<CsvJob> {
    parse_manta_ray_csv(text, &CsvDefaults::default()).expect("the text should be valid")
}

/// The problems of `text`, as the messages that name the lines.
fn problems(text: &str) -> Vec<String> {
    parse_manta_ray_csv(text, &CsvDefaults::default())
        .expect_err("the text should have a problem")
        .errors
        .iter()
        .map(ToString::to_string)
        .collect()
}

/// The request body of `job`, as it is sent.
fn body(job: &CsvJob) -> Value {
    serde_json::to_value(&job.params).expect("a request body is JSON")
}

/// Assert that `body` has every field of `expected` with the same value.
fn assert_has(body: &Value, expected: Value) {
    for (key, value) in expected.as_object().expect("expected is an object") {
        assert_eq!(&body[key], value, "field {key} of {body}");
    }
}

// ---------------------------------------------------------------------------
// The six files
// ---------------------------------------------------------------------------

#[test]
fn the_beamformer_file_has_two_beamformer_jobs() {
    let jobs = parse(BEAMFORMER_FILE);

    assert_eq!(jobs.len(), 2);
    assert!(jobs
        .iter()
        .all(|j| matches!(j.params, CsvJobParams::Beamformer(_))));
    assert_eq!(jobs[0].obs_id.get(), 1457858440);
    assert_eq!(jobs[1].obs_id.get(), 1459045600);
    assert_has(
        &body(&jobs[0]),
        json!({"obs_id": 1457858440_u64, "delivery": "acacia", "delivery_format": "tar"}),
    );
}

#[test]
fn the_conversion_file_has_two_conversion_jobs() {
    let jobs = parse(CONVERSION_FILE);

    assert_eq!(jobs.len(), 2);
    assert_has(
        &body(&jobs[0]),
        json!({
            "obs_id": 1349904240_u64,
            "avg_time_res": 1.0,
            "avg_freq_res": 80.0,
            "flag_edge_width": 80.0,
            "centre": "phase",
            "delivery": "acacia",
            "output": "uvfits",
            "apply_di_cal": false,
            "no_geometric_delay": false,
            "no_rfi": true,
            "no_passband_gains": false,
            "no_cable_delay": false,
            "no_digital_gains": false,
            "no_flag_dc": false,
        }),
    );
    assert_has(
        &body(&jobs[1]),
        json!({
            "obs_id": 1349904184_u64,
            "centre": "custom",
            "custom_centre_ra": 123.0,
            "custom_centre_dec": -12.0,
        }),
    );
}

#[test]
fn the_imaging_file_has_two_imaging_jobs() {
    let jobs = parse(IMAGING_FILE);

    assert_eq!(jobs.len(), 2);
    assert!(jobs
        .iter()
        .all(|j| matches!(j.params, CsvJobParams::Imaging(_))));
    assert_has(
        &body(&jobs[0]),
        json!({
            "obs_id": 1349904240_u64,
            "flag_edge_width": 80.0,
            "avg_freq_res": 40.0,
            "avg_time_res": 2.0,
            "apply_di_cal": true,
            "image_size": 3072,
            "pixel_scale": 20.0,
            "weighting": "briggs",
            "robust": -0.5,
            "clean_iterations": 100000,
            "centre": "phase",
            "nmiter": 10,
            "mgain": 0.8,
            "auto_mask": 3,
            "uvw_min": 75.0,
            "join_polarizations": false,
            "apply_primary_beam": true,
            "multiscale": false,
            "output_mode": "fits",
            "delivery": "acacia",
            "delivery_format": "tar",
        }),
    );
}

#[test]
fn the_metadata_file_has_two_metadata_downloads() {
    let jobs = parse(META_FILE);

    assert_eq!(jobs.len(), 2);
    for job in &jobs {
        assert_eq!(job.params.description(), "metadata download");
        assert_has(
            &body(job),
            json!({"download_type": "meta", "delivery": "acacia"}),
        );
    }
}

#[test]
fn the_visibility_file_has_two_visibility_downloads() {
    let jobs = parse(VISIBILITY_FILE);

    assert_eq!(jobs.len(), 2);
    for job in &jobs {
        assert_eq!(job.params.description(), "visibility download");
        assert_has(
            &body(job),
            json!({"download_type": "vis", "delivery_format": "tar"}),
        );
    }
}

#[test]
fn the_voltage_file_has_two_voltage_jobs() {
    let jobs = parse(VOLTAGE_FILE);

    assert_eq!(jobs.len(), 2);
    assert_has(
        &body(&jobs[0]),
        json!({"obs_id": 1323577520_u64, "offset": 0, "duration": 16, "delivery": "scratch", "channel_range": false}),
    );
    assert_has(
        &body(&jobs[1]),
        json!({
            "obs_id": 1323507256_u64,
            "from_channel": 185,
            "to_channel": 185,
            "channel_range": true,
        }),
    );
}

#[test]
fn every_file_job_validates_and_has_an_endpoint() {
    for text in [
        BEAMFORMER_FILE,
        CONVERSION_FILE,
        IMAGING_FILE,
        META_FILE,
        VISIBILITY_FILE,
        VOLTAGE_FILE,
    ] {
        for job in parse(text) {
            job.params.validate().expect("the body should be valid");
            assert!(job.params.endpoint().starts_with("/api/v2/"));
        }
    }
}

// ---------------------------------------------------------------------------
// The shape of a file
// ---------------------------------------------------------------------------

#[test]
fn comments_blank_lines_spaces_and_both_line_endings_are_allowed() {
    let text = "\u{feff}# a comment\r\n\r\n   \n  # an indented comment\n\
                obs_id=1349904240 , job_type = d , download_type= vis ,\r\n\
                \tobs_id\t=\t1349904184,job_type=d,download_type=vis_meta,";

    let jobs = parse(text);

    assert_eq!(jobs.len(), 2);
    assert_eq!(jobs[0].line, 5);
    assert_eq!(jobs[1].line, 6);
    assert_eq!(jobs[1].params.description(), "metadata download");
}

#[test]
fn a_file_with_no_job_gives_no_job() {
    assert!(parse("").is_empty());
    assert!(parse("# only a comment\n\n").is_empty());
}

#[test]
fn the_same_obs_id_can_have_several_jobs() {
    let jobs =
        parse("obs_id=1349904240,job_type=d,download_type=vis\nobs_id=1349904240,job_type=c\n");

    assert_eq!(jobs.len(), 2);
    assert_eq!(jobs[0].obs_id, jobs[1].obs_id);
}

#[test]
fn the_names_of_values_ignore_case_hyphens_and_underscores() {
    let jobs = parse("obs_id=1349904240,job_type=C,output=UVFITS,delivery=Acacia,no_rfi=TRUE\n");

    assert_has(&body(&jobs[0]), json!({"output": "uvfits", "no_rfi": true}));
}

#[test]
fn a_cell_with_no_value_is_ignored() {
    let jobs = parse("obs_id=1349904240,job_type=d,download_type=vis,delivery=,delivery_format=\n");

    assert_has(
        &body(&jobs[0]),
        json!({"delivery": "acacia", "delivery_format": "tar"}),
    );
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

#[test]
fn a_key_that_a_line_does_not_give_has_the_schema_default() {
    let bare = parse("obs_id=1349904240,job_type=c\n");

    let defaults = crate::mwa_asvo::api::job_params::conversion_params(
        crate::obs_id::ObsId::validate(1349904240).unwrap(),
    );
    assert_eq!(body(&bare[0]), serde_json::to_value(defaults).unwrap());
}

#[test]
fn the_defaults_of_the_file_fill_the_keys_that_a_line_leaves_out() {
    let defaults = CsvDefaults {
        delivery: Some(Delivery::Scratch),
        delivery_format: Some(DeliveryFormat::Files),
        allow_resubmit: Some(true),
    };
    let text = "obs_id=1349904240,job_type=d,download_type=vis\n\
                obs_id=1349904184,job_type=d,download_type=vis,delivery=dug,delivery_format=tar,allow_resubmit=false\n\
                obs_id=1323577520,job_type=v,offset=0,duration=16\n";

    let jobs = parse_manta_ray_csv(text, &defaults).expect("the file should be valid");

    assert_has(
        &body(&jobs[0]),
        json!({"delivery": "scratch", "delivery_format": "files", "allow_resubmit": true}),
    );
    assert_has(
        &body(&jobs[1]),
        json!({"delivery": "dug", "delivery_format": "tar", "allow_resubmit": false}),
    );
    assert_has(
        &body(&jobs[2]),
        json!({"delivery": "scratch", "allow_resubmit": true}),
    );
}

#[test]
fn a_default_delivery_format_that_does_not_suit_the_delivery_is_refused() {
    let defaults = CsvDefaults {
        delivery_format: Some(DeliveryFormat::Files),
        ..CsvDefaults::default()
    };

    let error = parse_manta_ray_csv("obs_id=1349904240,job_type=b\n", &defaults)
        .expect_err("files with acacia is refused");

    assert_eq!(error.errors.len(), 1);
    assert!(
        error.errors[0].message.contains("delivery_format"),
        "{error}"
    );
}

// ---------------------------------------------------------------------------
// Problems
// ---------------------------------------------------------------------------

#[test]
fn every_bad_line_is_reported_with_its_line_number() {
    let text = "obs_id=1349904240,job_type=d,download_type=vis\n\
                obs_id=1349904184,job_type=x\n\
                # a comment\n\
                job_type=d,download_type=vis\n\
                obs_id=1349904184,job_type=d\n";

    let problems = problems(text);

    assert_eq!(problems.len(), 3, "{problems:?}");
    assert!(problems[0].starts_with("line 2: job_type:"), "{problems:?}");
    assert!(
        problems[1].starts_with("line 4: obs_id is required"),
        "{problems:?}"
    );
    assert!(
        problems[2].starts_with("line 5: download_type is required"),
        "{problems:?}"
    );
}

#[test]
fn the_error_of_a_file_counts_its_problems() {
    let error = parse_manta_ray_csv("job_type=c\nobs_id=1,job_type=c\n", &CsvDefaults::default())
        .expect_err("both lines are bad");

    let text = error.to_string();

    assert!(
        text.starts_with("2 problems in the CSV file:\n  line 1: "),
        "{text}"
    );
    assert!(text.contains("\n  line 2: "), "{text}");
}

#[test]
fn a_key_that_the_job_type_does_not_have_is_an_error_that_lists_the_keys() {
    let problems = problems("obs_id=1349904240,job_type=c,no_rf=true\n");

    assert_eq!(problems.len(), 1);
    assert!(
        problems[0].contains("unknown key `no_rf` for a conversion job"),
        "{problems:?}"
    );
    assert!(problems[0].contains("no_rfi"), "{problems:?}");
}

#[test]
fn a_key_of_another_job_type_is_an_error() {
    let problems = problems("obs_id=1349904240,job_type=d,download_type=vis,output=ms\n");

    assert!(
        problems[0].contains("unknown key `output` for a download job"),
        "{problems:?}"
    );
}

#[test]
fn a_key_given_twice_is_an_error() {
    let problems =
        problems("obs_id=1349904240,job_type=d,download_type=vis,download_type=vis_meta\n");

    assert!(
        problems[0].contains("the key `download_type` is given twice"),
        "{problems:?}"
    );
}

#[test]
fn a_cell_that_is_not_key_value_is_an_error() {
    let problems = problems("obs_id=1349904240,job_type=d,vis\n");

    assert!(
        problems[0].contains("`vis` is not a key=value cell"),
        "{problems:?}"
    );
}

#[test]
fn a_value_with_no_key_is_an_error() {
    let problems = problems("obs_id=1349904240,job_type=d,download_type=vis,=acacia\n");

    assert!(
        problems[0].contains("has a value and no key"),
        "{problems:?}"
    );
}

#[test]
fn an_imaging_job_from_a_conversion_job_points_to_its_command() {
    let problems = problems("obs_id=1349904240,job_type=p\n");

    assert!(
        problems[0].contains("submit-image-from-job"),
        "{problems:?}"
    );
}

#[test]
fn a_bad_value_names_its_key() {
    for (text, expected) in [
        (
            "obs_id=abc,job_type=c\n",
            "obs_id: expected a whole number, got `abc`",
        ),
        (
            "obs_id=1349904240,job_type=c,avg_time_res=fast\n",
            "avg_time_res: expected a number, got `fast`",
        ),
        (
            "obs_id=1349904240,job_type=c,no_rfi=yes\n",
            "no_rfi: expected true or false, got `yes`",
        ),
        (
            "obs_id=1349904240,job_type=c,output=fits\n",
            "output: `fits` is not one of ms, uvfits",
        ),
        (
            "obs_id=1349904240,job_type=d,download_type=meta\n",
            "download_type: `meta` is not one of vis, vis_meta",
        ),
    ] {
        let problems = problems(text);

        assert!(problems[0].contains(expected), "{problems:?}");
    }
}

#[test]
fn a_value_outside_the_limits_of_the_schema_is_refused_at_its_line() {
    for (text, name) in [
        (
            "obs_id=1349904240,job_type=c,flag_edge_width=9999\n",
            "flag_edge_width",
        ),
        (
            "obs_id=1349904240,job_type=i,pixel_scale=1\n",
            "pixel_scale",
        ),
        (
            "obs_id=1349904240,job_type=i,image_size=1000\n",
            "image_size",
        ),
        ("obs_id=1349904240,job_type=i,nmiter=0\n", "nmiter"),
        (
            "obs_id=1323577520,job_type=v,offset=99999,duration=16\n",
            "offset",
        ),
        (
            "obs_id=1349904240,job_type=c,centre=custom\n",
            "custom_centre_ra",
        ),
        (
            "obs_id=1349904240,job_type=c,phase_centre_ra=10,phase_centre_dec=10\n",
            "custom_centre_ra",
        ),
        (
            "obs_id=1349904240,job_type=c,delivery=acacia,delivery_format=files\n",
            "delivery_format",
        ),
    ] {
        let problems = problems(text);

        assert!(problems[0].starts_with("line 1: "), "{problems:?}");
        assert!(problems[0].contains(name), "{name}: {problems:?}");
    }
}

#[test]
fn an_obs_id_that_is_not_valid_is_refused() {
    let problems = problems("obs_id=5,job_type=c\n");

    assert!(problems[0].starts_with("line 1: obs_id: "), "{problems:?}");
}

#[test]
fn a_voltage_channel_range_must_agree_with_the_channels() {
    let no_channels =
        problems("obs_id=1323577520,job_type=v,offset=0,duration=16,channel_range=true\n");
    let with_channels = problems(
        "obs_id=1323577520,job_type=v,offset=0,duration=16,channel_range=false,from_channel=1\n",
    );

    assert!(
        no_channels[0].contains("channel_range=true does not agree"),
        "{no_channels:?}"
    );
    assert!(
        with_channels[0].contains("channel_range=false does not agree"),
        "{with_channels:?}"
    );
}
