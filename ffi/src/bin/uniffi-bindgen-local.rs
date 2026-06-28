use camino::Utf8PathBuf;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let udl = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing UDL path argument"))?;
    let out_root = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing output directory argument"))?;

    let udl = Utf8PathBuf::from(udl);
    let swift_out = Utf8PathBuf::from(&out_root).join("swift");
    let kotlin_out = Utf8PathBuf::from(&out_root).join("kotlin");

    std::fs::create_dir_all(&swift_out)?;
    std::fs::create_dir_all(&kotlin_out)?;

    uniffi_bindgen::generate_bindings(
        &udl,
        None,
        uniffi_bindgen::bindings::SwiftBindingGenerator,
        Some(&swift_out),
        None,
        None,
        true,
    )?;

    uniffi_bindgen::generate_bindings(
        &udl,
        None,
        uniffi_bindgen::bindings::KotlinBindingGenerator,
        Some(&kotlin_out),
        None,
        None,
        true,
    )?;

    Ok(())
}
