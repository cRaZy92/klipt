fn main() {
    println!("cargo:rustc-link-lib=obs");
    println!("cargo:rustc-link-lib=wayland-client");
    println!("cargo:rustc-link-lib=glib-2.0");
    println!("cargo:rerun-if-changed=wrapper.h");
    let b = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_arg("-I/usr/include").clang_arg("-Ithird_party/simde")
        .allowlist_function("(obs|gs|video|audio|calldata|bfree|blog|base|proc_handler|signal_handler)_.*")
        .allowlist_function("bfree")
        .allowlist_type("(obs|gs|video|audio|calldata|encoder)_.*")
        .allowlist_var("(OBS|LOG|MAX|VIDEO|AUDIO|GS)_.*")
        .derive_default(true)
        .generate()
        .expect("bindgen");
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    b.write_to_file(out.join("obs.rs")).unwrap();
}
