// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Tests for [`super::Obsid`].

use super::*;
use std::mem::discriminant;

#[test]
fn validation_works() {
    assert!(Obsid::validate(1065880128).is_ok());
}

#[test]
fn validation_fails_too_small() {
    assert!(Obsid::validate(106588012).is_err());
}

#[test]
fn validation_fails_too_big() {
    assert!(Obsid::validate(10658801288).is_err());
}

#[test]
fn batch_spaces() {
    let result = Obsid::from_string("1061311664 1061311784 1061312032");
    assert!(result.is_ok());
    let obsids = result.unwrap();
    assert_eq!(obsids[0], Obsid(1061311664));
    assert_eq!(obsids[1], Obsid(1061311784));
    assert_eq!(obsids[2], Obsid(1061312032));
}

#[test]
fn batch_lines() {
    let result = Obsid::from_string("1061311664\n1061311784\n1061312032");
    assert!(result.is_ok());
    let obsids = result.unwrap();
    assert_eq!(obsids[0], Obsid(1061311664));
    assert_eq!(obsids[1], Obsid(1061311784));
    assert_eq!(obsids[2], Obsid(1061312032));
}

#[test]
fn batch_mix() {
    let result = Obsid::from_string("1061311664 1061311784 \n 1061312032");
    assert!(result.is_ok());
    let obsids = result.unwrap();
    assert_eq!(obsids[0], Obsid(1061311664));
    assert_eq!(obsids[1], Obsid(1061311784));
    assert_eq!(obsids[2], Obsid(1061312032));
}

#[test]
fn batch_fail() {
    // Last int is too small.
    let result = Obsid::from_string("1061311664 1061311784 \n 106131203");
    assert!(result.is_err());
    assert_eq!(
        // `discriminant` allows comparison of enum variants. Here, we
        // verify that the error's enum variant is `WrongNumDigits`. The
        // data in the variant is ignored.
        discriminant(&result.unwrap_err()),
        discriminant(&ObsidError::WrongNumDigits(0))
    );
}

/// A dummy function to return a `ParseIntError`.
fn parse_int_error() -> ParseIntError {
    "5.1".parse::<u64>().unwrap_err()
}

#[test]
fn batch_fail_float() {
    let result = Obsid::from_string("1061311.664");
    assert!(result.is_err());
    assert_eq!(
        discriminant(&result.unwrap_err()),
        // The specific ParseIntError error doesn't matter.
        discriminant(&ObsidError::Parse(parse_int_error()))
    );
}
