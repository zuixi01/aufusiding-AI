use camino::Utf8PathBuf;
use uniffi_bindgen::bindings::{GenerateOptions, TargetLanguage};

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

    for (language, out_dir) in [
        (TargetLanguage::Swift, swift_out),
        (TargetLanguage::Kotlin, kotlin_out),
    ] {
        uniffi_bindgen::bindings::generate(GenerateOptions {
            languages: vec![language],
            source: udl.clone(),
            out_dir,
            metadata_no_deps: true,
            // Keep generation independent of optional swiftformat/ktlint installations.
            format: false,
            ..GenerateOptions::default()
        })?;
    }

    Ok(())
}
