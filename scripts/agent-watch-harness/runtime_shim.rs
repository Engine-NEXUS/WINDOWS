//! Compile and exercise the ACTUAL runtime glue with fake platform/storage/delivery boundaries.
//! No real keyring, audio, OS power APIs, sockets, filesystem writes or Tauri application are used.
// The glue is included in a nested boundary crate below, so its existing `crate::` imports resolve here.
extern crate self as tauri;
extern crate self as keyring;
pub use crate::agent_lifecycle;
use std::collections::BTreeMap;
use std::sync::{Mutex,OnceLock};
use std::sync::atomic::{AtomicBool,AtomicU64,AtomicUsize,Ordering};

pub trait Runtime {}
pub struct MockRuntime;impl Runtime for MockRuntime {}
pub struct AppHandle<R:Runtime>(std::marker::PhantomData<R>);
pub struct Paths;
pub trait Manager {fn path(&self)->Paths;}
impl<R:Runtime> Manager for AppHandle<R>{fn path(&self)->Paths{Paths}}
impl Paths {pub fn app_data_dir(&self)->Result<std::path::PathBuf,String>{Ok(std::env::temp_dir().join(format!("nexus-agent-shim-{}",std::process::id())))}}
pub trait Emitter {fn emit<T:serde::Serialize+Clone>(&self,event:&str,payload:T)->Result<(),String>;}
impl<R:Runtime> Emitter for AppHandle<R>{fn emit<T:serde::Serialize+Clone>(&self,_:&str,_:T)->Result<(),String>{Ok(())}}
#[derive(Debug)]pub enum Error{NoEntry}
impl std::fmt::Display for Error{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{write!(f,"mock key unavailable")}}
static KEYS:OnceLock<Mutex<BTreeMap<String,String>>>=OnceLock::new();
pub struct Entry(String);
impl Entry{
 pub fn new(service:&str,user:&str)->Result<Self,Error>{Ok(Self(format!("{service}/{user}")))}
 pub fn get_password(&self)->Result<String,Error>{KEYS.get_or_init(Default::default).lock().unwrap().get(&self.0).cloned().ok_or(Error::NoEntry)}
 pub fn set_password(&self,text:&str)->Result<(),Error>{KEYS.get_or_init(Default::default).lock().unwrap().insert(self.0.clone(),text.into());Ok(())}
}
pub mod app_dirs {pub const APP_ID:&str="test.nexus.agent-lifecycle";}
pub mod memcore {pub mod crypto{
 pub type Key=[u8;32];pub fn generate_key()->Option<Key>{Some([7;32])}
 pub fn seal(_:&Key,bytes:&[u8])->Option<Vec<u8>>{Some(bytes.to_vec())}
 pub fn open(_:&Key,bytes:&[u8])->Option<Vec<u8>>{Some(bytes.to_vec())}
}}
static SAVE_OK:AtomicBool=AtomicBool::new(true);
static CALLS:Mutex<Vec<String>>=Mutex::new(Vec::new());
pub mod memory {pub fn atomic_write(_:&std::path::Path,_:&[u8])->bool{
 let ok=super::SAVE_OK.load(super::Ordering::SeqCst);if ok {super::CALLS.lock().unwrap().push("persist".into());}ok
}}
static AWAKE:AtomicUsize=AtomicUsize::new(0);
static ACQUIRES:AtomicUsize=AtomicUsize::new(0);
pub mod keep_awake{
 pub const MAX_WATCH_HOLD:std::time::Duration=std::time::Duration::from_secs(8*3600);
 pub struct KeepAwakeLease;
 impl Drop for KeepAwakeLease{fn drop(&mut self){super::AWAKE.fetch_sub(1,super::Ordering::SeqCst);}}
 pub fn acquire_pinned(_:std::time::Duration)->Option<KeepAwakeLease>{super::ACQUIRES.fetch_add(1,super::Ordering::SeqCst);super::AWAKE.fetch_add(1,super::Ordering::SeqCst);Some(KeepAwakeLease)}
}
pub mod google {pub mod types{pub enum AlertUrgency{High}}}
pub mod proactive_policy{
 pub fn someone_is_here()->bool{true}
 pub fn link_alert(_:&str,_:&str){}
 pub fn submit_event<R:super::Runtime>(_:&super::AppHandle<R>,id:String,_:String,_:super::google::types::AlertUrgency,_:Option<String>){super::CALLS.lock().unwrap().push(format!("speech:{id}"));}
}
// Boundary only: the real rule lives in agent_attention.rs (needs the whole app); a lifecycle receipt is never suppressed here.
pub mod agent_attention{pub fn receipt_already_told(_complete:bool,_agent:&str,_stop_ms:Option<u64>)->bool{false}}
pub mod notify_hub{
 pub fn publish_agent_task<R:super::Runtime>(_:&super::AppHandle<R>,_:&str,_:&str,_:Option<&str>,receipt:&str,_:&str)->Option<String>{super::CALLS.lock().unwrap().push(format!("notice:{receipt}"));Some(receipt.into())}
}
static NOW:AtomicU64=AtomicU64::new(1000);
pub mod agent_hooks{
 #[derive(Clone,Copy,PartialEq,Eq)]pub enum Agent{Antigravity,OpenCode}
 pub const ALL_AGENTS:&[Agent]=&[Agent::Antigravity,Agent::OpenCode];
 impl Agent{pub fn from_name(s:&str)->Option<Self>{match s{"antigravity"=>Some(Self::Antigravity),"opencode"=>Some(Self::OpenCode),_=>None}}
 pub fn name(self)->&'static str{match self{Self::Antigravity=>"antigravity",Self::OpenCode=>"opencode"}}
 pub fn label(self)->&'static str{match self{Self::Antigravity=>"Antigravity",Self::OpenCode=>"OpenCode"}}}
 pub enum EventKind{Start,Notification,Stop,Message}
 pub struct StopInfo{pub fully_idle:Option<bool>,pub ok:Option<bool>}
 pub struct ParsedEvent{pub agent:Agent,pub kind:EventKind,pub cwd:Option<String>,pub identity:crate::agent_lifecycle::Identity,pub info:StopInfo,pub tail:Option<String>}
 pub fn now_ms()->u64{super::NOW.load(super::Ordering::SeqCst)}
}

// Include as text with namespace adaptation only. The harness build script creates this from the real glue.
include!(concat!(env!("OUT_DIR"),"/runtime_glue.rs"));

#[test]
fn real_runtime_pipeline_delivers_once_defers_away_preserves_other_holds_and_retries_failed_persistence(){
 let app=AppHandle::<MockRuntime>(std::marker::PhantomData);
 let make=|session:&str,kind,idle|agent_hooks::ParsedEvent{agent:agent_hooks::Agent::Antigravity,kind,cwd:Some("/repo".into()),identity:agent_lifecycle::Identity{session:Some(session.into()),..Default::default()},info:agent_hooks::StopInfo{fully_idle:idle,ok:None},tail:Some("Done".into())};
 let feed=|time,session:&str,kind,idle|{NOW.store(time,Ordering::SeqCst);record_hook(&app,&make(session,kind,idle));};
 feed(1000,"one",agent_hooks::EventKind::Start,None);assert_eq!(AWAKE.load(Ordering::SeqCst),1);
 feed(1001,"one",agent_hooks::EventKind::Stop,Some(false));NOW.store(20_000,Ordering::SeqCst);poll(&app,false);assert!(!CALLS.lock().unwrap().iter().any(|x|x.starts_with("speech:")));
 feed(20_001,"one",agent_hooks::EventKind::Stop,Some(true));NOW.store(40_000,Ordering::SeqCst);poll(&app,true);assert_eq!(AWAKE.load(Ordering::SeqCst),0);assert!(!CALLS.lock().unwrap().iter().any(|x|x.starts_with("speech:")));
 poll(&app,false);assert_eq!(CALLS.lock().unwrap().iter().filter(|x|x.starts_with("speech:")).count(),1);
 for t in 40_001..40_101{feed(t,"one",agent_hooks::EventKind::Stop,Some(true));poll(&app,t%2==0);}
 assert_eq!(CALLS.lock().unwrap().iter().filter(|x|x.starts_with("speech:")).count(),1);
 feed(50_000,"a",agent_hooks::EventKind::Start,None);feed(50_001,"b",agent_hooks::EventKind::Start,None);
 feed(50_002,"a",agent_hooks::EventKind::Stop,Some(true));NOW.store(70_000,Ordering::SeqCst);poll(&app,false);assert_eq!(AWAKE.load(Ordering::SeqCst),1);
 feed(70_001,"b",agent_hooks::EventKind::Stop,Some(true));SAVE_OK.store(false,Ordering::SeqCst);NOW.store(90_000,Ordering::SeqCst);poll(&app,false);
 assert_eq!(CALLS.lock().unwrap().iter().filter(|x|x.starts_with("speech:")).count(),2);
 SAVE_OK.store(true,Ordering::SeqCst);poll(&app,false);assert_eq!(AWAKE.load(Ordering::SeqCst),0);
 assert_eq!(CALLS.lock().unwrap().iter().filter(|x|x.starts_with("speech:")).count(),3);
 owned(&app,"opencode",None,"cancel-me",agent_lifecycle::Kind::Start,false);cancel_owned("cancel-me");owned(&app,"opencode",None,"cancel-me",agent_lifecycle::Kind::Stop,false);NOW.store(120_000,Ordering::SeqCst);poll(&app,false);
 assert_eq!(CALLS.lock().unwrap().iter().filter(|x|x.starts_with("speech:")).count(),3);
 assert_eq!(owned_phase("OpenCode","cancel-me"),Some(agent_lifecycle::Phase::Cancelled));
 feed(121_000,"waiting",agent_hooks::EventKind::Notification,None);assert!(!busy_since("Antigravity",121_000));
 feed(121_001,"waiting",agent_hooks::EventKind::Message,None);assert!(busy_since("Antigravity",121_000));
 assert_eq!(desktop_result("Antigravity",Some("/repo"),121_000),None);
 feed(121_002,"waiting",agent_hooks::EventKind::Stop,Some(true));NOW.store(140_000,Ordering::SeqCst);poll(&app,true);
 assert_eq!(desktop_result("Antigravity",Some("/repo"),121_000),Some(agent_lifecycle::Phase::Complete));
 deliver_labels(&app,&["Antigravity".into()]);deliver_labels(&app,&["Antigravity".into()]);
 assert_eq!(CALLS.lock().unwrap().iter().filter(|x|x.starts_with("speech:")).count(),4);
 owned(&app,"opencode",None,"exited",agent_lifecycle::Kind::Start,false);
 owned_exit(&app,"opencode","exited",true);NOW.store(160_000,Ordering::SeqCst);poll(&app,false);
 assert_eq!(owned_phase("OpenCode","exited"),Some(agent_lifecycle::Phase::Failed));
 assert_eq!(CALLS.lock().unwrap().iter().filter(|x|x.starts_with("speech:")).count(),5);
 owned(&app,"opencode",None,"long",agent_lifecycle::Kind::Start,false);
 let acquired=ACQUIRES.load(Ordering::SeqCst);NOW.store(3_800_000,Ordering::SeqCst);poll(&app,true);
 assert_eq!(AWAKE.load(Ordering::SeqCst),1);assert_eq!(ACQUIRES.load(Ordering::SeqCst),acquired+1);
 cancel_owned("long");assert_eq!(AWAKE.load(Ordering::SeqCst),0);
 let calls=CALLS.lock().unwrap();for (i,call) in calls.iter().enumerate(){if call.starts_with("notice:"){assert!(i>0&&calls[i-1]=="persist");}}
}
