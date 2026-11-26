import {readIdentity} from "./identity.js";
import {hex} from "./crypto.js";
import {endpoint,pin,ViewSession} from "./view-session.js";
import {normalizePolicy,directPolicy} from "./ice-policy.js";
import {QUALITY_MODES} from "./session-policy.js";
import {BOOK_KEY,connectionKey,readBook,writeBook,saveEntry,removeEntry,forEndpoint} from "./connection-book.js";
const $=id=>document.getElementById(id);
const url=$("view-endpoint"),server=$("view-server"),host=$("view-host"),consent=$("view-consent"),connect=$("view-connect"),disconnect=$("view-disconnect"),next=$("view-next"),video=$("view-video"),status=$("view-status");
let config=null,identity=null,active=null,displayCount=0,policy=directPolicy(),allowControl=false,allowExtended=false,inputActive=false;
const control=$("view-control"),enable=$("input-enable"),disable=$("input-disable"),inputMode=$("input-mode"),extended=$("view-extended"),qualityMode=$("quality-mode"),qualityApply=$("quality-apply");
const saved=$("saved-connection"),connectionLabel=$("connection-label"),save=$("connection-save"),remove=$("connection-remove"),bookStatus=$("connection-book-status");
let book=[],bookAvailable=false;
if(matchMedia("(pointer: coarse)").matches)inputMode.value="tap";
function state(kind,label){status.dataset.state=kind;status.textContent=label;$("view-placeholder").hidden=!video.hidden;}
function controls(){
    const locked=!!active;
    server.disabled=host.disabled=locked;consent.disabled=locked||!config||!identity;
    let good=false;try{pin(server.value);pin(host.value);good=true;}catch{}
    connect.disabled=locked||!good||!identity||!config||!consent.checked;
    saved.disabled=locked||!bookAvailable||!config||saved.options.length<=1;connectionLabel.disabled=locked||!bookAvailable;
    save.disabled=locked||!bookAvailable||!good||!config||!connectionLabel.value.trim();remove.disabled=locked||!bookAvailable||!saved.value;
    disconnect.disabled=!locked;control.disabled=locked||!allowControl;extended.disabled=locked||!allowExtended;next.disabled=!locked||active.control||active.phase!=="viewing"||active.stopRequested||displayCount<=1;
    const show=allowControl&&control.checked;enable.hidden=disable.hidden=$("input-status").hidden=$("input-mode-wrap").hidden=!show;inputMode.disabled=inputActive;
    enable.disabled=!locked||!active.inputReady()||active.qualityPending!==null||inputActive;
    disable.disabled=!inputActive;
    const long=allowExtended&&extended.checked;
    $("quality-wrap").hidden=$("quality-status").hidden=!long;
    qualityApply.disabled=!locked||!active.scope.extended||active.phase!=="viewing"||active.qualityPending!==null||active.stopRequested;
    qualityMode.disabled=qualityApply.disabled;
    $("consent-label").textContent="I verified the identities and request one "+(show?"screen AND input":"read-only")+" session (up to "+(long?"five minutes":"30 seconds")+"). Separate host approval is required.";
}
function refreshBook(selected=""){
    saved.replaceChildren(new Option("Select a connection",""));
    try{
        book=readBook(localStorage.getItem(BOOK_KEY));bookAvailable=true;
        if(config)for(const entry of forEndpoint(book,config))saved.add(new Option(entry.label,connectionKey(entry)));
        if([...saved.options].some(o=>o.value===selected))saved.value=selected;
        bookStatus.textContent="Saved details never authorize a session. Verify both identities independently.";
    }catch{book=[];bookAvailable=false;bookStatus.textContent="Local connection storage unavailable or invalid. Manual connections remain available.";}
    controls();
}
function updateBook(action,selected=""){
    if(active||!bookAvailable)return;
    try{
        const fresh=readBook(localStorage.getItem(BOOK_KEY));localStorage.setItem(BOOK_KEY,writeBook(action(fresh)));refreshBook(selected);
    }catch(e){refreshBook();bookStatus.textContent="Connection details were not changed: "+String(e?.message??e).slice(0,100);}
}
saved.addEventListener("change",()=>{
    if(active||!saved.value)return;
    const entry=book.find(v=>connectionKey(v)===saved.value&&v.endpoint===config);if(!entry)return;
    server.value=entry.serverId;host.value=entry.hostKey;connectionLabel.value=entry.label;
    control.checked=false;extended.checked=false;consent.checked=false;controls();
});
connectionLabel.addEventListener("input",controls);
save.addEventListener("click",()=>{
    if(save.disabled)return;
    const entry={endpoint:config,serverId:server.value,hostKey:host.value,label:connectionLabel.value.trim()};
    updateBook(current=>saveEntry(current,entry),connectionKey(entry));
});
remove.addEventListener("click",()=>{if(!remove.disabled){const k=saved.value;updateBook(current=>removeEntry(current,k));consent.checked=false;controls();}});
window.addEventListener("storage",e=>{if(!active&&e.key===BOOK_KEY){consent.checked=false;refreshBook();}});
let loading=false;
async function load(){
    if(loading||active)return;loading=true;
    try{
        const r=await fetch("/view-config.json",{cache:"no-store"});if(!r.ok)throw Error("Viewer not enabled by operator");
        const v=await r.json();if(v.schema!==1||v.mode!=="direct-read-only")throw Error("Invalid viewer configuration");
        config=endpoint(v.endpoint).href;url.value=config;policy=normalizePolicy(v.ice);allowControl=v.control===true;$("control-option").hidden=!allowControl;control.checked=false;allowExtended=v.extended===true;$("extended-option").hidden=!allowExtended;extended.checked=false;
        identity=await readIdentity();
        $("view-id").value=identity?.id??"";$("view-public").value=identity?hex(identity.pub):"";
        consent.checked=false;refreshBook();state("idle",identity?"Ready. Enter the independently verified identities.":"Create a browser identity first.");
    }catch(e){config=null;state("error",String(e.message??e));}finally{loading=false;controls();}
}
function metrics(m){
    displayCount=m.count;$("view-route").textContent="Transport: "+(m.route==="relay-both"||m.route==="relay"?"TURN relay":m.route==="direct"?"Direct":"Verifying");$("view-frames").textContent=String(m.frames);
    $("view-resolution").textContent=m.width&&m.height?m.width+" × "+m.height:"—";
    $("view-time").textContent=m.seconds+" s";
    $("view-layout").textContent=m.layout===m.count?"All "+m.count+" approved displays":"Display "+(m.layout+1)+" of "+m.count;
    if(m.quality)$("quality-status").textContent="Host quality: "+QUALITY_MODES[m.quality.mode]+" / "+QUALITY_MODES[m.quality.tier]+(m.qualityPending!==null?" — applying":"")+(active?.control&&m.quality.tier!==1?" — input disabled at reduced resolution":"");
    controls();
}
for(const item of [server,host])item.addEventListener("input",()=>{saved.value="";consent.checked=false;controls();});
consent.addEventListener("change",controls);control.addEventListener("change",()=>{consent.checked=false;controls();});extended.addEventListener("change",()=>{consent.checked=false;controls();});
qualityApply.addEventListener("click",()=>{active?.setQuality(Number(qualityMode.value));controls();});
enable.addEventListener("click",()=>active?.enableInput(inputMode.value));disable.addEventListener("click",()=>active?.releaseInput());
connect.addEventListener("click",async()=>{
    if(connect.disabled||active)return;
    displayCount=0;for(const id of ["view-frames","view-resolution","view-time"])$(id).textContent="—";$("view-layout").textContent="No approved displays";$("quality-status").textContent="Awaiting host status";qualityMode.value="0";
    const s=new ViewSession({url:config,serverId:server.value,hostKey:host.value,identityId:identity.id,video,policy,control:allowControl&&control.checked,extended:allowExtended&&extended.checked,onInput:value=>{inputActive=value;$("input-status").textContent=value?"Input active — keep focus on the remote screen. Leaving it releases input.":"Input disabled; click Enable input again to resume.";controls();},onState:(kind,label)=>{if(active===s){state(kind,label);controls();}},onMetrics:m=>{if(active===s)metrics(m);}});
    active=s;controls();
    try{await s.run();}catch(e){s.cancel();state("error",String(e.message??e));}
    finally{if(active===s){active=null;inputActive=false;consent.checked=false;displayCount=0;$("view-placeholder").hidden=false;controls();}}
});
disconnect.addEventListener("click",()=>{active?.stop();controls();});
next.addEventListener("click",()=>active?.next());
window.addEventListener("pageshow",()=>{if(!active)load();});
controls();load();
