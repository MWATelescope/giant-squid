//! IDs on standard input: the text parser and the choice of what to read.

use std::io::Read;

use serde_json::json;

use super::{parse_ids_from_text, resolve_ids_from, ResolvedIds};
use crate::helpers::ParseError;
use crate::mwa_asvo::AsvoJobId;
use crate::obs_id::ObsId;

const OBS_ID: u64 = 1065880128;

fn job_id(id: u64) -> AsvoJobId {
    AsvoJobId::new(id).expect("a Job ID is more than 0")
}

fn obs_id(id: u64) -> ObsId {
    ObsId::validate(id).expect("a valid Obs ID")
}

/// Standard input that fails the test if it is read.
struct MustNotBeRead;

impl Read for MustNotBeRead {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        panic!("standard input must not be read");
    }
}

/// Standard input that holds `text`.
fn stdin(text: &str) -> &[u8] {
    text.as_bytes()
}

fn args(texts: &[&str]) -> Vec<String> {
    texts.iter().map(ToString::to_string).collect()
}

// ---------------------------------------------------------------------------
// The text
// ---------------------------------------------------------------------------

#[test]
fn a_byte_order_mark_at_the_start_is_not_part_of_the_text() {
    let ids = parse_ids_from_text("\u{feff}101\n102\n").expect("valid");
    assert_eq!(ids.job_ids, [job_id(101), job_id(102)]);

    let ids = parse_ids_from_text("\u{feff}{\"7\": {}}").expect("valid");
    assert_eq!(ids.job_ids, [job_id(7)]);
}

#[test]
fn ids_on_lines_are_job_ids_and_obs_ids() {
    let ids = parse_ids_from_text(&format!("101\n102\r\n  {OBS_ID}  \n\n")).expect("valid");

    assert_eq!(ids.job_ids, [job_id(101), job_id(102)]);
    assert_eq!(ids.obs_ids, [obs_id(OBS_ID)]);
}

#[test]
fn text_with_no_id_gives_no_id() {
    for text in ["", "  \n\n \t"] {
        let ids = parse_ids_from_text(text).expect("valid");

        assert!(ids.job_ids.is_empty() && ids.obs_ids.is_empty(), "{text:?}");
    }
}

#[test]
fn the_json_of_list_has_the_job_ids_as_keys_in_numeric_order() {
    let text =
        json!({"100": {"id": 100}, "9": {"id": 9}, "10": {"id": 10, "obs_id": OBS_ID}}).to_string();

    let ids = parse_ids_from_text(&text).expect("valid");

    assert_eq!(ids.job_ids, [job_id(9), job_id(10), job_id(100)]);
    assert!(ids.obs_ids.is_empty(), "the Obs IDs of a list are not used");
}

#[test]
fn the_legacy_json_of_list_is_read_too() {
    let text = json!({"101": {"jobId": 101, "obsid": OBS_ID}}).to_string();

    let ids = parse_ids_from_text(&format!("\n  {text}\n")).expect("valid");

    assert_eq!(ids.job_ids, [job_id(101)]);
}

#[test]
fn the_json_of_a_list_with_no_job_gives_no_id() {
    let ids = parse_ids_from_text("{}\n").expect("valid");

    assert!(ids.job_ids.is_empty() && ids.obs_ids.is_empty());
}

#[test]
fn the_error_line_of_a_failed_command_is_an_error_with_its_message() {
    let text = json!({"status": "failed", "error_code": "AUTH", "message": "Bad key"}).to_string();

    let err = parse_ids_from_text(&text).expect_err("not a list");

    assert!(
        matches!(&err, ParseError::StdinError { message } if message == "Bad key"),
        "{err:?}"
    );
    assert_eq!(
        err.to_string(),
        "Standard input has an error, not jobs: Bad key"
    );
}

#[test]
fn json_that_is_not_a_job_list_is_an_error() {
    for text in [
        r#"{"foo": {"id": 1}}"#,
        r#"{"5": 1}"#,
        r#"{"0": {"id": 0}}"#,
        r#"{"5": {"id": 5}"#,
        r#"{"job_id": 5, "message": "ok", "status": "success"}"#,
    ] {
        let err = parse_ids_from_text(text).expect_err(text);

        assert!(
            matches!(err, ParseError::StdinJson { .. }),
            "{text}: {err:?}"
        );
        assert!(err.to_string().contains("giant-squid list --json"), "{err}");
    }
}

#[test]
fn the_table_of_list_is_an_error_that_names_the_options_to_use() {
    let table = "+--------+------------+\n| Job ID | Obs ID     |\n";

    let err = parse_ids_from_text(table).expect_err("a table is not IDs");

    assert!(
        matches!(&err, ParseError::InsideStdin { text } if text == "+--------+------------+"),
        "{err:?}"
    );
    let message = err.to_string();
    assert!(message.contains("giant-squid list --ids"), "{message}");
    assert!(message.contains("giant-squid list --json"), "{message}");
}

