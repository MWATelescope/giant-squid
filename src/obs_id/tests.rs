// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Tests for [`super::ObsId`].

use super::*;
use std::mem::discriminant;

#[test]
fn validation_works() {
    assert!(ObsId::validate(1065880128).is_ok());
}

#[test]
fn validation_fails_too_small() {
    assert!(ObsId::validate(106588012).is_err());
}

#[test]
fn validation_fails_too_big() {
    assert!(ObsId::validate(10658801288).is_err());
}

/// A string that is a number but not a 10-digit one is `WrongNumDigits`.
#[test]
fn parse_fails_too_small() {
    let result = "106131203".parse::<ObsId>();
    assert_eq!(
        // `discriminant` allows comparison of enum variants. Here, we
        // verify that the error's enum variant is `WrongNumDigits`. The
        // data in the variant is ignored.
        discriminant(&result.unwrap_err()),
        discriminant(&ObsIdError::WrongNumDigits(0))
    );
}

/// A dummy function to return a `ParseIntError`.
fn parse_int_error() -> ParseIntError {
    "5.1".parse::<u64>().unwrap_err()
}

/// A string that is not an integer is `Parse`.
#[test]
fn parse_fails_float() {
    let result = "1061311.664".parse::<ObsId>();
    assert_eq!(
        discriminant(&result.unwrap_err()),
        // The specific ParseIntError error doesn't matter.
        discriminant(&ObsIdError::Parse(parse_int_error()))
    );
}
