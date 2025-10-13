fn main() {
    // Tell PyO3 to link against Python
    pyo3_build_config::add_extension_module_link_args();
}
