import {readIdentity} from "./identity.js";
import {hex} from "./crypto.js";
import {endpoint,pin,ViewSession} from "./view-session.js";
import {normalizePolicy,directPolicy} from "./ice-policy.js";
import {QUALITY_MODES} from "./session-policy.js";
import {probeCapabilities,capabilityBlockers,compatibilityGuidance} from "./capabilities.js";
import {BOOK_KEY,connectionKey,readBook,writeBook,saveEntry,removeEntry,forEndpoint} from "./connection-book.js";
import {applyAppearance,takeSelection,applyLanguage,uiText as t} from "./controller-ui.js";
applyAppearance();applyLanguage();
const $=id=>document.getElementById(id);
const url=$("view-endpoint"),server=$("view-server"),host=$("view-host"),consent=$("view-consent"),connect=$("view-connect"),disconnect=$("view-disconnect"),next=$("view-next"),video=$("view-video"),status=$("view-status");
let config=null,identity=null,active=null,displayCount=0,policy=directPolicy(),allowControl=false,allowExtended=false,inputActive=false,compatible=false;
const control=$("view-control"),enable=$("input-enable"),disable=$("input-disable"),inputMode=$("input-mode"),extended=$("view-extended"),qualityMode=$("quality-mode"),qualityApply=$("quality-apply");
const saved=$("saved-connection"),connectionLabel=$("connection-label"),save=$("connection-save"),remove=$("connection-remove"),bookStatus=$("connection-book-status");
let book=[],bookAvailable=false,toolsCollapsed=false;
const toolsToggle=$("view-tools-toggle"),tools=$("view-tools");
function updateTools(){
    const live=!!active&&active.phase==="viewing"&&!active.stopRequested;
    if(!live)toolsCollapsed=false;
    toolsToggle.hidden=!live;tools.hidden=live&&toolsCollapsed;
    toolsToggle.setAttribute("aria-expanded",String(!tools.hidden));
    toolsToggle.textContent=t(tools.hidden?"Show tools":"Hide tools");
}
toolsToggle.addEventListener("click",()=>{if(!active||active.phase!=="viewing"||active.stopRequested)return;active.releaseInput();toolsCollapsed=!toolsCollapsed;updateTools();});
window.addEventListener("resize",()=>{if(inputActive)active?.releaseInput();});
window.visualViewport?.addEventListener("resize",()=>{if(inputActive)active?.releaseInput();});
if(matchMedia("(pointer: coarse)").matches)inputMode.value="tap";
function state(kind,label){status.dataset.state=kind;status.textContent=t(label);$("view-placeholder").hidden=!video.hidden;document.body.classList.toggle("session-live",kind==="viewing");}
function controls(){
    const locked=!!active;
    server.disabled=host.disabled=locked;consent.disabled=locked||!config||!identity||!compatible;
    let good=false;try{pin(server.value);pin(host.value);good=true;}catch{}
    connect.disabled=locked||!good||!identity||!config||!compatible||!consent.checked;
    $("view-recheck").disabled=locked||loading;
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
    $("consent-label").textContent=t("I verified the identities and request one {scope} session (up to {duration}). Separate host approval is required.",{scope:t(show?"screen AND input":"read-only"),duration:t(long?"five minutes":"30 seconds")});
    updateTools();
}
function refreshBook(selected=""){
    saved.replaceChildren(new Option(t("Select a connection"),""));
    try{
        book=readBook(localStorage.getItem(BOOK_KEY));bookAvailable=true;
        if(config)for(const entry of forEndpoint(book,config))saved.add(new Option(entry.label,connectionKey(entry)));
        if([...saved.options].some(o=>o.value===selected))saved.value=selected;
        bookStatus.textContent=t("Saved details never authorize a session. Verify both identities independently.");
    }catch{book=[];bookAvailable=false;bookStatus.textContent=t("Local connection storage unavailable or invalid. Manual connections remain available.");}
    controls();
}
function updateBook(action,selected=""){
    if(active||!bookAvailable)return;
    try{
        const fresh=readBook(localStorage.getItem(BOOK_KEY));localStorage.setItem(BOOK_KEY,writeBook(action(fresh)));refreshBook(selected);
    }catch(e){refreshBook();bookStatus.textContent=t("Connection details were not changed: ")+String(e?.message??e).slice(0,100);}
}
function applySaved(){
    if(active||!saved.value)return;
    const entry=book.find(v=>connectionKey(v)===saved.value&&v.endpoint===config);if(!entry)return;
    server.value=entry.serverId;host.value=entry.hostKey;connectionLabel.value=entry.label;
    control.checked=false;extended.checked=false;consent.checked=false;controls();
}
saved.addEventListener("change",applySaved);
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
    if(loading||active)return;loading=true;compatible=false;consent.checked=false;
    $("view-compat").textContent=t("Checking local browser capabilities…");
    state("checking",t("Checking configuration and capabilities"));controls();
    try{
        const r=await fetch("/view-config.json",{cache:"no-store"});if(!r.ok)throw Error("Viewer not enabled by operator");
        const v=await r.json();if(v.schema!==1||v.mode!=="direct-read-only")throw Error("Invalid viewer configuration");
        config=endpoint(v.endpoint).href;url.value=config;policy=normalizePolicy(v.ice);allowControl=v.control===true;$("control-option").hidden=!allowControl;control.checked=false;allowExtended=v.extended===true;$("extended-option").hidden=!allowExtended;extended.checked=false;
        const checks=await probeCapabilities(),blockers=capabilityBlockers(checks);
        $("view-compat").textContent=blockers.length?t("Connection unavailable: ")+blockers.join("; ")+". "+t(compatibilityGuidance(checks)):t("Required browser capabilities verified locally (not a remote session).");
        if(blockers.length){identity=null;$("view-id").value="";$("view-public").value="";state("error",t("Local compatibility check failed. No connection attempted."));return;}
        identity=await readIdentity();compatible=true;
        $("view-id").value=identity?.id??"";$("view-public").value=identity?hex(identity.pub):"";
        consent.checked=false;refreshBook();
        const selected=takeSelection();
        if(selected&&[...saved.options].some(o=>o.value===selected)){saved.value=selected;applySaved();}
        state("idle",identity?t("Ready. Enter the independently verified identities."):t("Create a browser identity first."));
    }catch(e){config=null;identity=null;compatible=false;$("view-id").value="";$("view-public").value="";$("view-compat").textContent=t("Configuration or local identity unavailable. Recheck or manage identity.");state("error",String(e.message??e));}finally{loading=false;controls();}
}
function metrics(m){
    displayCount=m.count;$("view-route").textContent=t("Transport: ")+(m.route==="relay-both"||m.route==="relay"?t("TURN relay"):m.route==="direct"?t("Direct"):t("Verifying"));$("view-frames").textContent=String(m.frames);
    $("view-resolution").textContent=m.width&&m.height?m.width+" × "+m.height:"—";
    $("view-time").textContent=m.seconds+" s";
    $("view-layout").textContent=m.layout===m.count?t("All {count} approved displays",{count:m.count}):t("Display {index} of {count}",{index:m.layout+1,count:m.count});
    if(m.quality)$("quality-status").textContent=t("Host quality: ")+t(QUALITY_MODES[m.quality.mode])+" / "+t(QUALITY_MODES[m.quality.tier])+(m.qualityPending!==null?t(" — applying"):"")+(active?.control&&m.quality.tier!==1?t(" — input disabled at reduced resolution"):"");
    controls();
}
for(const item of [server,host])item.addEventListener("input",()=>{saved.value="";consent.checked=false;controls();});
consent.addEventListener("change",controls);control.addEventListener("change",()=>{consent.checked=false;controls();});extended.addEventListener("change",()=>{consent.checked=false;controls();});
qualityApply.addEventListener("click",()=>{active?.setQuality(Number(qualityMode.value));controls();});
enable.addEventListener("click",()=>active?.enableInput(inputMode.value));disable.addEventListener("click",()=>active?.releaseInput());
connect.addEventListener("click",async()=>{
    if(connect.disabled||active||!compatible||document.hidden)return;
    displayCount=0;for(const id of ["view-frames","view-resolution","view-time"])$(id).textContent="—";$("view-layout").textContent=t("No approved displays");$("quality-status").textContent=t("Awaiting host status");qualityMode.value="0";
    const s=new ViewSession({url:config,serverId:server.value,hostKey:host.value,identityId:identity.id,video,policy,control:allowControl&&control.checked,extended:allowExtended&&extended.checked,onInput:value=>{inputActive=value;$("input-status").textContent=value?t("Input active — keep focus on the remote screen. Leaving it releases input."):t("Input disabled; click Enable input again to resume.");controls();},onState:(kind,label)=>{if(active===s){state(kind,label);controls();}},onMetrics:m=>{if(active===s)metrics(m);}});
    active=s;controls();
    try{await s.run();}catch(e){s.cancel();state("error",String(e.message??e));}
    finally{if(active===s){active=null;inputActive=false;consent.checked=false;displayCount=0;document.body.classList.remove("session-live");$("view-placeholder").hidden=false;controls();}}
});
disconnect.addEventListener("click",()=>{active?.stop();controls();});
next.addEventListener("click",()=>active?.next());
$("view-recheck").addEventListener("click",()=>{if(!active)load();});
window.addEventListener("pageshow",()=>{if(!active)load();});
controls();load();
