import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const read=p=>readFileSync(new URL('../'+p,import.meta.url),'utf8').replace(/\r\n/g,'\n');
test('choice commands are registered and sidebar restricted',()=>{
  const source=read('src-tauri/src/desktop_choices.rs');
  for(const name of ['ghost_pending_choice','ghost_choose_control','ghost_cancel_choice']){
    assert.ok(read('src-tauri/src/lib.rs').includes('desktop_choices::'+name));
    assert.ok(source.includes('fn '+name));
  }
  assert.equal((source.match(/window.label\(\) != "sidebar"/g)||[]).length,3);
  assert.ok(source.includes('h.0 as isize == w.hwnd && w.pid == std::process::id()'));
  assert.ok(source.includes('w.hwnd == SIDEBAR_HWND.load'));
});
test('selection consumes native state, re-resolves identity and compares lock state',()=>{
  const s=read('src-tauri/src/desktop_choices.rs');
  assert.ok(s.indexOf('p.take().unwrap()')<s.indexOf('desktop_native::perform'));
  assert.ok(s.indexOf('desktop_native::check_identity')<s.indexOf('focus_window'));
  assert.ok(s.includes('pending.lock_on != Some(lock_on)'));
  assert.ok(s.includes('Arc::ptr_eq(&p.cancel, cancel)'));
  assert.ok(s.includes('bind(&pending.cancel, lock_on)'));
});
test('blocking selection work has a bounded wait and cancels late work',()=>{
  const s=read('src-tauri/src/desktop_choices.rs');
  assert.ok(s.includes('tokio::task::spawn_blocking'));
  assert.ok(s.includes('tokio::time::timeout'));
  assert.ok(s.includes('no automatic retry'));
  assert.ok(s.includes('publish(&app).await'));
  const o=read('src-tauri/src/orchestrator.rs');
  const task=o.slice(o.indexOf('async fn run_ghost_center_task'),o.indexOf('/// Ghost-unknown retry'));
  assert.ok(task.includes('cancel_flag.store(true'));
  assert.ok(task.includes('tokio::time::timeout'));
  assert.ok(task.includes('choose_voice_with_lock'));
  assert.ok(task.includes('bind(&cancel_flag, lock_at_start)'));
});
test('cancellation and sidebar closure invalidate choices',()=>{
  for(const path of ['src-tauri/src/ghost.rs','src-tauri/src/commands.rs','src-tauri/src/orchestrator.rs'])
    assert.ok(read(path).includes('desktop_choices::clear()'));
});
test('native adapter has no visual target fallback or raw text logging',()=>{
  const s=read('src-tauri/src/desktop_native.rs');
  assert.doesNotMatch(s,/screenshot|ocr|gemini|vision_click|println!|tracing::/i);
  assert.ok(s.includes('get_runtime_id'));
  assert.ok(s.includes('GetProcessTimes'));
  assert.ok(s.includes('insertion_matches'));
  assert.ok(s.includes('Another control covers the target'));
});
test('sidebar sends only token and number and prevents repeated submission',()=>{
  const s=read('frontend/src/sidebar/DesktopChoices.tsx');
  assert.ok(s.includes('"ghost_choose_control",{token,number}'));
  assert.ok(s.includes('busyRef.current=true'));
  assert.ok(s.includes('current.current?.token!==token'));
  assert.ok(s.includes('e.key==="Escape"'));
});

test('wake capture preserves pending choice while explicit cancellation clears it',()=>{
 const s=read('src-tauri/src/orchestrator.rs');
 const barge=s.slice(s.indexOf('pub(crate) fn request_barge_in'),s.indexOf('/// Signal that the current request is done'));
 assert.ok(barge.includes('capture_preserves_choice(reason)'));
 assert.ok(barge.includes('desktop_choices::cancel_running()'));
 assert.ok(barge.includes('else { cancel_active(); }'));
 assert.ok(barge.includes('pub fn cancel_active() {\n    crate::desktop_choices::clear();'));
});

test('capture handoff preserves only pending choice, explicit abort remains default',()=>{
 assert.ok(read('frontend/src/stage/orbRuntime.ts').includes('await abortCapture(true)'));
 assert.ok(read('frontend/src/audio/recorder.ts').includes('await cancelOrchestrator(forCapture)'));
 assert.ok(read('frontend/src/net/orchestrator.ts').includes('if (forCapture) await invoke("orchestrator_cancel", {forCapture: true})'));
 assert.ok(read('src-tauri/src/orchestrator.rs').includes('if for_capture == Some(true) { "barge" } else { "ipc-cancel" }'));
});
