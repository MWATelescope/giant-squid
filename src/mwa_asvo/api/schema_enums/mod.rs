// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The enums of the OpenAPI schema that the library, the CLI and the Python
//! module use, each with the list of its variants, in one place.
//!
//! The generated types have no list of their variants. `for_each_schema_enum!`
//! lists them once, in the schema's order, and the CLI (`--help` and its
//! value parser) and the Python classes are made from that list. The list
//! also gives each enum its Python class name and docstring. Each list is
//! checked at compile time: when the schema gets a new value and `openapi.rs`
//! is regenerated, the build fails here until the new variant is added.

use std::fmt::Display;
use std::str::FromStr;

use super::openapi::{
    Centre, Delivery, DeliveryFormat, JobState, Output, OutputMode, Polarization, Status, Type,
    Weighting,
};

/// An enum of the OpenAPI schema, with the list of all its values.
pub trait SchemaEnum: Copy + Display + FromStr + PartialEq + Send + Sync + 'static {
    /// Every variant, in the schema's order.
    const VARIANTS: &'static [Self];

    /// The API values of the variants (for example `completed`), in the
    /// schema's order.
    fn names() -> Vec<String> {
        Self::VARIANTS.iter().map(ToString::to_string).collect()
    }

    /// The variant whose API value is `text`, as a user types it: the case,
    /// spaces, hyphens and underscores do not matter, so `WAIT-CAL` and
    /// `waitcal` are the same job state. (The schema's own `FromStr` accepts
    /// only the exact value.) `None` for any other text.
    fn from_name(text: &str) -> Option<Self> {
        let wanted = sanitize_identifier(text);
        Self::VARIANTS
            .iter()
            .find(|variant| sanitize_identifier(&variant.to_string()) == wanted)
            .copied()
    }
}

/// The characters that do not matter in a name, as well as the case.
const NAME_SEPARATORS: [char; 3] = [' ', '-', '_'];

/// `s` in lower case, without spaces, hyphens and underscores: the form in
/// which two names of a value are compared (see [`SchemaEnum::from_name`]).
pub(crate) fn sanitize_identifier(s: &str) -> String {
    let mut sanitized = s.to_lowercase();
    sanitized.retain(|c| !NAME_SEPARATORS.contains(&c));
    sanitized
}

/// Call the macro `$callback` once for each schema enum, with: the type, the
/// name of its Python class in Rust and in Python, the Python docstring, and
/// the variants in the schema's order.
macro_rules! for_each_schema_enum {
    ($callback:ident) => {
        $callback!(
            Delivery,
            PyDelivery,
            "Delivery",
            "Where the MWA ASVO delivers a job's files: the OpenAPI schema's\n\
             `Delivery`, the `delivery` argument of the submit methods. `str()` is\n\
             the API value.",
            [Acacia, Scratch, Dug]
        );
        $callback!(
            Type,
            PyType,
            "Type",
            "Where a job's file is delivered: the OpenAPI schema's `Type`, the type\n\
             of `JobFile.type`. `str()` is the API value.",
            [Acacia, Scratch, Dug]
        );
        $callback!(
            DeliveryFormat,
            PyDeliveryFormat,
            "DeliveryFormat",
            "How the MWA ASVO packages a job's files: one tar file, or separate\n\
             files. `str()` is the API value.",
            [Tar, Files]
        );
        $callback!(
            Output,
            PyOutput,
            "Output",
            "The format of a conversion job's output. `str()` is the API value.",
            [Ms, Uvfits]
        );
        $callback!(
            Centre,
            PyCentre,
            "Centre",
            "Where to put the phase centre of a conversion job or an imaging job.\n\
             `str()` is the API value.",
            [Phase, Pointing, Custom]
        );
        $callback!(
            OutputMode,
            PyOutputMode,
            "OutputMode",
            "The products an imaging job returns. `str()` is the API value.",
            [Fits, AllFits, AllFiles]
        );
        $callback!(
            Weighting,
            PyWeighting,
            "Weighting",
            "The WSClean weighting scheme of an imaging job. `str()` is the API\n\
             value.",
            [Briggs, Uniform, Natural]
        );
        $callback!(
            Polarization,
            PyPolarization,
            "Polarization",
            "The polarisation an imaging job images. `str()` is the API value.",
            [Xx, Yy, Xxyy]
        );
        $callback!(
            JobState,
            PyJobState,
            "JobState",
            "The state of an MWA ASVO job: the OpenAPI schema's `JobState`. `str()`\n\
             is the API value (for example \"completed\"). For `Error`, the message is\n\
             in `AsvoJob.error_text`.",
            [
                Preparing,
                Queued,
                Waitcal,
                Staging,
                Staged,
                Downloading,
                Preprocessing,
                Imaging,
                Delivering,
                Completed,
                Error,
                Cancelled
            ]
        );
        $callback!(
            Status,
            PyStatus,
            "Status",
            "The `status` of the MWA ASVO's reply to a submission or a cancellation:\n\
             the OpenAPI schema's `Status`. `str()` is the API value (\"success\" or\n\
             \"failed\"). It describes the reply, like `message`; it is for display\n\
             only. Success or failure of a call is decided by the HTTP status, so a\n\
             call that fails raises `AsvoApiError`.",
            [Success, Failed]
        );
    };
}
// Only the Python module uses the list from outside this module.
#[cfg(feature = "python")]
pub(crate) use for_each_schema_enum;

/// Implement [`SchemaEnum`] for one enum. The `match` fails to compile when
/// the list lacks a variant of the type.
macro_rules! impl_schema_enum {
    ($ty:ident, $py:ident, $name:literal, $doc:literal, [$($variant:ident),+ $(,)?]) => {
        impl SchemaEnum for $ty {
            const VARIANTS: &'static [Self] = &[$($ty::$variant),+];
        }

        const _: fn($ty) = |value| match value {
            $($ty::$variant => (),)+
        };
    };
}

for_each_schema_enum!(impl_schema_enum);

#[cfg(test)]
mod tests;
