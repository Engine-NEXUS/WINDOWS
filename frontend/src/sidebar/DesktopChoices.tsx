import {useEffect,useRef,useState} from "react";
import {invoke} from "@tauri-apps/api/core";
import {maySelect,validChoices,type DesktopChoices as Choices} from "./desktopChoiceModel";
import "./desktop-choices.css";

/** Native pending state is authoritative. No control identity/coordinates are sent back from UI. */
export function DesktopChoices(){
  const [choices,setChoices]=useState<Choices|null>(null);
  const [busy,setBusy]=useState(false);const [message,setMessage]=useState("");
  const current=useRef<Choices|null>(null);const deadline=useRef(0);const busyRef=useRef(false);
  useEffect(()=>{
    let disposed=false;let timer:ReturnType<typeof setTimeout>;
    const poll=async()=>{
      try{
        const value=await invoke<unknown>("ghost_pending_choice");if(disposed)return;
        const next=validChoices(value)?value:null;
        if(next||!busyRef.current){
          if(next?.token!==current.current?.token){busyRef.current=false;setBusy(false);setMessage("")}
          current.current=next;deadline.current=next?performance.now()+next.expires_in_ms:0;setChoices(next);
        }
      }catch{if(!disposed){current.current=null;setChoices(null)}}
      if(!disposed)timer=setTimeout(poll,250);
    };
    void poll();return()=>{disposed=true;clearTimeout(timer)};
  },[]);
  const cancel=()=>{current.current=null;setChoices(null);void invoke("ghost_cancel_choice").catch(()=>{})};
  useEffect(()=>{
    const escape=(e:KeyboardEvent)=>{if(e.key==="Escape"&&current.current)cancel()};
    window.addEventListener("keydown",escape,true);return()=>window.removeEventListener("keydown",escape,true);
  },[]);
  const select=async(number:number)=>{
    if(!maySelect(current.current,number,deadline.current,performance.now(),busyRef.current))return;
    const token=current.current!.token;busyRef.current=true;setBusy(true);
    try{
      const receipts=await invoke<Array<{outcome:string}>>("ghost_choose_control",{token,number});
      if(current.current?.token!==token)return;
      setMessage(receipts[receipts.length-1]?.outcome==="verified"?"The result was verified.":"The action could not be confirmed. Ask NEXUS again before continuing.");
    }catch{if(current.current?.token===token)setMessage("That choice is no longer available. Ask NEXUS again.")}
    finally{if(current.current?.token===token){current.current=null;setChoices(null)}busyRef.current=false;setBusy(false)}
  };
  return choices?<section className="desktop-choice-panel" aria-label="Choose a desktop control" aria-busy={busy}>
    <h2>Which control?</h2><p>Say “choose 1” or select a control below. Choices expire after 30 seconds.</p>
    <ol>{choices.choices.map(c=><li key={c.number}><button type="button" disabled={busy} onClick={()=>void select(c.number)}>
      <strong>{c.number}. {c.label}</strong><span>{c.role}{c.container?` · ${c.container}`:""}</span>
    </button></li>)}</ol><button type="button" onClick={cancel}>Cancel</button>
  </section>:message?<p className="desktop-choice-status" role="status">{message}</p>:null;
}
