// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The clap value parser for every option that takes a named value: the
//! enums of the OpenAPI schema (`Delivery`, `JobState`, `Polarization` and
//! so on) and the job types.
//!
//! The parser has two jobs:
//!
//! - It lists the allowed values in `--help` (`[possible values: ...]`).
//! - It accepts a value as a user types it: the case, spaces, hyphens and
//!   underscores do not matter (see [`SchemaEnum::from_name`]). A bad value
//!   is a `ValueValidation` error, and its message names the allowed
//!   values.
//!
//! The values come from the library's list of each enum, so the CLI has no
//! list of its own. Every option lists them in the schema's order.

use std::ffi::OsStr;
use std::marker::PhantomData;

use clap::builder::{PossibleValue, TypedValueParser};
use clap::{Arg, Command};

use crate::mwa_asvo::api::schema_enums::SchemaEnum;
use crate::mwa_asvo::JobType;

/// The clap value parser for a [`NamedValue`].
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

/// A value that the CLI takes by name: a schema enum ([`SchemaEnum`]), or a
/// job type, whose schema type is a number with a name.
pub trait NamedValue: Copy + Send + Sync + 'static {
    /// The names, in the schema's order: the order of `--help` and of the
    /// messages.
    fn value_names() -> Vec<String>;

    /// The value named `text`, where the case, spaces, hyphens and
    /// underscores do not matter, or `None`.
    fn from_value_name(text: &str) -> Option<Self>;
}

impl<T: SchemaEnum> NamedValue for T {
    fn value_names() -> Vec<String> {
        T::names()
    }

    fn from_value_name(text: &str) -> Option<Self> {
        T::from_name(text)
    }
}

impl NamedValue for JobType {
    fn value_names() -> Vec<String> {
        JobType::names()
    }

    fn from_value_name(text: &str) -> Option<Self> {
        JobType::parse_name(text).ok()
    }
}

impl<T: NamedValue> TypedValueParser for SchemaEnumParser<T> {
    type Value = T;

    fn parse_ref(&self, cmd: &Command, arg: Option<&Arg>, value: &OsStr) -> Result<T, clap::Error> {
        // The same parser that clap builds for a `FromStr` type, so the
        // kind of the error is `ValueValidation`.
        let parse = |text: &str| {
            T::from_value_name(text)
                .ok_or_else(|| format!("expected one of: {}", T::value_names().join(", ")))
        };
        parse.parse_ref(cmd, arg, value)
    }

    fn possible_values(&self) -> Option<Box<dyn Iterator<Item = PossibleValue> + '_>> {
        Some(Box::new(
            T::value_names().into_iter().map(PossibleValue::new),
        ))
    }
}

#[cfg(test)]
mod tests;
