fn main(){
 let manifest=std::env::var("CARGO_MANIFEST_DIR").unwrap();
 let src=std::path::Path::new(&manifest).join("../../src-tauri/src");
 let hooks=src.join("agent_hooks.rs");println!("cargo:rerun-if-changed={}",hooks.display());
 let hooks=std::fs::read_to_string(hooks).unwrap().replace("\r\n","\n");
 let types=hooks.find("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub enum Agent").unwrap();
 let end_types=hooks.find("#[derive(Debug, PartialEq, Eq)]\npub enum HookError").unwrap();
 let parser=hooks.find("/// Where a Stop event's text").unwrap();
 let end_parser=hooks.find("static CWDS:").unwrap();
 std::fs::write(std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("hook_parser.rs"),
     format!("use serde_json::Value;\n{}\n{}",&hooks[types..end_types],&hooks[parser..end_parser])).unwrap();
 for (file,marker,out) in [("watch_runner.rs","// ─── The loop around the engine","watch_policy.rs")] {
    let path=src.join(file);println!("cargo:rerun-if-changed={}",path.display());
    let text=std::fs::read_to_string(path).unwrap().replace("\r\n","\n");
    let end=text.find(marker).expect("policy boundary moved");
    let tests=text.find("#[cfg(test)]\nmod tests").expect("policy tests moved");
    let combined=format!("{}\n{}",&text[..end],&text[tests..]);
    let combined=combined.lines().filter(|l|!l.starts_with("//!")).collect::<Vec<_>>().join("\n");
    std::fs::write(std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join(out),combined).unwrap();
 }
 let root=src.join("agent_lifecycle_runtime.rs");
 println!("cargo:rerun-if-changed={}",root.display());
 let source=std::fs::read_to_string(root).unwrap();
 let source=source.lines().filter(|l|!l.starts_with("//!")).collect::<Vec<_>>().join("\n");
 // The compiled glue is unchanged except imports resolving against the fake boundary module.
 let source=source.replace("crate::","self::").replace("use self::agent_lifecycle::{self as life, Identity, Input, Kind, Phase, Registry};","use self::agent_lifecycle::{Identity, Input, Kind, Phase, Registry};")
  .replace("use std::collections::BTreeMap;","").replace("use std::sync::{Mutex, OnceLock};","")
  .replace("use tauri::{Emitter, Manager, Runtime};","").replace("tauri::","self::").replace("keyring::","self::");
 std::fs::write(std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("runtime_glue.rs"),source).unwrap();
}
