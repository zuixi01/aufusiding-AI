fn main() {
    println!("cargo:rerun-if-changed=src/prometheus.udl");

    uniffi::generate_scaffolding("src/prometheus.udl")
        .expect("failed to generate uniffi scaffolding");

    // UniFFI 0.31 requires this 32 KiB scratch buffer to remain const: into_array
    // consumes it while producing the much smaller exported metadata static.
    // Limit the lint expectation to that generated declaration, and fail if an
    // upstream template change makes this compatibility adapter obsolete.
    let path = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("missing OUT_DIR"))
        .join("prometheus.uniffi.rs");
    let scaffolding = std::fs::read_to_string(&path).expect("failed to read generated scaffolding");
    let declaration = "const UNIFFI_META_CONST_UDL_PROMETHEUS:";
    assert_eq!(
        scaffolding.matches(declaration).count(),
        1,
        "UniFFI metadata declaration changed; review the large_const_arrays adapter"
    );
    let expected = concat!(
        "#[expect(clippy::large_const_arrays, ",
        "reason = \"UniFFI requires this compile-time metadata scratch buffer\")]\n",
        "const UNIFFI_META_CONST_UDL_PROMETHEUS:"
    );
    std::fs::write(&path, scaffolding.replacen(declaration, expected, 1))
        .expect("failed to adapt generated scaffolding");
}
