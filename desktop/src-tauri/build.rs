fn main() {
    // The PKGBUILD stamps the commit a package is built from, for the Linux update check.
    println!("cargo:rerun-if-env-changed=KLIPT_COMMIT");
    if let Ok(commit) = std::env::var("KLIPT_COMMIT") {
        println!("cargo:rustc-env=KLIPT_COMMIT={commit}");
    }
    let mut attributes = tauri_build::Attributes::new();
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // tauri-build embeds its manifest (Common Controls v6) only into the app's exe, so test
        // binaries get comctl32 v5 and die at load with STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139)
        // once they link a v6-only function. The linker embeds the same manifest into every
        // target instead.
        attributes =
            attributes.windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        let manifest = std::env::current_dir().unwrap().join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
    tauri_build::try_build(attributes).expect("tauri-build failed");
}
