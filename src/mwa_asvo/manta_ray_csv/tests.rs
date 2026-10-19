//! The parser of the CSV files of manta-ray-client.
//!
//! The six files in `tests/files/manta-ray-client` are the ones that
//! manta-ray-client users have: each is read, and the request bodies are
//! compared with what the `submit-*` commands send for the same options.

use serde_json::{json, Value};

use super::{parse_manta_ray_csv, parse_with_warnings, CsvDefaults, CsvJob, CsvJobParams};
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
fn a_key_given_twice_is_a_warning_and_the_last_value_is_used() {
    let text = "obs_id=1349904240,job_type=d,download_type=vis,download_type=vis_meta\n";

    let (jobs, warnings) =
        parse_with_warnings(text, &CsvDefaults::default()).expect("the text should be valid");

    assert_eq!(jobs[0].params.description(), "metadata download");
    assert_eq!(warnings.len(), 1);
    assert_eq!(
        warnings[0].to_string(),
        "line 1: the key `download_type` is given twice; the last value is used"
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

// ---------------------------------------------------------------------------
// Warnings
// ---------------------------------------------------------------------------

/// The warnings of `text`, as the messages that name the lines.
fn warnings(text: &str) -> Vec<String> {
    parse_with_warnings(text, &CsvDefaults::default())
        .expect("the text should be valid")
        .1
        .iter()
        .map(ToString::to_string)
        .collect()
}

#[test]
fn a_line_that_makes_the_same_job_as_an_earlier_line_is_a_warning() {
    let text = "obs_id=1349904240,job_type=d,download_type=vis\n\
                obs_id=1349904184,job_type=d,download_type=vis\n\
                # a comment\n\
                download_type=vis, job_type=d, obs_id=1349904240, delivery=acacia\n";

    assert_eq!(
        warnings(text),
        ["line 4: this line makes the same job as line 1"]
    );
}

#[test]
fn jobs_that_differ_make_no_warning() {
    let text = "obs_id=1349904240,job_type=d,download_type=vis\n\
                obs_id=1349904240,job_type=d,download_type=vis_meta\n\
                obs_id=1349904240,job_type=c\n";

    assert!(warnings(text).is_empty());
}

#[test]
fn the_six_files_make_no_warning() {
    for text in [
        BEAMFORMER_FILE,
        CONVERSION_FILE,
        IMAGING_FILE,
        META_FILE,
        VISIBILITY_FILE,
        VOLTAGE_FILE,
    ] {
        assert!(warnings(text).is_empty());
    }
}

// ---------------------------------------------------------------------------
// The other keys of the schema
// ---------------------------------------------------------------------------

#[test]
fn the_other_fields_of_the_schema_are_keys_of_an_imaging_job() {
    let text = "obs_id=1349904240, job_type=i, pol=xx_yy, join_channels=true, abs_threshold=0.5, \
                wstack_nwlayers=64, no_apply_amps=true, no_rfi=true, no_flag_dc=false\n";

    let jobs = parse(text);

    assert_has(
        &body(&jobs[0]),
        json!({
            "pol": "XXYY",
            "join_channels": true,
            "abs_threshold": 0.5,
            "wstack_nwlayers": 64,
            "no_apply_amps": true,
            "no_rfi": true,
            "no_flag_dc": false,
        }),
    );
}

#[test]
fn no_apply_amps_is_a_key_of_a_conversion_job() {
    let jobs = parse("obs_id=1349904240,job_type=c,no_apply_amps=true\n");

    assert_has(&body(&jobs[0]), json!({"no_apply_amps": true}));
}

#[test]
fn the_mode_of_a_beamformer_job_is_a_key() {
    let jobs = parse(
        "obs_id=1457858440,job_type=b,mode=mwax_corr_bf\n\
         obs_id=1459045600,job_type=b,mode=MWAX-BEAMFORMER\n",
    );

    assert_has(&body(&jobs[0]), json!({"mode": "MWAX_CORR_BF"}));
    assert_has(&body(&jobs[1]), json!({"mode": "MWAX_BEAMFORMER"}));
}

#[test]
fn a_bad_beamformer_mode_names_the_modes() {
    let problems = problems("obs_id=1457858440,job_type=b,mode=fast\n");

    assert!(
        problems[0].contains("mode: `fast` is not one of MWAX_BEAMFORMER, MWAX_CORR_BF"),
        "{problems:?}"
    );
}

#[test]
fn a_key_that_no_job_type_has_is_still_an_error() {
    let problems = problems("obs_id=1349904240,job_type=i,source_job_id=5\n");

    assert!(
        problems[0].contains("unknown key `source_job_id` for a imaging job"),
        "{problems:?}"
    );
}

// ---------------------------------------------------------------------------
// Submitting a file
// ---------------------------------------------------------------------------

mod submitting {
    use std::path::Path;

    use httpmock::prelude::*;
    use tempfile::TempDir;

    use super::super::{CsvDefaults, CsvSubmitError};
    use crate::mwa_asvo::AsvoClient;
    use crate::test_common::*;
    use crate::test_config::client_config;

    /// A CSV file in a temporary directory.
    fn csv_file(dir: &TempDir, text: &str) -> std::path::PathBuf {
        let path = dir.path().join("jobs.csv");
        std::fs::write(&path, text).expect("could not write the CSV file");
        path
    }

    /// Serve `POST endpoint` for `obs_id` with a successful reply.
    fn mock_submit<'a>(
        env: &'a TestEnv,
        endpoint: &str,
        obs_id: u64,
        job_id: u64,
    ) -> httpmock::Mock<'a> {
        env.server.mock(|when, then| {
            when.method(POST)
                .path(endpoint)
                .json_body_includes(serde_json::json!({ "obs_id": obs_id }).to_string());
            then.status(200)
                .header("content-type", "application/json")
                .json_body(job_submitted_response(job_id));
        })
    }

    #[test]
    fn a_file_gives_the_reply_of_each_job_in_order() {
        let env = TestEnv::with_session();
        let first = mock_submit(&env, "/api/v2/download_vis_job", 1349904240, 11);
        let second = mock_submit(&env, "/api/v2/conversion_job", 1349904184, 12);
        let dir = TempDir::new().expect("could not create a temporary directory");
        let file = csv_file(
            &dir,
            "obs_id=1349904240,job_type=d,download_type=vis\nobs_id=1349904184,job_type=c\n",
        );
        let client = AsvoClient::new(client_config(&env)).expect("client should be created");

        let replies = client
            .submit_csv_file(&file, &CsvDefaults::default())
            .expect("both jobs should be submitted");

        let job_ids: Vec<u64> = replies.iter().map(|r| r.job_id.get()).collect();
        assert_eq!(job_ids, [11, 12]);
        assert_eq!(first.calls(), 1);
        assert_eq!(second.calls(), 1);
    }

    #[test]
    fn a_job_that_fails_does_not_stop_the_others_and_the_error_has_the_replies() {
        let env = TestEnv::with_session();
        env.server.mock(|when, then| {
            when.method(POST)
                .path("/api/v2/download_vis_job")
                .json_body_includes(serde_json::json!({ "obs_id": 1349904240_u64 }).to_string());
            then.status(400)
                .header("content-type", "application/json")
                .json_body(error_response("VALID_MISSING_FIELD", "Bad obsid"));
        });
        let ok = mock_submit(&env, "/api/v2/download_vis_job", 1349904184, 22);
        let dir = TempDir::new().expect("could not create a temporary directory");
        let file = csv_file(
            &dir,
            "obs_id=1349904240,job_type=d,download_type=vis\nobs_id=1349904184,job_type=d,download_type=vis\n",
        );
        let client = AsvoClient::new(client_config(&env)).expect("client should be created");

        let err = client
            .submit_csv_file(&file, &CsvDefaults::default())
            .expect_err("the first job fails");

        assert_eq!(ok.calls(), 1);
        let CsvSubmitError::Failed {
            submitted,
            failures,
        } = &err
        else {
            panic!("expected Failed, got {err:?}");
        };
        assert_eq!(submitted.len(), 1);
        assert_eq!(submitted[0].job_id.get(), 22);
        assert_eq!(failures.len(), 1);
        assert_eq!(
            (failures[0].line, failures[0].obs_id.get()),
            (1, 1349904240)
        );
        let message = err.to_string();
        assert!(
            message.starts_with("1 of 2 job submissions failed\n  line 1: Obs ID 1349904240: "),
            "{message}"
        );
    }

    #[test]
    fn a_bad_line_submits_nothing() {
        let env = TestEnv::with_session();
        let any = env.server.mock(|when, then| {
            when.any_request();
            then.status(500);
        });
        let dir = TempDir::new().expect("could not create a temporary directory");
        let file = csv_file(
            &dir,
            "obs_id=1349904240,job_type=d,download_type=vis\nobs_id=1349904184,job_type=x\n",
        );
        let client = AsvoClient::new(client_config(&env)).expect("client should be created");

        let err = client
            .submit_csv_file(&file, &CsvDefaults::default())
            .expect_err("line 2 is bad");

        assert!(
            matches!(&err, CsvSubmitError::Invalid(e) if e.errors.len() == 1 && e.errors[0].line == 2),
            "{err:?}"
        );
        assert_eq!(any.calls(), 0);
    }

    #[test]
    fn a_file_that_cannot_be_read_is_an_error_that_names_it() {
        let env = TestEnv::with_session();
        let client = AsvoClient::new(client_config(&env)).expect("client should be created");

        let err = client
            .submit_csv_file(Path::new("/no/such/dir/jobs.csv"), &CsvDefaults::default())
            .expect_err("the file is missing");

        assert!(matches!(err, CsvSubmitError::Read { .. }));
        assert!(
            err.to_string()
                .starts_with("Could not read the CSV file /no/such/dir/jobs.csv: "),
            "{err}"
        );
    }

    #[test]
    fn a_file_with_no_job_gives_no_reply() {
        let env = TestEnv::with_session();
        let dir = TempDir::new().expect("could not create a temporary directory");
        let file = csv_file(&dir, "# nothing\n");
        let client = AsvoClient::new(client_config(&env)).expect("client should be created");

        let replies = client
            .submit_csv_file(&file, &CsvDefaults::default())
            .expect("an empty file is not an error");

        assert!(replies.is_empty());
    }
}
