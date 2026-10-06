// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::*;

const TEST_OBS_ID: u64 = 1_384_357_952;

fn job_id(id: u64) -> AsvoJobId {
    AsvoJobId::new(id).expect("a job ID is not zero")
}

fn obs_id() -> ObsId {
    ObsId::validate(TEST_OBS_ID).expect("a valid obsid")
}

#[test]
fn an_api_error_is_copied_verbatim() {
    let error = AsvoApiError::ApiError {
        error_code: "JOB_NOT_FOUND".to_string(),
        message: "No such job".to_string(),
        detail: Some("detail text".to_string()),
        suggestion: Some("suggestion text".to_string()),
        field_errors: vec![FieldError {
            field: "job_id".to_string(),
            message: "bad".to_string(),
        }],
        request_id: Some("abc".to_string()),
    };

    let resp = error.error_response();

    assert_eq!(resp.error_code, "JOB_NOT_FOUND");
    assert_eq!(resp.message, "No such job");
    assert_eq!(resp.detail.as_deref(), Some("detail text"));
    assert_eq!(resp.suggestion.as_deref(), Some("suggestion text"));
    assert_eq!(resp.request_id.as_deref(), Some("abc"));
    assert_eq!(resp.field_errors.map(|f| f.len()), Some(1));
}

#[test]
fn an_api_error_without_field_errors_has_no_field_errors_key() {
    let error = AsvoApiError::ApiError {
        error_code: "AUTH_REQUIRED".to_string(),
        message: "Log in".to_string(),
        detail: None,
        suggestion: None,
        field_errors: vec![],
        request_id: None,
    };

    let json = serde_json::to_value(error.error_response()).unwrap();

    assert_eq!(
        json,
        serde_json::json!({"error_code": "AUTH_REQUIRED", "message": "Log in"})
    );
}

#[test]
fn a_bad_status_is_the_http_code_with_the_body_as_the_detail() {
    let body = "<html><body>503</body></html>\n";
    let error = AsvoApiError::BadStatus {
        code: reqwest::StatusCode::SERVICE_UNAVAILABLE,
        message: body.to_string(),
    };

    let resp = error.error_response();

    assert_eq!(resp.error_code, "HTTP_503");
    assert_eq!(resp.message, "Service Unavailable");
    assert_eq!(resp.detail.as_deref(), Some(body));
}

#[test]
fn a_failed_login_with_an_error_response_body_is_copied_verbatim() {
    let error = AsvoApiError::AuthenticationFailed {
        message: r#"{"error_code":"AUTH_INVALID_KEY","message":"Bad API key"}"#.to_string(),
    };

    let resp = error.error_response();

    assert_eq!(resp.error_code, "AUTH_INVALID_KEY");
    assert_eq!(resp.message, "Bad API key");
}

#[test]
fn a_failed_login_with_another_body_is_authentication_failed() {
    let error = AsvoApiError::AuthenticationFailed {
        message: "no".to_string(),
    };

    let resp = error.error_response();

    assert_eq!(resp.error_code, ERROR_CODE_AUTHENTICATION_FAILED);
    assert_eq!(resp.message, error.to_string());
}

#[test]
fn an_invalid_parameter_has_a_field_error() {
    let error = AsvoApiError::InvalidParameter {
        name: "days",
        message: "must be between 1 and 30 (got 31)".to_string(),
    };

    let resp = error.error_response();

    assert_eq!(resp.error_code, ERROR_CODE_INVALID_PARAMETER);
    assert_eq!(resp.message, error.to_string());
    assert_eq!(
        resp.field_errors,
        Some(vec![FieldError {
            field: "days".to_string(),
            message: "must be between 1 and 30 (got 31)".to_string(),
        }])
    );
}

#[test]
fn a_missing_api_key_has_its_code() {
    let error = AsvoApiError::MissingAuthKey { variable: None };
    assert_eq!(
        error.error_response().error_code,
        ERROR_CODE_MISSING_API_KEY
    );
}

#[test]
fn a_download_error_from_the_api_is_the_api_error() {
    let error = AsvoError::AsvoApi(AsvoApiError::BadStatus {
        code: reqwest::StatusCode::BAD_GATEWAY,
        message: String::new(),
    });

    assert_eq!(error.error_response().error_code, "HTTP_502");
}

#[test]
fn a_missing_job_uses_the_api_code_and_names_the_job() {
    let error = AsvoError::NoAsvoJob(job_id(12345));

    assert_eq!(error.error_response().error_code, ERROR_CODE_JOB_NOT_FOUND);
    assert_eq!(error.error_response().message, error.to_string());
    assert_eq!(error.job_id(), Some(job_id(12345)));
    assert_eq!(error.obs_id(), None);
}

#[test]
fn a_failed_job_has_the_job_error_code_as_the_detail() {
    let error = AsvoError::JobFailed {
        job_id: job_id(12345),
        obs_id: obs_id(),
        error: "it broke".to_string(),
        error_code: Some(7),
    };

    let resp = error.error_response();

    assert_eq!(resp.error_code, ERROR_CODE_JOB_FAILED);
    assert_eq!(resp.detail.as_deref(), Some("7"));
    assert_eq!(error.job_id(), Some(job_id(12345)));
    assert_eq!(error.obs_id(), Some(obs_id()));
}

#[test]
fn a_missing_obs_id_names_the_obs_id() {
    let error = AsvoError::NoObsId(obs_id());

    assert_eq!(
        error.error_response().error_code,
        ERROR_CODE_OBS_ID_NOT_FOUND
    );
    assert_eq!(error.obs_id(), Some(obs_id()));
    assert_eq!(error.job_id(), None);
}

#[test]
fn a_download_http_error_is_the_http_code_with_the_body_as_the_detail() {
    let error = AsvoError::HttpError {
        status: 500,
        message: "body".to_string(),
    };

    let resp = error.error_response();

    assert_eq!(resp.error_code, "HTTP_500");
    assert_eq!(resp.detail.as_deref(), Some("body"));
}

#[test]
fn a_parse_error_is_an_invalid_argument() {
    let error = ParseError::NoObsIds;

    let resp = error.error_response();

    assert_eq!(resp.error_code, ERROR_CODE_INVALID_ARGUMENT);
    assert_eq!(resp.message, error.to_string());
}
