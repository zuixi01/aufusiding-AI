fn main() {
    println!("cargo:rerun-if-changed=src/prometheus.udl");

    uniffi::generate_scaffolding("src/prometheus.udl")
        .expect("failed to generate uniffi scaffolding");
}
