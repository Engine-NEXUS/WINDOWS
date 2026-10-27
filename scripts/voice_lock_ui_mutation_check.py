"""Run real Voice Lock UI model tests against isolated temporary mutations."""
from pathlib import Path
import json
import shutil
import subprocess
import tempfile
import sys
sys.stdout.reconfigure(errors="backslashreplace")

ROOT = Path(__file__).resolve().parents[1]
FRONTEND = ROOT / "frontend"
MODEL = FRONTEND / "src/settings/voiceLockState.ts"
MUTATIONS = {
    "unverified requested state": ('if (typeof patch.speakerVerification === "boolean" && patch.speakerVerification !== status.effective)', 'if (false)'),
    "missing profile enable": ('if (profile?.enrolled !== true)', 'if (false)'),
    "overlapping saves": ('if (this.state.saving) throw new Error("Save already in progress.");', ''),
    "stale refresh overwrite": ('if (epoch !== this.epoch) return;', ''),
    "native disagreement accepted": ('if (!status.agrees) throw new Error', 'if (false) throw new Error'),
    "in-flight draft erased": ('Object.is(current[key], submitted[key])', 'true'),
    "untouched draft stays stale": ('return { ...current, speakerVerification: effective };', 'return current;'),
    "local draft discarded on refresh": ('dirty.has("speakerVerification") || ', ''),
    "old Save confirmation survives refresh": ('profileReady: false, saveVerified: false, error: null', 'profileReady: false, error: null'),
    "profile loss accepted": ('if (patch.speakerVerification === true && !status.profileReady)', 'if (false)'),
}

def main():
    source = MODEL.read_text(encoding="utf-8")
    display = (FRONTEND / "src/settings/VoiceLockStatus.tsx").read_text(encoding="utf-8")
    results = {}
    with tempfile.TemporaryDirectory(prefix=".voice-lock-mutations-", dir=FRONTEND) as directory:
        temp = Path(directory)
        shutil.copyfile(FRONTEND / "src/settings/voiceLockState.test.ts", temp / "voiceLockState.test.ts")
        shutil.copyfile(FRONTEND / "src/settings/VoiceLockStatus.test.tsx", temp / "VoiceLockStatus.test.tsx")
        config = temp / "vitest.config.mjs"
        config.write_text("export default " + json.dumps({"esbuild": {"jsx": "automatic"}, "test": {"environment": "node", "include": [str(temp / "*.test.ts").replace("\\", "/"), str(temp / "*.test.tsx").replace("\\", "/")], "testTimeout": 10000}}), encoding="utf-8")
        command = [shutil.which("node"), str(FRONTEND / "node_modules/vitest/vitest.mjs"), "run", "--config", str(config)]
        def run(candidate, ui=display):
            (temp / "voiceLockState.ts").write_text(candidate, encoding="utf-8")
            (temp / "VoiceLockStatus.tsx").write_text(ui, encoding="utf-8")
            return subprocess.run(command, cwd=FRONTEND, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=90)
        baseline = run(source)
        if baseline.returncode:
            print(baseline.stdout, baseline.stderr)
            raise SystemExit("Baseline failed; mutations not assessed")
        for name, (old, new) in MUTATIONS.items():
            if source.count(old) != 1:
                raise SystemExit("Mutation anchor changed: " + name)
            result = run(source.replace(old, new))
            killed = result.returncode != 0 and "AssertionError" in result.stdout + result.stderr
            results[name] = "killed" if killed else "survived or invalid failure"
            print(name + ": " + results[name], flush=True)
            if not killed:
                print(result.stdout, result.stderr)
        old = '{verified ? "✓ Saved" : null}'
        if display.count(old) != 1:
            raise SystemExit("Accessible Save mutation anchor changed")
        result = run(source, display.replace(old, '{"✓ Saved"}'))
        killed = result.returncode != 0 and "AssertionError" in result.stdout + result.stderr
        results["unverified Saved text rendered"] = "killed" if killed else "survived or invalid failure"
        print("unverified Saved text rendered: " + results["unverified Saved text rendered"], flush=True)
        if not killed:
            print(result.stdout, result.stderr)
    print(json.dumps(results, indent=2))
    raise SystemExit(0 if all(v == "killed" for v in results.values()) else 1)

if __name__ == "__main__":
    main()
