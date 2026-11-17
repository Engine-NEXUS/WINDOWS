// build.rs — Tauri v2 build script.
// `tauri_build::build()` processes tauri.conf.json + the capabilities/ directory and
// emits the gen/schemas used by the `generate_context!` macro. Without this call the
// capability "main-cap" referenced in the config is never registered.

fn main() {
    println!("cargo:rerun-if-env-changed=NEXUS_MANAGED_PYTHON");
    println!("cargo:rerun-if-env-changed=TAURI_CONFIG");
    if std::env::var_os("CARGO_FEATURE_CONTRIBUTOR").is_some() {
        let config = std::env::var("TAURI_CONFIG").unwrap_or_default();
        // Prevent `--features contributor` from accidentally sharing Tauri's personal identity.
        let config: serde_json::Value = serde_json::from_str(&config).expect("TAURI_CONFIG must be JSON");
        assert_eq!(config.get("identifier").and_then(|v| v.as_str()), Some("com.nexus.assistant.contributor"),
            "Use node nexus.mjs build/dev for contributor configuration");
        let python = std::env::var("NEXUS_MANAGED_PYTHON").expect("Contributor builds require managed Python");
        assert!(std::path::Path::new(&python).is_absolute(), "Managed Python must be an absolute path");
    }
    // Declare the `mobile` cfg used by `#[cfg_attr(mobile, ...)]` so the
    // unexpected_cfgs lint doesn't fire on desktop builds.
    println!("cargo::rustc-check-cfg=cfg(mobile)");

    tauri_build::build();

    // Re-run when native assets or capabilities change.
    println!("cargo:rerun-if-changed=native");
    println!("cargo:rerun-if-changed=capabilities");
    println!("cargo:rerun-if-changed=tauri.conf.json");
}
