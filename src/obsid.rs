// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Code to handle obsids.

use std::num::ParseIntError;
use std::str::FromStr;

use serde::Serialize;
use thiserror::Error;

/// A newtype representing an MWA observation ID ("obsid"). Using this type
/// instead of a [u64] ensures that things work correctly at compile time.
#[derive(Serialize, PartialEq, Eq, Clone, Copy)]
pub struct Obsid(u64);

impl Obsid {
    /// Given a [u64], return it as an MWA [Obsid] if it is valid.
    pub fn validate(o: u64) -> Result<Obsid, ObsidError> {
        // Valid obsids are between 1e9 and 1e10.
        if o >= 1e9 as u64 && o < 1e10 as u64 {
            Ok(Obsid(o))
        } else {
            Err(ObsidError::WrongNumDigits(o))
        }
    }

    /// Convert a string of whitespace-delimited (e.g. spaces, tabs, newlines)
    /// integers to a [Vec<Obsid>]. If any of the integers are invalid as
    /// obsids, an error is returned.
    pub fn from_string(s: &str) -> Result<Vec<Obsid>, ObsidError> {
        s.split_whitespace().map(|i| i.parse()).collect()
    }
}

impl FromStr for Obsid {
    type Err = ObsidError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let int: u64 = s.parse()?;
        Obsid::validate(int)
    }
}

impl std::fmt::Display for Obsid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::fmt::Debug for Obsid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Error, Debug)]
pub enum ObsidError {
    /// If an int doesn't have 10 digits, it's not a valid obsid.
    #[error("'{0}' doesn't have 10 digits and cannot be used as an MWA obsid")]
    WrongNumDigits(u64),

    /// An error associated with string parsing.
    #[error("{0}")]
    Parse(#[from] ParseIntError),
}

#[cfg(test)]
mod tests {
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
}
