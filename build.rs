fn main() {
    // Gather build time info
    built::write_built_file().expect("Failed to acquire build-time information");
}
