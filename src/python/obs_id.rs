// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The Python class `ObsId`, and the arguments that take an Obs ID.
//!
//! `ObsId` wraps the library's [`ObsId`]: an MWA observation ID that is known
//! to be valid (10 digits). PyO3 cannot make a class that extends `int`, so it
//! is not an `int`, but it acts as one where Python needs a number: it has
//! `__int__` and `__index__`, it has the hash of the `int`, and it compares
//! with other `ObsId` and with `int`.
//!
//! An argument that is an Obs ID takes an `int` or an `ObsId`. [`ObsIdArg`]
//! checks it, as the library does ([`ObsId::validate`]). [`RawObsId`] does
//! not: a request body keeps the number that it is given, and the library
//! checks it when the request is made.

use pyo3::basic::CompareOp;
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;

use super::error::obs_id_error;
use crate::obs_id::ObsId;

/// An MWA observation ID that is known to be valid.
///
/// An `ObsId` is not an `int`, but it can be used where a number is needed
/// (`int(obs_id)`, `range`, a list index, a dictionary key) and it is equal
/// to the `int` of the same value. `json.dumps` does not know it: use
/// `int(obs_id)`.
///
/// Every argument of this module that is an Obs ID takes an `int` or an
/// `ObsId`. An Obs ID that is not valid raises `ObsIdError`.
#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(
    frozen,
    skip_from_py_object,
    name = "ObsId",
    module = "mwa_giant_squid"
)]
#[derive(Clone, Copy)]
pub struct PyObsId(pub ObsId);

impl PyObsId {
    /// The value, as the number of the Python `int`.
    fn value(&self) -> u64 {
        self.0.get()
    }

    /// The number that `other` has: an `ObsId` or an `int`, or `None` for
    /// any other object (and for an `int` that is not a `u64`).
    fn number_of(other: &Bound<'_, PyAny>) -> Option<u64> {
        other.extract::<RawObsId>().ok().map(|raw| raw.0)
    }
}

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PyObsId {
    /// The smallest valid Obs ID: the smallest 10-digit number.
    #[classattr]
    const MIN: u64 = ObsId::MIN;

    /// The largest valid Obs ID: the largest 10-digit number.
    #[classattr]
    const MAX: u64 = ObsId::MAX;

    /// Make an `ObsId` from a number, if it is valid: if it has 10 digits.
    ///
    /// Args:
    ///     value: The Obs ID.
    ///
    /// Raises:
    ///     ObsIdError: The number is not a valid Obs ID (kind
    ///         `WrongNumDigits`).
    ///     OverflowError: The number is negative, or too large for 64 bits.
    #[new]
    fn new(value: ObsIdArg) -> Self {
        Self(value.0)
    }

    /// Make an `ObsId` from a number, if it is valid: if it has 10 digits.
    ///
    /// Args:
    ///     o: The Obs ID.
    ///
    /// Raises:
    ///     ObsIdError: The number is not a valid Obs ID (kind
    ///         `WrongNumDigits`).
    ///     OverflowError: The number is negative, or too large for 64 bits.
    #[staticmethod]
    fn validate(o: ObsIdArg) -> Self {
        Self(o.0)
    }

    /// Make an `ObsId` from text, such as `"1065880128"`.
    ///
    /// Args:
    ///     text: The Obs ID as decimal digits.
    ///
    /// Raises:
    ///     ObsIdError: The text is not a number (kind `Parse`), or is not a
    ///         valid Obs ID (kind `WrongNumDigits`).
    #[staticmethod]
    fn from_str(py: Python<'_>, text: &str) -> PyResult<Self> {
        text.parse::<ObsId>()
            .map(Self)
            .map_err(|e| obs_id_error(py, e))
    }

    /// The value, as an `int`.
    fn get(&self) -> u64 {
        self.value()
    }

    fn __int__(&self) -> u64 {
        self.value()
    }

    fn __index__(&self) -> u64 {
        self.value()
    }

    /// The hash of the `int` of the same value, so that an `ObsId` and an
    /// `int` are the same key of a dictionary.
    fn __hash__(&self, py: Python<'_>) -> PyResult<isize> {
        self.value().into_pyobject(py)?.hash()
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        Self::number_of(other).is_some_and(|n| n == self.value())
    }

    fn __lt__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        self.compare(other, CompareOp::Lt)
    }

    fn __le__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        self.compare(other, CompareOp::Le)
    }

    fn __gt__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        self.compare(other, CompareOp::Gt)
    }

    fn __ge__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        self.compare(other, CompareOp::Ge)
    }

    fn __str__(&self) -> String {
        self.0.to_string()
    }

    fn __repr__(&self) -> String {
        format!("ObsId({})", self.0)
    }

    /// The arguments for `copy` and `pickle`.
    fn __getnewargs__(&self) -> (u64,) {
        (self.value(),)
    }
}

