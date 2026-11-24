fn main() {
    let root = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let file = root.join("../../src-tauri/src/desktop_choices.rs");
    println!("cargo:rerun-if-changed={}", file.display());
    let source = std::fs::read_to_string(file)
        .unwrap()
        .replace("#[tauri::command]", "")
        .replace("tauri::", "crate::choice_boundary::");
    let source = source
        .lines()
        .filter(|l| !l.starts_with("//!"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(
        std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("choices.rs"),
        source,
    )
    .unwrap();
    let file=root.join("../../src-tauri/src/orchestrator.rs");
    println!("cargo:rerun-if-changed={}",file.display());
    let source=std::fs::read_to_string(file).unwrap();
    let ranges=[("async fn run_ghost_center_task", "/// Ghost-unknown retry"),("async fn open_target_in_front", "async fn run_ghost_open")];
    let mut extracted=String::new();
    for (start,end) in ranges { let start=source.find(start).unwrap(); let end=start+source[start..].find(end).unwrap(); extracted.push_str(&source[start..end]); }
    std::fs::write(std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("orchestration.rs"),extracted.replace("tauri::", "crate::choice_boundary::")).unwrap();
}
