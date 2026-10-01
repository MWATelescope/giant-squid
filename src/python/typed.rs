// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Newtypes that give the Python stubs a precise type.
//!
//! pyo3-stub-gen writes `typing.Any` (or no type) for some pyo3 types, for
//! example `Py<PyAny>` and `Bound<PyIterator>`. A `#[gen_stub(...)]`
//! override attribute cannot fix this here: the stub annotations are
//! behind `cfg_attr(feature = "python-stubgen", ...)`, and an override
//! inside `cfg_attr` is not seen, while a plain one does not compile without
//! the feature. So each such argument or return value has a newtype, and
//! the newtype has a `PyStubType` with the Python type. In Python each
//! value is the object it wraps; nothing changes at run time.

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyIterator};

/// A stub type with the Python text `name`, which needs `imports`. The
/// names of the classes of this module are used without an import.
#[cfg(feature = "python-stubgen")]
fn stub_type(name: &str, imports: &[&str]) -> pyo3_stub_gen::TypeInfo {
    pyo3_stub_gen::TypeInfo {
        name: name.to_string(),
        source_module: None,
        import: imports.iter().map(|&i| i.into()).collect(),
        type_refs: std::collections::HashMap::new(),
    }
}

/// The value that `AsvoJobVec.__iter__` returns: an iterator over a list of
/// `AsvoJob`. In Python it is the list iterator.
pub struct JobIterator<'py>(pub Bound<'py, PyIterator>);

impl<'py> IntoPyObject<'py> for JobIterator<'py> {
    type Target = PyIterator;
    type Output = Bound<'py, PyIterator>;
    type Error = std::convert::Infallible;

    fn into_pyobject(self, _py: Python<'py>) -> Result<Self::Output, Self::Error> {
        Ok(self.0)
    }
}

#[cfg(feature = "python-stubgen")]
impl pyo3_stub_gen::PyStubType for JobIterator<'_> {
    fn type_output() -> pyo3_stub_gen::TypeInfo {
        stub_type("typing.Iterator[AsvoJob]", &["typing"])
    }
}

/// A `dict` with `str` keys, made from JSON: a request body, or a job's
/// parameters.
pub struct JsonDict<'py>(pub Bound<'py, PyDict>);

impl<'py> IntoPyObject<'py> for JsonDict<'py> {
    type Target = PyDict;
    type Output = Bound<'py, PyDict>;
    type Error = std::convert::Infallible;

    fn into_pyobject(self, _py: Python<'py>) -> Result<Self::Output, Self::Error> {
        Ok(self.0)
    }
}

#[cfg(feature = "python-stubgen")]
impl pyo3_stub_gen::PyStubType for JsonDict<'_> {
    fn type_output() -> pyo3_stub_gen::TypeInfo {
        stub_type(
            "builtins.dict[builtins.str, typing.Any]",
            &["builtins", "typing"],
        )
    }
}

/// The `progress` argument of the download methods: a function that takes
/// a `DownloadProgress`. Any object is taken, as before; a call to an object
/// that is not callable raises when the first event is sent.
pub struct ProgressCallback(pub Py<PyAny>);

impl<'a, 'py> FromPyObject<'a, 'py> for ProgressCallback {
    type Error = PyErr;

    fn extract(obj: Borrowed<'a, 'py, PyAny>) -> PyResult<Self> {
        Ok(Self(obj.to_owned().unbind()))
    }
}

#[cfg(feature = "python-stubgen")]
impl pyo3_stub_gen::PyStubType for ProgressCallback {
    fn type_output() -> pyo3_stub_gen::TypeInfo {
        stub_type(
            "collections.abc.Callable[[DownloadProgress], builtins.object]",
            &["builtins", "collections.abc"],
        )
    }
}
