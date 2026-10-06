// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A clap value parser for the enums that the OpenAPI schema defines
//! (`Delivery`, `DeliveryFormat`, `Output`, `Centre`, `OutputMode`,
//! `Weighting` and `Polarization`).
//!
//! The parser has two jobs:
//!
//! - It lists the allowed values in `--help` (`[possible values: ...]`).
//! - It accepts exactly what the generated `FromStr` accepts, and a bad
//!   value is a `ValueValidation` error, as it was before. The message now
//!   names the allowed values.
//!
//! The values come from the library's list of each schema enum
//! ([`SchemaEnum`]), so the CLI has no list of its own. The help and the
//! messages list them in alphabetical order of their API value.

use std::ffi::OsStr;
use std::marker::PhantomData;

use clap::builder::{PossibleValue, TypedValueParser};
use clap::{Arg, Command};

pub use crate::mwa_asvo::api::schema_enums::SchemaEnum;

/// The clap value parser for a [`SchemaEnum`].
///
/// Use it as `value_parser = SchemaEnumParser::<Delivery>::new()`.
pub struct SchemaEnumParser<T>(PhantomData<fn() -> T>);

impl<T> SchemaEnumParser<T> {
    /// A parser for `T`.
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T> Default for SchemaEnumParser<T> {
    fn default() -> Self {
        Self::new()
    }
}

// Not derived: a derive would require `T: Clone`, which the parser does not
// need.
impl<T> Clone for SchemaEnumParser<T> {
    fn clone(&self) -> Self {
        Self::new()
    }
}

/// The API values of `T`, in alphabetical order: the order of `--help` and
/// of the messages.
fn sorted_values<T: SchemaEnum>() -> Vec<String> {
    let mut values: Vec<String> = T::VARIANTS.iter().map(ToString::to_string).collect();
    values.sort();
    values
}

/// The API values of `T`, as text for a message.
fn allowed_values<T: SchemaEnum>() -> String {
    sorted_values::<T>().join(", ")
}

impl<T: SchemaEnum> TypedValueParser for SchemaEnumParser<T> {
    type Value = T;

    fn parse_ref(&self, cmd: &Command, arg: Option<&Arg>, value: &OsStr) -> Result<T, clap::Error> {
        // The same parser that clap builds for a `FromStr` type, so the
        // kind of the error is the same as it was: `ValueValidation`.
        let parse = |text: &str| {
            text.parse::<T>()
                .map_err(|_| format!("expected one of: {}", allowed_values::<T>()))
        };
        parse.parse_ref(cmd, arg, value)
    }

    fn possible_values(&self) -> Option<Box<dyn Iterator<Item = PossibleValue> + '_>> {
        Some(Box::new(
            sorted_values::<T>().into_iter().map(PossibleValue::new),
        ))
    }
}

#[cfg(test)]
mod tests;
