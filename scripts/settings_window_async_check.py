"""Guard the real settings opener's async type contract; live window smoke remains required."""
from pathlib import Path
import re
import subprocess
import tempfile
import textwrap
import sys

sys.stdout.reconfigure(errors="backslashreplace")
ROOT = Path(__file__).resolve().parents[1]
STUBS = r"""
mod tauri {
    pub trait Runtime {}
    pub struct Wry;
    impl Runtime for Wry {}
    pub struct AppHandle<R: Runtime>(std::marker::PhantomData<R>);
}
mod dyn_windows {
    use crate::tauri::{AppHandle,Runtime};
    pub struct WindowConfig;
    impl WindowConfig { pub fn settings() -> Self { Self } }
    pub struct Window;
    impl Window {
        pub fn set_always_on_top(&self, _:bool)->Result<(),String>{Ok(())}
        pub fn show(&self)->Result<(),String>{Ok(())}
        pub fn set_focus(&self)->Result<(),String>{Ok(())}
    }
    pub fn get_or_create_window<R:Runtime>(_:&AppHandle<R>,_:WindowConfig)->Result<Window,String>{Ok(Window)}
}
use tauri::Runtime;
"""

def main():
    source = (ROOT / "src-tauri/src/commands.rs").read_text(encoding="utf-8")
    match = re.search(r"pub (?:async )?fn open_settings_window<R: Runtime>", source)
    if not match:
        raise SystemExit("Settings opener anchor changed")
    end = source.index("\n}\n", match.start()) + 2
    function = source[match.start():end]
    start = source.index("    fn settings_window_command_returns_future()")
    end = source.index("\n    }", start) + 6
    regression = "#[test]\n" + textwrap.dedent(source[start:end])
    with tempfile.TemporaryDirectory(prefix="nexus-settings-async-") as directory:
        temp = Path(directory)
        src = temp / "contract.rs"
        exe = temp / "contract.exe"
        def compile_variant(body):
            src.write_text(STUBS + body + "\n" + regression, encoding="utf-8")
            return subprocess.run(["rustc", "--edition=2021", "--test", str(src), "-o", str(exe)],capture_output=True,text=True,encoding="utf-8",errors="replace",timeout=60)
        baseline = compile_variant(function)
        if baseline.returncode:
            print(baseline.stderr)
            raise SystemExit("FAIL: real opener does not satisfy its async regression")
        result = subprocess.run([str(exe)], capture_output=True,text=True,timeout=30)
        if result.returncode:
            print(result.stdout, result.stderr)
            raise SystemExit("Baseline contract test failed")
        mutant = compile_variant(function.replace("pub async fn", "pub fn", 1))
        if mutant.returncode == 0 or "Future" not in mutant.stderr:
            raise SystemExit("FAIL: synchronous-opener mutation survived or failed for another reason")
        print("PASS: actual opener/test async contract; 1/1 synchronous-opener mutation killed")
        print("The harness does not create a real WebView; fresh-release live smoke is still required.")

if __name__ == "__main__":
    main()
