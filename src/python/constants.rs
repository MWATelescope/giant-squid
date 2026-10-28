// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The public constants of the Rust library, as variables of the Python
//! module. Each has the name and the value of the Rust constant. A `Duration`
//! is a `float` in seconds; a size is an `int` in bytes.

use pyo3::prelude::*;

/// Make the module variables, and the stub entry of each (with its value).
macro_rules! module_constants {
    ($($name:ident: $ty:ty = $value:expr;)*) => {
        $(
            #[cfg(feature = "python-stubgen")]
            pyo3_stub_gen::module_variable!("mwa_giant_squid", stringify!($name), $ty, $value);
        )*

        /// Add every constant to the module.
        pub(super) fn add(m: &Bound<'_, PyModule>) -> PyResult<()> {
            $(
                let value: $ty = $value;
                m.add(stringify!($name), value)?;
            )*
            Ok(())
        }
    };
}

module_constants! {
    DEFAULT_ASVO_HOST: String = crate::DEFAULT_ASVO_HOST.to_string();
    DEFAULT_API_TIMEOUT: f64 = crate::DEFAULT_API_TIMEOUT.as_secs_f64();
    BYTES_PER_MIB: usize = crate::BYTES_PER_MIB;
    MIN_DOWNLOAD_BUFFER_SIZE: usize = crate::MIN_DOWNLOAD_BUFFER_SIZE;
    DEFAULT_DOWNLOAD_BUFFER_SIZE: usize = crate::DEFAULT_DOWNLOAD_BUFFER_SIZE;
    DEFAULT_DOWNLOAD_RETRY_DURATION: f64 = crate::DEFAULT_DOWNLOAD_RETRY_DURATION.as_secs_f64();
    SIGNED_URL_LIFETIME_DAYS: u32 = crate::SIGNED_URL_LIFETIME_DAYS;
    OBS_ID_HINT: String = crate::OBS_ID_HINT.to_string();
    ENV_MWA_ASVO_HOST: String = crate::ENV_MWA_ASVO_HOST.to_string();
    ENV_MWA_ASVO_API_KEY: String = crate::ENV_MWA_ASVO_API_KEY.to_string();
    ENV_MWA_ASVO_API_TIMEOUT: String = crate::ENV_MWA_ASVO_API_TIMEOUT.to_string();
    ENV_HOME: String = crate::ENV_HOME.to_string();
    ENV_GIANT_SQUID_BUF_SIZE: String = crate::ENV_GIANT_SQUID_BUF_SIZE.to_string();
    ENV_GIANT_SQUID_DOWNLOAD_RETRY_SECS: String = crate::ENV_GIANT_SQUID_DOWNLOAD_RETRY_SECS.to_string();
    ENV_GIANT_SQUID_DELIVERY: String = crate::ENV_GIANT_SQUID_DELIVERY.to_string();
    ENV_GIANT_SQUID_DELIVERY_FORMAT: String = crate::ENV_GIANT_SQUID_DELIVERY_FORMAT.to_string();
}
