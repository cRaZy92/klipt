//! Generates runtime-loaded bindings to libobs.
//!
//! libobs is NOT linked at build time. bindgen's `dynamic_library_name` produces a struct
//! (`Obs`) whose methods call through function pointers resolved with libloading, so the
//! build only needs headers:
//! - `OBS_INCLUDE_DIR`: the directory that contains `obs.h`
//!   (Windows: `<deps>/include/obs` from scripts/fetch-obs-windows.ps1; Linux default: /usr/include/obs)
//! - `SIMDE_INCLUDE_DIR`: SIMDe checkout (default: third_party/simde next to this file)

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-changed=include-stub");
    println!("cargo:rerun-if-env-changed=OBS_INCLUDE_DIR");
    println!("cargo:rerun-if-env-changed=SIMDE_INCLUDE_DIR");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    let obs_include = env::var("OBS_INCLUDE_DIR").unwrap_or_else(|_| "/usr/include/obs".into());
    let simde_include =
        env::var("SIMDE_INCLUDE_DIR").unwrap_or_else(|_| manifest_dir.join("third_party/simde").display().to_string());
    if !PathBuf::from(&obs_include).join("obs.h").exists() {
        panic!(
            "obs.h not found in OBS_INCLUDE_DIR={obs_include}. On Windows run \
             scripts/fetch-obs-windows.ps1 and set OBS_INCLUDE_DIR; on Linux install the OBS headers."
        );
    }

    let bindings = bindgen::Builder::default()
        .header(manifest_dir.join("wrapper.h").display().to_string())
        .clang_arg(format!("-I{obs_include}"))
        .clang_arg(format!("-I{simde_include}"))
        // Last, so a real obsconfig.h next to installed headers wins over the stub.
        .clang_arg(format!("-I{}", manifest_dir.join("include-stub").display()))
        // util/util_uint64.h calls the MSVC intrinsic _udiv128 in an inline helper. Older clang
        // (e.g. the GitHub runner's) doesn't declare it, and since clang 16 an undeclared call is an
        // error. We never bind that helper, so the diagnostic doesn't matter here.
        .clang_arg("-Wno-implicit-function-declaration")
        .dynamic_library_name("Obs")
        // Missing symbols become an Err stored in the struct instead of failing the whole load;
        // calling one panics, which only happens for functions we never use.
        .dynamic_link_require_all(false)
        .allowlist_function("obs_.*")
        .allowlist_function("(calldata|proc_handler|signal_handler)_.*")
        .allowlist_function("bfree")
        .allowlist_function("base_set_log_handler")
        .allowlist_var("(LOG|OBS|MODULE|MAX)_.*")
        .derive_default(true)
        .layout_tests(false)
        .generate()
        .expect("bindgen failed to generate libobs bindings");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings.write_to_file(out.join("obs_sys.rs")).expect("write bindings");

    match target_os.as_str() {
        // libobs log messages are formatted with vsnprintf. In the UCRT it is an inline function,
        // and this import library provides the out-of-line symbol.
        "windows" => println!("cargo:rustc-link-lib=legacy_stdio_definitions"),
        // Wayland display for libobs-opengl and the GLib loop that drives the screen-cast portal.
        "linux" => {
            println!("cargo:rustc-link-lib=wayland-client");
            println!("cargo:rustc-link-lib=glib-2.0");
        }
        _ => {}
    }
}
