fn main() {
    // The PKGBUILD stamps the commit a package is built from, for the Linux update check.
    println!("cargo:rerun-if-env-changed=KLIPT_COMMIT");
    if let Ok(commit) = std::env::var("KLIPT_COMMIT") {
        println!("cargo:rustc-env=KLIPT_COMMIT={commit}");
    }
    tauri_build::build()
}
