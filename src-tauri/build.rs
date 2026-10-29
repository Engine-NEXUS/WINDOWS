// build.rs — Tauri v2 build script.
// `tauri_build::build()` processes tauri.conf.json + the capabilities/ directory and
// emits the gen/schemas used by the `generate_context!` macro. Without this call the
// capability "main-cap" referenced in the config is never registered.

fn main() {
    // Declare the `mobile` cfg used by `#[cfg_attr(mobile, ...)]` so the
    // unexpected_cfgs lint doesn't fire on desktop builds.
    println!("cargo::rustc-check-cfg=cfg(mobile)");

    tauri_build::build();

    // Re-run when native assets or capabilities change.
    println!("cargo:rerun-if-changed=native");
    println!("cargo:rerun-if-changed=capabilities");
    println!("cargo:rerun-if-changed=tauri.conf.json");
}
