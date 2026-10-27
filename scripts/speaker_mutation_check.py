"""Run Voice Lock safety mutations against isolated copies of the real modules.

Requires the repository's cached Cargo dependencies. No project source is edited.
The harness runs the modules' own tests plus corrupt-profile/reload regressions.
"""

import os
import shutil
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
HARNESS = r'''
pub mod memcore { pub mod crypto; }
pub mod voice_profile;

#[cfg(test)]
mod safety_regressions {
    use crate::voice_profile::*;

    fn path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("nexus_mutation_{}_{}.json", std::process::id(), label))
    }
    fn valid() -> VoiceProfile {
        let mut profile = VoiceProfile::new(0.5);
        for _ in 0..3 { profile.add_embedding(vec![0.2; 96]); }
        profile.num_clips = 3;
        profile
    }
    #[test]
    fn invalid_persisted_profile_is_refused() {
        let path = path("invalid");
        let mut profile = valid();
        profile.embeddings[0] = vec![0.0; 96];
        std::fs::write(&path, serde_json::to_vec(&profile).unwrap()).unwrap();
        assert!(VoiceProfile::load(&path).is_err());
        let _ = std::fs::remove_file(path);
    }
    #[test]
    fn corrupt_reload_discards_previously_valid_owner() {
        let path = path("reload");
        valid().save(&path).unwrap();
        let mut verifier = SpeakerVerifier::new(path.clone()).unwrap();
        assert!(verifier.verify(&vec![0.2; 96]).1);
        std::fs::write(&path, b"corrupt").unwrap();
        assert!(verifier.reload().is_err());
        assert!(!verifier.verify(&vec![0.2; 96]).1);
        let _ = std::fs::remove_file(path);
    }
}
'''


def replace_once(source: str, old: str, new: str) -> str:
    if source.count(old) != 1:
        raise RuntimeError(f"mutation anchor must occur exactly once: {old[:80]}")
    return source.replace(old, new, 1)


def replace_body(source: str, signature: str, body: str) -> str:
    start = source.index(signature)
    opening = source.index("{", start)
    depth = 1
    closing = opening + 1
    while depth:
        if source[closing] == "{":
            depth += 1
        elif source[closing] == "}":
            depth -= 1
        closing += 1
    return source[:opening + 1] + body + source[closing - 1:]


def check_frontend() -> None:
    frontend = ROOT / "frontend"
    cache = frontend / "node_modules/.cache"
    cache.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="nexus-speaker-mutation-", dir=cache) as directory:
        harness = Path(directory)
        shutil.copytree(frontend / "src", harness / "src")
        for name in ("vitest.config.ts", "package.json"):
            shutil.copyfile(frontend / name, harness / name)
        source_path = harness / "src/net/orchestrator.ts"
        original = source_path.read_text(encoding="utf-8")
        mutant = replace_once(
            original,
            'const locked = await invoke<boolean>("voice_lock_enabled").catch(() => true);',
            "const locked = false;",
        )
        command = [
            "node", str(frontend / "node_modules/vitest/vitest.mjs"), "run",
            "src/net/orchestrator.test.ts", "--root", str(harness),
            "--config", str(harness / "vitest.config.ts"),
        ]
        for label, source in (("baseline", original), ("frontend confirmation voice gate bypassed", mutant)):
            source_path.write_text(source, encoding="utf-8")
            result = subprocess.run(command, cwd=harness, encoding="utf-8", errors="replace", capture_output=True)
            output = result.stdout + result.stderr
            if label == "baseline":
                if result.returncode != 0:
                    raise RuntimeError("frontend baseline failed:\n" + output[-8000:])
                print("PASS frontend baseline: actual orchestrator regression tests", flush=True)
            elif result.returncode == 0:
                raise RuntimeError(f"SURVIVED: {label}")
            elif "Tests  " not in output or "failed" not in output:
                raise RuntimeError(f"frontend mutation did not reach tests:\n{output[-8000:]}")
            else:
                print(f"KILLED: {label}", flush=True)

        # Exercise the actual enrollment lifecycle without touching project sources.
        source_path.write_text(original, encoding="utf-8")
        session_path = harness / "src/setup/enrollmentSession.ts"
        session = session_path.read_text(encoding="utf-8")
        command[3] = "src/setup/enrollmentSession.test.ts"
        session_mutants = {
            "enrollment records without pausing wake": replace_once(session, "await pauseWake();", "void pauseWake();"),
            "enrollment never restores wake listening": replace_once(session, "await resumeWake();", "/* resume removed */"),
            "pending microphone permission blocks cancellation": replace_once(session, 'signal.addEventListener("abort", abortRun, { once: true });', "/* cancellation listener removed */"),
        }
        for label, code in [("baseline", session), *session_mutants.items()]:
            session_path.write_text(code, encoding="utf-8")
            result = subprocess.run(command, cwd=harness, encoding="utf-8", errors="replace", capture_output=True)
            output = result.stdout + result.stderr
            if label == "baseline":
                if result.returncode != 0:
                    raise RuntimeError("enrollment baseline failed:\n" + output[-8000:])
                print("PASS enrollment baseline: actual mic-session lifecycle tests", flush=True)
            elif result.returncode == 0:
                raise RuntimeError(f"SURVIVED: {label}")
            elif "Tests  " not in output or "failed" not in output:
                raise RuntimeError(f"enrollment mutation did not reach tests:\n{output[-8000:]}")
            else:
                print(f"KILLED: {label}", flush=True)


