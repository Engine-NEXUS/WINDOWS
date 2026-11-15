//! Compile actual orchestrator functions without constructing an app or dispatching input.
#![allow(dead_code)]
use crate::choice_boundary::{AppHandle, Runtime};
use std::sync::{Arc, atomic::{AtomicBool,Ordering}};
pub enum Subsystem { LocalCommand }
pub struct ProcessResult {request_id:String,subsystem:Subsystem,handled_locally:bool}
fn install_new_request(_:Subsystem)->(String,Arc<AtomicBool>){("fixture".into(),Arc::new(AtomicBool::new(false)))}
fn is_cancelled(c:&Arc<AtomicBool>)->bool{c.load(Ordering::Relaxed)}
fn speak_line<R:Runtime>(_:&AppHandle<R>,_:String,_:&str){}
fn clear_active_request(_:&str){}
fn say_and_finish<R:Runtime>(_:&AppHandle<R>,request_id:String,_:String)->Result<ProcessResult,String>{Ok(ProcessResult{request_id,subsystem:Subsystem::LocalCommand,handled_locally:true})}
async fn drain_ghost_followups<R:Runtime>(_:&AppHandle<R>){}
include!(concat!(env!("OUT_DIR"),"/orchestration.rs"));
