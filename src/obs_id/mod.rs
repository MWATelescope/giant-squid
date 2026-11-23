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
#[derive(Serialize, PartialEq, Eq, Hash, Clone, Copy)]
pub struct ObsId(u64);

impl ObsId {
    /// The smallest valid Obs ID: the MWA ASVO schema's `obs_id` minimum. A
    /// test checks that it is the schema's.
    pub const MIN: u64 = 1_056_643_216;

    /// The largest valid Obs ID: the largest 10-digit number.
    pub const MAX: u64 = 9_999_999_999;

    /// The smallest number that has 10 digits. A number with 10 digits is
    /// read as an Obs ID, and any other number as a Job ID
    /// ([`ObsId::has_obs_id_digits`]), so a 10-digit number below
    /// [`ObsId::MIN`] is an Obs ID that is not valid, not a Job ID.
    const SMALLEST_TEN_DIGITS: u64 = 1_000_000_000;

    /// Whether `o` has 10 digits, which is the form of an Obs ID. It is not
    /// valid if it is below [`ObsId::MIN`] ([`ObsId::validate`]).
    pub(crate) fn has_obs_id_digits(o: u64) -> bool {
        (Self::SMALLEST_TEN_DIGITS..=Self::MAX).contains(&o)
    }

    /// Given a [u64], return it as an MWA [`ObsId`] if it is valid: if it is
    /// from [`ObsId::MIN`] to [`ObsId::MAX`].
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
    /// The number is outside [`ObsId::MIN`] to [`ObsId::MAX`], so it is not a
    /// valid Obs ID. (The name is from when the range was all the 10-digit
    /// numbers.)
    #[error(
        "'{0}' is not a valid MWA Obs ID: it must be from {min} to {max}",
        min = ObsId::MIN,
        max = ObsId::MAX
    )]
    WrongNumDigits(u64),

    /// An error associated with string parsing.
    #[error("{0}")]
    Parse(#[from] ParseIntError),
}

#[cfg(test)]
mod tests;
