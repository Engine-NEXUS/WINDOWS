"""Mutate isolated copies of actual decoder/local-dispatch functions; no OS actions."""
from pathlib import Path
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def block(source, marker):
    start = source.index(marker)
    opening = source.index('{', start)
    depth = 1
    end = opening + 1
    while depth:
        depth += (source[end] == '{') - (source[end] == '}')
        end += 1
    return source[start:opening], source[opening+1:end-1]


def function(source, marker):
    head, body = block(source, marker)
    return head + '{' + body + '}'


def once(source, old, new):
    assert source.count(old) == 1, old
    return source.replace(old, new, 1)


def harness(decoder, orchestrator, stt):
    # Wiring check is separate from runtime dispatch tests below: the native
    # recorder skips the frontend executor, so merely defining an adapter is insufficient.
    local_branch = orchestrator[orchestrator.index('match subsystem {'):]
    assert 'dispatch_native_local(\n                turn.session, &intent, &cancel_flag,' in local_branch
    assert 'crate::command_executor::execute_command,' in local_branch
    assert 'Some(crate::stt_groq::NEXUS_VOCABULARY)' not in stt
    constants = '\n'.join(re.search(pattern, decoder, re.S).group(0) for pattern in [
        r'pub const NEXUS_VOCABULARY: &str = ".*?";',
        r'pub const NEUTRAL_DECODER_PROMPT: &str = ".*?";',
        r'const USE_OWNER_COMMAND_VOCABULARY: bool = .*?;',
    ])
    selected = [function(decoder, 'pub enum DecoderBias'),
                function(decoder, 'pub(crate) fn decoder_bias_for'),
                function(decoder, 'pub(crate) fn decoder_bias_prompt')]
    tests = ''
    for name in ['captured_commands_use_neutral_decoder_even_after_owner_verification', 'decoder_bias_follows_ownership']:
        head, body = block(decoder, 'fn ' + name + '()')
        tests += '#[test]\n' + head + '{' + body + '}\n'
    async_tests = ''
    for name in ['native_local_adapter_executes_once_and_propagates_result',
                 'native_local_adapter_skips_legacy_cancelled_and_unsupported_turns',
                 'native_local_adapter_preserves_execution_error']:
        _, body = block(orchestrator, 'async fn ' + name + '()')
        async_tests += '#[test]\nfn ' + name + '(){block_on(async {' + body + '});}\n'
    return r'''
use std::sync::atomic::{AtomicBool,Ordering};
#[derive(Clone,Debug)]
pub enum ParsedIntent { OpenApp{target:String},OpenUrl{target:String,url:String},CloseApp{target:String},WhatsappChat{contact:String},Search{query:String},MediaPlayPause,MediaNext,MediaPrevious,MediaStop,Greeting{reply:String},Unknown{raw:String} }
mod intent_parser { pub use crate::ParsedIntent; }
mod voice_profile { #[derive(Clone,Copy)] pub enum TurnOwnership {Verified,Unenrolled,Uncertain,Rejected} }
mod command_executor {
 #[derive(Debug)] pub enum Intent {OpenApp{target:String},OpenUrl{target:String,url:String},CloseApp{target:String},WhatsappChat{contact:String},Search{query:String},MediaPlayPause,MediaNext,MediaPrevious,MediaStop,Greeting{reply:String}}
 #[derive(Debug)] pub struct CommandResult {pub success:bool,pub message:String}
}
fn block_on<F: std::future::Future>(future:F)->F::Output {
 struct Noop; impl std::task::Wake for Noop { fn wake(self:std::sync::Arc<Self>){} }
 let waker=std::task::Waker::from(std::sync::Arc::new(Noop));
 let mut context=std::task::Context::from_waker(&waker);
 match std::pin::pin!(future).poll(&mut context) {std::task::Poll::Ready(value)=>value,_=>panic!("unexpected pending test future")}
}
''' + 'mod command_center {use crate::ParsedIntent;\n' + function((ROOT/'src-tauri/src/command_center.rs').read_text(encoding='utf-8'), 'pub(crate) fn to_local_intent') + '}\n' + function(orchestrator, 'async fn dispatch_native_local') + '\nmod stt_groq {\n#[derive(Clone,Copy,PartialEq,Eq,Debug)]\n' + constants + '\n'.join(selected) + '\nmod tests {use super::*;\n' + tests + '}}\nmod tests {use super::*;\n' + async_tests + '}\n'


def main():
    decoder=(ROOT/'src-tauri/src/stt_groq.rs').read_text(encoding='utf-8')
    orchestrator=(ROOT/'src-tauri/src/orchestrator.rs').read_text(encoding='utf-8')
    stt=(ROOT/'src-tauri/src/stt.rs').read_text(encoding='utf-8')
    # Place derives directly on the enum, not the extracted constants.
    def source(d,o,s):
        return harness(d,o,s).replace('mod stt_groq {\n#[derive(Clone,Copy,PartialEq,Eq,Debug)]\n','mod stt_groq {\n').replace('pub enum DecoderBias','#[derive(Clone,Copy,PartialEq,Eq,Debug)] pub enum DecoderBias')
    cases=[('baseline',decoder,orchestrator,stt),
      ('owner command prompt restored',once(decoder,'const USE_OWNER_COMMAND_VOCABULARY: bool = false;','const USE_OWNER_COMMAND_VOCABULARY: bool = true;'),orchestrator,stt),
      ('neutral prompt replaced by command vocabulary',once(decoder,'DecoderBias::Neutral => NEUTRAL_DECODER_PROMPT,','DecoderBias::Neutral => NEXUS_VOCABULARY,'),orchestrator,stt),
      ('legacy double execution guard removed',decoder,once(orchestrator,'if session == 0 || cancelled.load(Ordering::SeqCst)','if cancelled.load(Ordering::SeqCst)'),stt),
      ('cancelled execution guard removed',decoder,once(orchestrator,'if session == 0 || cancelled.load(Ordering::SeqCst)','if session == 0'),stt),
      ('native local executor removed',decoder,once(orchestrator,'Some(execute(local).await)','None'),stt),
      ('native local call disconnected',decoder,once(orchestrator,'dispatch_native_local(\n                turn.session, &intent, &cancel_flag,','disconnected_adapter(\n                turn.session, &intent, &cancel_flag,'),stt),
      ('frontend prompt restored',decoder,orchestrator,once(stt,'transcribe_with_groq(&samples, &groq_key, &state.client, None)','transcribe_with_groq(&samples, &groq_key, &state.client, Some(crate::stt_groq::NEXUS_VOCABULARY))'))]
    with tempfile.TemporaryDirectory(prefix='nexus-neutral-local-') as temp:
        path=Path(temp)
        for i,(name,d,o,s) in enumerate(cases):
            try: content=source(d,o,s)
            except AssertionError:
                if i==0: raise
                print('KILLED wiring:',name);continue
            (path/'test.rs').write_text(content,encoding='utf-8')
            built=subprocess.run(['rustc','--edition=2021','--test',str(path/'test.rs'),'-o',str(path/'test.exe')],capture_output=True,text=True)
            if built.returncode:
                raise RuntimeError('Harness failed to compile: '+built.stderr[:3000])
            tested=subprocess.run([str(path/'test.exe')],capture_output=True,text=True)
            if (tested.returncode==0)!=(i==0):
                raise RuntimeError('Unexpected result '+name+'\n'+tested.stdout)
            print(('PASS 5 actual-function tests:' if i==0 else 'KILLED runtime:'),name)
    print('7/7 mutations killed; source unchanged; no native OS action executed.')


if __name__=='__main__':main()
