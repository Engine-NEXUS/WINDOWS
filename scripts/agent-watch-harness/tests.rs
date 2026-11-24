use crate::agent_lifecycle::*;
use crate::power_thread::PowerThread;
use std::sync::{Arc, Mutex};

fn id(session: &str) -> Identity { Identity { session: Some(session.into()), ..Identity::default() } }
fn event(r: &mut Registry, identity: &Identity, kind: Kind, at: u64, idle: Option<bool>) -> Option<String> {
    r.observe(Input { agent: "antigravity", cwd: Some("/repo"), identity, kind, at_ms: at,
        fully_idle: idle, ok: None, signature: "same final output", authoritative: true })
}

#[test] fn busy_stop_never_finishes_or_releases_activity() {
    let mut r=Registry::default(); let i=id("s");
    event(&mut r,&i,Kind::Start,1,None); event(&mut r,&i,Kind::Stop,10,Some(false)); r.tick(100_000);
    assert!(r.active()); assert!(r.ready().is_empty());
}
#[test] fn unknown_adapter_stop_cannot_override_explicit_busy() {
    let mut r=Registry::default(); let i=id("s");
    event(&mut r,&i,Kind::Stop,1,Some(false)); event(&mut r,&i,Kind::Stop,2,None); r.tick(100_000);
    assert!(r.ready().is_empty()); assert!(r.active());
}
#[test] fn hundred_duplicate_stops_produce_one_receipt() {
    let mut r=Registry::default(); let i=id("s");
    event(&mut r,&i,Kind::Start,1,None);
    for t in 2..102 { event(&mut r,&i,Kind::Stop,t,Some(true)); }
    r.tick(SETTLE_MS+2); let ready=r.ready(); assert_eq!(ready.len(),1);
    assert!(r.claim(&ready[0].key).is_some());
    for t in 20_000..20_100 { event(&mut r,&i,Kind::Stop,t,Some(true)); r.tick(t); }
    assert!(r.ready().is_empty()); assert!(!r.active());
}
#[test] fn activity_during_settle_cancels_completion() {
    let mut r=Registry::default(); let i=id("s");
    event(&mut r,&i,Kind::Stop,1,Some(true)); event(&mut r,&i,Kind::Busy,5,None); r.tick(100_000);
    assert!(r.ready().is_empty()); assert!(r.active());
}
#[test] fn file_silence_is_not_an_authoritative_end() {
    let mut r=Registry::default(); let i=id("s");
    r.observe(Input {agent:"generic",cwd:None,identity:&i,kind:Kind::Stop,at_ms:1,fully_idle:None,ok:None,signature:"Done",authoritative:false});
    r.tick(100_000); assert!(r.ready().is_empty());
}
#[test] fn new_real_task_can_finish_with_identical_text() {
    let mut r=Registry::default(); let i=id("s");
    event(&mut r,&i,Kind::Stop,1,Some(true)); r.tick(SETTLE_MS+1);
    let first=r.ready()[0].clone(); r.claim(&first.key);
    event(&mut r,&i,Kind::Start,20_000,None); event(&mut r,&i,Kind::Stop,20_001,Some(true)); r.tick(40_000);
    let second=r.ready()[0].clone(); assert_ne!(first.receipt(),second.receipt());
}
#[test] fn repeated_internal_starts_do_not_create_tasks() {
    let mut r=Registry::default(); let i=id("s");
    for t in 1..100 { event(&mut r,&i,Kind::Start,t,None); }
    event(&mut r,&i,Kind::Stop,101,Some(true));r.tick(20_000);assert_eq!(r.ready().len(),1);
}
#[test] fn separate_sessions_and_workspaces_are_independent() {
    let mut r=Registry::default();let a=id("a");let b=id("b");
    event(&mut r,&a,Kind::Start,1,None);event(&mut r,&b,Kind::Start,2,None);
    event(&mut r,&a,Kind::Stop,3,Some(true));r.tick(20_000);
    assert_eq!(r.ready().len(),1);assert!(r.active());assert!(r.busy_since("antigravity",0));
    event(&mut r,&b,Kind::Stop,20_001,Some(true));r.tick(40_000);assert_eq!(r.ready().len(),2);assert!(!r.active());
    assert_eq!(scope("agent",None,&a),scope("agent",Some("/two"),&a));
    assert_ne!(scope("agent",Some("/one"),&Identity::default()),scope("agent",Some("/two"),&Identity::default()));
    let mut changing=Registry::default();
    changing.observe(Input{agent:"antigravity",cwd:None,identity:&a,kind:Kind::Start,at_ms:1,fully_idle:None,ok:None,signature:"",authoritative:true});
    event(&mut changing,&a,Kind::Stop,2,Some(true));changing.tick(20_000);
    assert!(!changing.active());assert_eq!(changing.ready().len(),1);
}
#[test] fn owned_watch_and_its_hook_share_one_task() {
    let mut r=Registry::default(); let owned=Identity{owned_watch:Some("watch123".into()),..Identity::default()};
    let hook=Identity{session:Some("provider-session".into()),execution:Some("provider-turn".into()),..owned.clone()};
    event(&mut r,&owned,Kind::Start,1,None);event(&mut r,&hook,Kind::Stop,2,Some(true));
    event(&mut r,&owned,Kind::Stop,3,None);r.tick(20_000);assert_eq!(r.ready().len(),1);
}
#[test] fn cancellation_blocks_late_completion() {
    let mut r=Registry::default();let i=Identity{owned_watch:Some("w".into()),..id("s")};
    event(&mut r,&i,Kind::Start,1,None);r.cancel_owned("w",2);event(&mut r,&i,Kind::Stop,3,Some(true));r.tick(100_000);
    assert!(r.ready().is_empty());assert!(!r.active());
}
#[test] fn cancel_after_settling_suppresses_an_undelivered_completion() {
    let mut r=Registry::default();let i=Identity{owned_watch:Some("w".into()),..id("s")};
    event(&mut r,&i,Kind::Stop,1,Some(true));r.tick(20_000);assert_eq!(r.ready().len(),1);
    r.cancel_owned("w",20_001);assert!(r.ready().is_empty());
}
#[test] fn claimed_receipt_survives_restart_and_idle_cycles() {
    let mut r=Registry::default();let i=id("s");event(&mut r,&i,Kind::Stop,1,Some(true));r.tick(20_000);
    r.claim(&r.ready()[0].key);let mut restored=Registry::restore(&r.encode().unwrap(),21_000).unwrap();
    for t in 22_000..22_100 {event(&mut restored,&i,Kind::Stop,t,Some(true));restored.tick(t);assert!(restored.ready().is_empty());}
}
#[test] fn restart_does_not_keep_awake_for_an_old_process() {
    let mut r=Registry::default();event(&mut r,&id("s"),Kind::Start,1,None);
    let restored=Registry::restore(&r.encode().unwrap(),2).unwrap();assert!(!restored.active());
}
#[test] fn pending_completion_survives_restart_without_replay_of_claimed_siblings() {
    let mut r=Registry::default();event(&mut r,&id("a"),Kind::Stop,1,Some(true));event(&mut r,&id("b"),Kind::Stop,2,Some(true));r.tick(20_000);
    r.claim(&r.ready()[0].key);let restored=Registry::restore(&r.encode().unwrap(),21_000).unwrap();assert_eq!(restored.ready().len(),1);
}
#[test] fn corrupt_or_wrong_schema_state_is_refused() {
    assert!(Registry::restore("broken",0).is_err());
    let text=Registry::default().encode().unwrap().replace("\"schema\":1","\"schema\":99");assert!(Registry::restore(&text,0).is_err());
    assert!(Registry::restore(&"x".repeat(600_000),0).is_err());
}
#[test] fn waiting_releases_running_hold_and_resume_restores_it() {
    let mut r=Registry::default();let i=id("s");event(&mut r,&i,Kind::Start,1,None);assert!(r.active());
    event(&mut r,&i,Kind::Wait,2,None);assert!(!r.active());assert!(r.ready().is_empty());
    event(&mut r,&i,Kind::Start,3,None);assert!(r.active());
}
#[test] fn timeout_is_expired_not_finished() {
    let mut r=Registry::default();event(&mut r,&id("s"),Kind::Start,1,None);r.tick(MAX_RUN_MS+1);
    assert_eq!(r.ready()[0].phase,Phase::Expired);assert!(!r.active());
}
#[test] fn out_of_order_stop_cannot_rewind_newer_activity() {
    let mut r=Registry::default();let i=id("s");event(&mut r,&i,Kind::Start,100,None);
    assert!(event(&mut r,&i,Kind::Stop,99,Some(true)).is_none());r.tick(20_000);assert!(r.ready().is_empty());
}
#[test] fn repeated_provider_event_id_does_not_rearm_after_delivery() {
    let mut r=Registry::default();let mut i=id("s");i.event=Some("event123".into());
    event(&mut r,&i,Kind::Stop,1,Some(true));r.tick(20_000);r.claim(&r.ready()[0].key);
    assert!(event(&mut r,&i,Kind::Stop,30_000,Some(true)).is_none());assert!(r.ready().is_empty());
}
#[test] fn failure_never_becomes_success() {
    let mut r=Registry::default();let i=id("s");
    r.observe(Input{agent:"a",cwd:None,identity:&i,kind:Kind::Stop,at_ms:1,fully_idle:Some(true),ok:Some(false),signature:"failed",authoritative:true});
    r.observe(Input{agent:"a",cwd:None,identity:&i,kind:Kind::Stop,at_ms:2,fully_idle:Some(true),ok:None,signature:"retry",authoritative:true});
    r.tick(20_000);assert_eq!(r.ready()[0].phase,Phase::Failed);
}
#[test] fn saved_state_contains_no_plaintext_output_or_paths() {
    let mut r=Registry::default();event(&mut r,&id("private session"),Kind::Stop,1,Some(true));let text=r.encode().unwrap();
    assert!(!text.contains("same final output"));assert!(!text.contains("/repo"));assert!(!text.contains("private session"));
}
#[test] fn power_assert_and_clear_always_use_one_dedicated_thread() {
    let calls=Arc::new(Mutex::new(Vec::new()));let log=calls.clone();let caller=std::thread::current().id();
    let p=Arc::new(PowerThread::new(move |awake| {log.lock().unwrap().push((std::thread::current().id(),awake));Ok(())}).unwrap());
    for value in [true,true,false,true,false] {let p=p.clone();std::thread::spawn(move || p.set(value).unwrap()).join().unwrap();}
    let log=calls.lock().unwrap();assert_eq!(log.len(),5);assert_ne!(log[0].0,caller);assert!(log.iter().all(|x|x.0==log[0].0));assert_eq!(log.last().unwrap().1,false);
}
#[test] fn power_backend_errors_propagate() {
    let p=PowerThread::new(|_|Err("OS refused".into())).unwrap();assert_eq!(p.set(true),Err("OS refused".into()));
}
#[test] fn completion_requires_fifteen_seconds_of_quiet() {
    let mut r=Registry::default();event(&mut r,&id("s"),Kind::Stop,100,Some(true));
    r.tick(101);assert!(r.ready().is_empty());r.tick(15_099);assert!(r.ready().is_empty());
    r.tick(15_100);assert_eq!(r.ready().len(),1);
}
#[test] fn actual_hook_parser_keeps_session_turn_and_event_identity() {
    let ev=crate::hook_parser::parse_event("agent=codex&event=stop",r#"{"thread-id":"s","turn-id":"t","event_id":"e","last-assistant-message":"done"}"#).unwrap();
    assert_eq!(ev.identity.session.as_deref(),Some("s"));assert_eq!(ev.identity.execution.as_deref(),Some("t"));assert_eq!(ev.identity.event.as_deref(),Some("e"));
}
#[test] fn actual_hook_parser_preserves_antigravity_idle_veto_and_outer_session() {
    let ev=crate::hook_parser::parse_event("agent=antigravity&event=stop",r#"{"trajectoryId":"outer","execution_id":"inner","fullyIdle":false}"#).unwrap();
    assert_eq!(ev.identity.session.as_deref(),Some("outer"));assert!(ev.identity.execution.is_none());assert_eq!(ev.info.fully_idle,Some(false));
}
#[test] fn actual_hook_parser_keeps_owned_alias_from_query_or_plugin_body() {
    for (query,body) in [("agent=claude&event=stop&watch=watch-123","{}"),("agent=opencode&event=stop",r#"{"nexus_watch_id":"watch-123"}"#)] {
        assert_eq!(crate::hook_parser::parse_event(query,body).unwrap().identity.owned_watch.as_deref(),Some("watch-123"));
    }
}
#[test] fn actual_hook_parser_does_not_split_stop_only_transcript_paths_into_sessions() {
    let ev=crate::hook_parser::parse_event("agent=antigravity&event=stop",r#"{"transcriptPath":"private.jsonl"}"#).unwrap();
    assert!(ev.identity.session.is_none());assert!(ev.source.is_some());
}
#[test] fn timed_out_power_assertion_is_cleared_on_the_owner_thread() {
    let calls=Arc::new(Mutex::new(Vec::new()));let log=calls.clone();
    let (sent,received)=std::sync::mpsc::channel();
    let (entered,started)=std::sync::mpsc::channel();
    let (release,blocked)=std::sync::mpsc::channel();
    let p=PowerThread::new(move |awake| {
        log.lock().unwrap().push(awake);
        if awake {let _=entered.send(());blocked.recv().unwrap();}
        else {let _=sent.send(());}
        Ok(())
    }).unwrap();
    let p=Arc::new(p);let task=p.clone();
    let caller=std::thread::spawn(move ||task.set_timeout(true,std::time::Duration::from_millis(100)));
    started.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
    assert!(caller.join().unwrap().is_err());release.send(()).unwrap();
    received.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
    assert_eq!(*calls.lock().unwrap(),vec![true,false]);
}