#[test]
fn a_zero_is_not_a_job_id() {
    assert!(matches!(
        parse_ids_from_text("5\n0\n"),
        Err(ParseError::ZeroJobId)
    ));
}

// ---------------------------------------------------------------------------
// What is read
// ---------------------------------------------------------------------------

#[test]
fn an_id_that_is_given_more_than_once_is_used_once_at_its_first_place() {
    let obs = OBS_ID.to_string();
    let ids = resolve_ids_from(
        &args(&["9", "5", "9", &obs, &obs]),
        &mut MustNotBeRead,
        false,
    )
    .expect("valid");

    assert_eq!(ids.job_ids, [job_id(9), job_id(5)]);
    assert_eq!(ids.obs_ids, [obs_id(OBS_ID)]);
}

#[test]
fn an_id_on_standard_input_that_an_argument_repeats_is_used_once() {
    let ids = resolve_ids_from(&args(&["5", "-"]), &mut stdin("7\n5\n7\n"), false).expect("valid");

    assert_eq!(ids.job_ids, [job_id(5), job_id(7)]);
}

#[test]
fn arguments_alone_do_not_read_standard_input() {
    for is_piped in [true, false] {
        let ids = resolve_ids_from(
            &args(&["12", &OBS_ID.to_string()]),
            &mut MustNotBeRead,
            is_piped,
        )
        .expect("valid");

        assert_eq!(ids.job_ids, [job_id(12)]);
        assert_eq!(ids.obs_ids, [obs_id(OBS_ID)]);
        assert!(!ids.from_stdin);
    }
}

#[test]
fn with_no_argument_a_pipe_or_a_file_is_read() {
    let ids = resolve_ids_from(&[], &mut stdin("7\n8\n"), true).expect("valid");

    assert_eq!(ids.job_ids, [job_id(7), job_id(8)]);
    assert!(ids.from_stdin);
}

#[test]
fn with_no_argument_a_terminal_or_dev_null_is_not_read() {
    let ids = resolve_ids_from(&[], &mut MustNotBeRead, false).expect("valid");

    assert_eq!(ids, ResolvedIds::default());
    assert!(ids.is_empty());
    assert!(!ids.is_empty_input(), "no input and no ID is a mistake");
}

#[test]
fn a_dash_reads_standard_input_also_in_a_terminal() {
    let ids = resolve_ids_from(&args(&["-"]), &mut stdin("7\n"), false).expect("valid");

    assert_eq!(ids.job_ids, [job_id(7)]);
    assert!(ids.from_stdin);
}

#[test]
fn a_dash_adds_standard_input_to_the_other_arguments() {
    let ids =
        resolve_ids_from(&args(&["5", "-", "6", "-"]), &mut stdin("7\n"), true).expect("valid");

    assert_eq!(ids.job_ids, [job_id(5), job_id(6), job_id(7)]);
}

#[test]
fn a_pipe_with_no_id_is_nothing_to_do() {
    // `list --json` prints `{}`, and `list --ids` prints nothing, for a list
    // with no job.
    for text in ["{}", "", "\n"] {
        let ids = resolve_ids_from(&[], &mut stdin(text), true).expect("valid");

        assert!(ids.is_empty_input(), "{text:?}");
        assert!(ids.from_stdin, "{text:?}");
    }
}

#[test]
fn an_empty_pipe_with_other_ids_is_not_empty() {
    let ids = resolve_ids_from(&args(&["5", "-"]), &mut stdin("{}"), true).expect("valid");

    assert!(!ids.is_empty());
    assert!(!ids.is_empty_input());
}

#[test]
fn a_bad_argument_is_an_error_before_standard_input_is_read() {
    let result = resolve_ids_from(&args(&["no_such_file.txt", "-"]), &mut MustNotBeRead, false);

    assert!(matches!(result, Err(ParseError::IO { .. })), "{result:?}");
}

#[test]
fn standard_input_that_is_not_text_is_an_error() {
    let mut bytes: &[u8] = &[0xff, 0xfe, 0xfd];

    let err = resolve_ids_from(&[], &mut bytes, true).expect_err("not UTF-8");

    assert!(matches!(err, ParseError::StdinIO { .. }), "{err:?}");
}

#[test]
fn job_ids_only_refuses_an_obs_id() {
    let ids = resolve_ids_from(&[], &mut stdin(&format!("5\n{OBS_ID}\n")), true).expect("valid");

    let err = ids.into_job_ids().expect_err("an Obs ID");

    assert!(matches!(err, ParseError::ObsIdsGiven { .. }), "{err:?}");
}
