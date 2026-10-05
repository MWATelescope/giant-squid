// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Tests of the lists of the schema enums.

use super::*;

/// The values of `T`, as the API writes them.
fn values<T: SchemaEnum>() -> Vec<String> {
    T::VARIANTS.iter().map(ToString::to_string).collect()
}

/// Each list is in the schema's order, and each value parses back to its
/// variant.
#[test]
fn each_list_is_in_the_schema_order_and_parses_back() {
    assert_eq!(values::<Delivery>(), ["acacia", "scratch", "dug"]);
    assert_eq!(values::<Polarization>(), ["XX", "YY", "XXYY"]);
    assert_eq!(
        values::<JobState>().first().map(String::as_str),
        Some("preparing")
    );
    for variant in Centre::VARIANTS {
        assert_eq!(variant.to_string().parse::<Centre>().ok(), Some(*variant));
    }
}