def main() -> None:
    original = (ROOT / "src-tauri/src/voice_profile.rs").read_text(encoding="utf-8")
    mutants = {
        "locked voice gate allows everyone": replace_once(
            original, "!enabled || !captured_voice || owner == TurnOwnership::Verified", "true"
        ),
        "corrupt settings silently unlock": replace_body(
            original, "pub fn voice_lock_from_settings(content: &str) -> bool", " false ",
        ),
        "unrelated settings save unlocks voice": replace_body(
            original, "pub fn voice_lock_after_settings_save(stored: bool, explicit_change: Option<bool>) -> bool",
            " false ",
        ),
        "missing profile verifies": replace_once(
            original, "None => (f32::NEG_INFINITY, false),", "None => (1.0, true),"
        ),
        "corrupt reload keeps stale owner": replace_once(
            original,
            "pub fn reload(&mut self) -> anyhow::Result<()> {\n        self.profile = None;",
            "pub fn reload(&mut self) -> anyhow::Result<()> {",
        ),
        "profile validation disabled": replace_body(original, "pub fn is_valid(&self) -> bool", " true "),
        "memory wipe can rotate the voice key": replace_body(
            original, "fn profile_key() -> Option<crate::memcore::crypto::Key>",
            " crate::memcore::crypto::process_key() ",
        ),
        "failed deletion reports success": replace_body(
            original, "pub fn delete_profile_file(path: &Path) -> anyhow::Result<()>",
            " crate::memcore::crypto::shred_file(path); Ok(()) ",
        ),
        "profile stored as plaintext": replace_once(
            original,
            'let sealed = crate::memcore::crypto::seal(&key, &data)\n            .ok_or_else(|| anyhow::anyhow!("Failed to encrypt voice profile"))?;',
            "let sealed = data;",
        ),
        "held-out calibration ignored": replace_once(
            original, "Ok((min_owner - 0.05).clamp(DEFAULT_THRESHOLD, 0.95))", "Ok(DEFAULT_THRESHOLD)"
        ),
        "authentication failure grants a profile": replace_once(
            original,
            '.ok_or_else(|| anyhow::anyhow!("Voice profile authentication failed"))?',
            '''.unwrap_or_else(|| {
                let mut fallback = VoiceProfile::new(DEFAULT_THRESHOLD);
                for _ in 0..3 { fallback.add_embedding(vec![0.2; 96]); }
                fallback.num_clips = 3;
                serde_json::to_vec(&fallback).unwrap()
            })''',
        ),
    }
    with tempfile.TemporaryDirectory(prefix="nexus-speaker-mutations-") as directory:
        harness = Path(directory)
        (harness / "src/memcore").mkdir(parents=True)
        (harness / "Cargo.toml").write_text('''[package]
name = "nexus-speaker-safety-harness"
version = "0.0.0"
edition = "2021"
[dependencies]
anyhow = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = { version = "0.4.45", features = ["serde"] }
ring = "0.17"
keyring = "3"
tracing = "0.1"
once_cell = "1"
''', encoding="utf-8")
        shutil.copyfile(ROOT / "src-tauri/Cargo.lock", harness / "Cargo.lock")
        shutil.copyfile(ROOT / "src-tauri/src/memcore/crypto.rs", harness / "src/memcore/crypto.rs")
        (harness / "src/lib.rs").write_text(HARNESS, encoding="utf-8")
        source_path = harness / "src/voice_profile.rs"
        command = [
            "cargo", "test", "--offline", "--lib", "--manifest-path", str(harness / "Cargo.toml"),
            "--target-dir", str(ROOT / "src-tauri/target/speaker-safety"), "-j", "1", "--", "--test-threads=1",
        ]
        env = dict(os.environ, CARGO_BUILD_JOBS="1")
        source_path.write_text(original, encoding="utf-8")
        check_command = [
            "cargo", "check", "--offline", "--lib", "--manifest-path", str(harness / "Cargo.toml"),
            "--target-dir", str(ROOT / "src-tauri/target/speaker-safety"), "-j", "1",
        ]
        checked = subprocess.run(check_command, cwd=ROOT, env=env, encoding="utf-8", errors="replace", capture_output=True)
        if checked.returncode != 0:
            raise RuntimeError("production profile module did not compile:\n" + (checked.stdout + checked.stderr)[-8000:])
        print("PASS production profile and crypto compilation", flush=True)
        for label, source in [("baseline", original), *mutants.items()]:
            source_path.write_text(source, encoding="utf-8")
            result = subprocess.run(command, cwd=ROOT, env=env, encoding="utf-8", errors="replace", capture_output=True)
            output = result.stdout + result.stderr
            if label == "baseline":
                if result.returncode != 0:
                    raise RuntimeError("baseline failed:\n" + output[-8000:])
                print("PASS baseline: actual voice_profile and crypto tests + two safety regressions", flush=True)
            elif result.returncode == 0:
                raise RuntimeError(f"SURVIVED: {label}")
            elif "test result: FAILED" not in output:
                raise RuntimeError(f"mutation did not reach tests ({label}):\n{output[-8000:]}")
            else:
                print(f"KILLED: {label}", flush=True)
    check_frontend()
    print(f"PASS: {len(mutants) + 4}/{len(mutants) + 4} safety mutations killed", flush=True)


if __name__ == "__main__":
    main()
