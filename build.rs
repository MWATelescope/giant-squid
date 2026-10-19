fn main() {
    // Without this, cargo runs the build script again after any change in the
    // package.
    println!("cargo:rerun-if-changed=build.rs");

    #[cfg(feature = "python-stubgen")]
    stub_gen_rpath();
}

/// Adds the library directory of the Python that pyo3 builds for to the
/// rpath of the `stub_gen` binary only. `stub_gen` embeds Python, so it is
/// linked to a libpython, and a uv-managed Python keeps that library in a
/// directory that the dynamic loader does not search. Without this, running
/// `target/debug/stub_gen` fails with "libpython3.x.so.1.0: cannot open
/// shared object file".
///
/// pyo3 does this itself for its own binaries and tests, but a link argument
/// from a dependency's build script does not reach this crate's binaries.
/// It does nothing when pyo3 reports no library directory.
#[cfg(feature = "python-stubgen")]
fn stub_gen_rpath() {
    println!("cargo:rerun-if-env-changed=PYO3_PYTHON");
    if let Some(lib_dir) = pyo3_build_config::get().lib_dir() {
        println!("cargo:rustc-link-arg-bin=stub_gen=-Wl,-rpath,{lib_dir}");
    }
}
