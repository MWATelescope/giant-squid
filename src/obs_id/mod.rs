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
pub struct ObsId(u64);

impl ObsId {
    /// Given a [u64], return it as an MWA [`ObsId`] if it is valid.
    pub fn validate(o: u64) -> Result<ObsId, ObsIdError> {
        // Valid obsids are between 1e9 and 1e10.
        if o >= 1e9 as u64 && o < 1e10 as u64 {
            Ok(ObsId(o))
        } else {
            Err(ObsIdError::WrongNumDigits(o))
        }
    }

    /// Convert a string of whitespace-delimited (e.g. spaces, tabs, newlines)
    /// integers to a [`Vec<ObsId>`]. If any of the integers are invalid as
    /// obsids, an error is returned.
    pub fn from_string(s: &str) -> Result<Vec<ObsId>, ObsIdError> {
        s.split_whitespace().map(|i| i.parse()).collect()
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

/// An obsid as the OpenAPI schema types `obs_id`: an `i64`. A valid obsid
/// is below 1e10, so it always fits.
impl From<ObsId> for i64 {
    fn from(o: ObsId) -> i64 {
        i64::try_from(o.0).expect("a valid obsid is below 1e10, so it fits in an i64")
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
    /// If an int doesn't have 10 digits, it's not a valid obsid.
    #[error("'{0}' doesn't have 10 digits and cannot be used as an MWA obsid")]
    WrongNumDigits(u64),

    /// An error associated with string parsing.
    #[error("{0}")]
    Parse(#[from] ParseIntError),
}

#[cfg(test)]
mod tests;
