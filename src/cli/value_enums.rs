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
//! The generated types have no list of their variants, so each type is
//! registered below with `schema_enum!`. The macro also checks, at compile
//! time, that the list has every variant: when the schema gets a new value
//! and `openapi.rs` is regenerated, the build fails here until the new
//! variant is added.

use std::ffi::OsStr;
use std::fmt::Display;
use std::marker::PhantomData;
use std::str::FromStr;

use clap::builder::{PossibleValue, TypedValueParser};
use clap::{Arg, Command};

use crate::asvo::apiv2::openapi::{
    Centre, Delivery, DeliveryFormat, Output, OutputMode, Polarization, Weighting,
};

/// An enum of the OpenAPI schema, with the list of all its values.
pub trait SchemaEnum: Copy + Display + FromStr + Send + Sync + 'static {
    /// Every variant, in the order that `--help` lists them.
    const VARIANTS: &'static [Self];
}

/// Implements [`SchemaEnum`] for a generated enum.
///
/// The variants are listed in alphabetical order of their API value, which
/// is the order of the Python `giant-squid` command. The `match` inside the
/// `const` fails to compile when the list lacks a variant of the type.
macro_rules! schema_enum {
    ($ty:ty, [$($variant:path),+ $(,)?]) => {
        impl SchemaEnum for $ty {
            const VARIANTS: &'static [Self] = &[$($variant),+];
        }

        const _: fn($ty) = |value| match value {
            $($variant => (),)+
        };
    };
}

schema_enum!(Centre, [Centre::Custom, Centre::Phase, Centre::Pointing]);
schema_enum!(
    Delivery,
    [Delivery::Acacia, Delivery::Dug, Delivery::Scratch]
);
schema_enum!(DeliveryFormat, [DeliveryFormat::Files, DeliveryFormat::Tar]);
schema_enum!(Output, [Output::Ms, Output::Uvfits]);
schema_enum!(
    OutputMode,
    [OutputMode::AllFiles, OutputMode::AllFits, OutputMode::Fits]
);
schema_enum!(
    Polarization,
    [Polarization::Xx, Polarization::Xxyy, Polarization::Yy]
);
schema_enum!(
    Weighting,
    [Weighting::Briggs, Weighting::Natural, Weighting::Uniform]
);

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

/// The API values of `T`, as text for a message.
fn allowed_values<T: SchemaEnum>() -> String {
    T::VARIANTS
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
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
            T::VARIANTS
                .iter()
                .map(|variant| PossibleValue::new(variant.to_string())),
        ))
    }
}

#[cfg(test)]
mod test;