impl PyObsId {
    /// Compare with an `ObsId` or an `int`.
    fn compare(&self, other: &Bound<'_, PyAny>, op: CompareOp) -> PyResult<bool> {
        let n = Self::number_of(other).ok_or_else(|| {
            PyTypeError::new_err(format!(
                "cannot compare an ObsId with {}",
                other
                    .get_type()
                    .name()
                    .map_or_else(|_| "this object".to_string(), |n| n.to_string())
            ))
        })?;
        Ok(op.matches(self.value().cmp(&n)))
    }
}

/// An Obs ID that a function takes: an `int` or an `ObsId`. It is not
/// checked: the number is as it was given.
#[derive(Clone, Copy)]
pub struct RawObsId(pub u64);

impl<'a, 'py> FromPyObject<'a, 'py> for RawObsId {
    type Error = PyErr;

    fn extract(obj: Borrowed<'a, 'py, PyAny>) -> PyResult<Self> {
        if let Ok(obs_id) = obj.cast::<PyObsId>() {
            return Ok(Self(obs_id.get().value()));
        }
        obj.extract::<u64>().map(Self)
    }
}

impl<'py> IntoPyObject<'py> for RawObsId {
    type Target = pyo3::types::PyInt;
    type Output = Bound<'py, pyo3::types::PyInt>;
    type Error = std::convert::Infallible;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        self.0.into_pyobject(py)
    }
}

#[cfg(feature = "python-stubgen")]
impl pyo3_stub_gen::PyStubType for RawObsId {
    fn type_output() -> pyo3_stub_gen::TypeInfo {
        <u64 as pyo3_stub_gen::PyStubType>::type_output()
    }

    fn type_input() -> pyo3_stub_gen::TypeInfo {
        <u64 as pyo3_stub_gen::PyStubType>::type_output()
            | <PyObsId as pyo3_stub_gen::PyStubType>::type_output()
    }
}

/// An Obs ID that a function takes: an `int` or an `ObsId`, which is valid.
///
/// An `int` that is not a valid Obs ID raises `ObsIdError`, as the library's
/// [`ObsId::validate`] returns [`crate::obs_id::ObsIdError`].
#[derive(Clone, Copy)]
pub struct ObsIdArg(pub ObsId);

impl<'a, 'py> FromPyObject<'a, 'py> for ObsIdArg {
    type Error = PyErr;

    fn extract(obj: Borrowed<'a, 'py, PyAny>) -> PyResult<Self> {
        if let Ok(obs_id) = obj.cast::<PyObsId>() {
            return Ok(Self(obs_id.get().0));
        }
        let number: u64 = obj.extract()?;
        ObsId::validate(number)
            .map(Self)
            .map_err(|e| obs_id_error(obj.py(), e))
    }
}

#[cfg(feature = "python-stubgen")]
impl pyo3_stub_gen::PyStubType for ObsIdArg {
    fn type_output() -> pyo3_stub_gen::TypeInfo {
        <PyObsId as pyo3_stub_gen::PyStubType>::type_output()
    }

    fn type_input() -> pyo3_stub_gen::TypeInfo {
        <u64 as pyo3_stub_gen::PyStubType>::type_output()
            | <PyObsId as pyo3_stub_gen::PyStubType>::type_output()
    }
}
