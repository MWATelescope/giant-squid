// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Code to handle Obs IDs.

use std::num::ParseIntError;
use std::str::FromStr;

use serde::Serialize;
use thiserror::Error;

/// A newtype representing an MWA observation ID ("Obs ID"). Using this type
/// instead of a [u64] ensures that things work correctly at compile time.
#[derive(Serialize, PartialEq, Eq, Clone, Copy)]
pub struct ObsId(u64);

impl ObsId {
    /// The smallest valid Obs ID: the smallest 10-digit number.
    ///
    /// The MWA ASVO schema's `obs_id` minimum is lower (888888889). A test
    /// checks that the schema's minimum is not above this one.
    pub const MIN: u64 = 1_000_000_000;

    /// The largest valid Obs ID: the largest 10-digit number.
    pub const MAX: u64 = 9_999_999_999;

    /// Given a [u64], return it as an MWA [`ObsId`] if it is valid: if it has
    /// 10 digits ([`ObsId::MIN`] to [`ObsId::MAX`]).
    pub fn validate(o: u64) -> Result<ObsId, ObsIdError> {
        if (Self::MIN..=Self::MAX).contains(&o) {
            Ok(ObsId(o))
        } else {
            Err(ObsIdError::WrongNumDigits(o))
        }
    }

    /// Get the underlying [u64] value.
    pub fn get(&self) -> u64 {
        self.0
    }
}

impl From<ObsId> for u64 {
    fn from(o: ObsId) -> u64 {
        o.0
    }
}

/// An Obs ID as the OpenAPI schema types `obs_id`: an `i64`. A valid Obs ID
/// is below 1e10, so it always fits.
impl From<ObsId> for i64 {
    fn from(o: ObsId) -> i64 {
        i64::try_from(o.0).expect("a valid Obs ID is below 1e10, so it fits in an i64")
    }
}

impl FromStr for ObsId {
    type Err = ObsIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let int: u64 = s.parse()?;
        ObsId::validate(int)
    }
}

impl std::fmt::Display for ObsId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::fmt::Debug for ObsId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Error, Debug)]
pub enum ObsIdError {
    /// If an int doesn't have 10 digits, it's not a valid Obs ID.
    #[error("'{0}' doesn't have 10 digits and cannot be used as an MWA Obs ID")]
    WrongNumDigits(u64),

    /// An error associated with string parsing.
    #[error("{0}")]
    Parse(#[from] ParseIntError),
}

#[cfg(test)]
mod tests;
