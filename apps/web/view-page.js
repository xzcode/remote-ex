import {readIdentity} from "./identity.js";
import {hex} from "./crypto.js";
import {endpoint,pin,ViewSession} from "./view-session.js";
import {normalizePolicy,directPolicy} from "./ice-policy.js";
const $=id=>document.getElementById(id);
const url=$("view-endpoint"),server=$("view-server"),host=$("view-host"),consent=$("view-consent"),connect=$("view-connect"),disconnect=$("view-disconnect"),next=$("view-next"),video=$("view-video"),status=$("view-status");
let config=null,identity=null,active=null,displayCount=0,policy=directPolicy(),allowControl=false,inputActive=false;
const control=$("view-control"),enable=$("input-enable"),disable=$("input-disable"),inputMode=$("input-mode");
if(matchMedia("(pointer: coarse)").matches)inputMode.value="tap";
function state(kind,label){status.dataset.state=kind;status.textContent=label;$("view-placeholder").hidden=!video.hidden;}
function controls(){
    const locked=!!active;
    server.disabled=host.disabled=locked;consent.disabled=locked||!config||!identity;
    let good=false;try{pin(server.value);pin(host.value);good=true;}catch{}
    connect.disabled=locked||!good||!identity||!config||!consent.checked;
    disconnect.disabled=!locked;control.disabled=locked||!allowControl;next.disabled=!locked||active.control||active.phase!=="viewing"||active.stopRequested||displayCount<=1;
    const show=allowControl&&control.checked;enable.hidden=disable.hidden=$("input-status").hidden=$("input-mode-wrap").hidden=!show;inputMode.disabled=inputActive;
    enable.disabled=!locked||!active.control||active.phase!=="viewing"||active.stopRequested||!active.frames||inputActive;
    disable.disabled=!inputActive;
    $("consent-label").textContent=show?"I verified the identities and request one screen AND input session; host approval is separate.":"I verified the identities and request one read-only session (up to 30 seconds after approval).";
}
let loading=false;
async function load(){
    if(loading||active)return;loading=true;
    try{
        const r=await fetch("/view-config.json",{cache:"no-store"});if(!r.ok)throw Error("Viewer not enabled by operator");
        const v=await r.json();if(v.schema!==1||v.mode!=="direct-read-only")throw Error("Invalid viewer configuration");
        config=endpoint(v.endpoint).href;url.value=config;policy=normalizePolicy(v.ice);allowControl=v.control===true;$("control-option").hidden=!allowControl;control.checked=false;
        identity=await readIdentity();
        $("view-id").value=identity?.id??"";$("view-public").value=identity?hex(identity.pub):"";
        state("idle",identity?"Ready. Enter the independently verified identities.":"Create a browser identity first.");
    }catch(e){config=null;state("error",String(e.message??e));}finally{loading=false;controls();}
}
function metrics(m){
    displayCount=m.count;$("view-route").textContent="Transport: "+(m.route==="relay-both"||m.route==="relay"?"TURN relay":m.route==="direct"?"Direct":"Verifying");$("view-frames").textContent=String(m.frames);
    $("view-resolution").textContent=m.width&&m.height?m.width+" × "+m.height:"—";
    $("view-time").textContent=m.seconds+" s";
    $("view-layout").textContent=m.layout===m.count?"All "+m.count+" approved displays":"Display "+(m.layout+1)+" of "+m.count;
    controls();
}
for(const item of [server,host])item.addEventListener("input",()=>{consent.checked=false;controls();});
consent.addEventListener("change",controls);control.addEventListener("change",()=>{consent.checked=false;controls();});
enable.addEventListener("click",()=>active?.enableInput(inputMode.value));disable.addEventListener("click",()=>active?.releaseInput());
connect.addEventListener("click",async()=>{
    if(connect.disabled||active)return;
    displayCount=0;for(const id of ["view-frames","view-resolution","view-time"])$(id).textContent="—";$("view-layout").textContent="No approved displays";
    const s=new ViewSession({url:config,serverId:server.value,hostKey:host.value,identityId:identity.id,video,policy,control:allowControl&&control.checked,onInput:value=>{inputActive=value;$("input-status").textContent=value?"Input active — keep focus on the remote screen. Leaving it releases input.":"Input disabled; click Enable input again to resume.";controls();},onState:(kind,label)=>{if(active===s){state(kind,label);controls();}},onMetrics:m=>{if(active===s)metrics(m);}});
    active=s;controls();
    try{await s.run();}catch(e){s.cancel();state("error",String(e.message??e));}
    finally{if(active===s){active=null;inputActive=false;consent.checked=false;displayCount=0;$("view-placeholder").hidden=false;controls();}}
});
disconnect.addEventListener("click",()=>{active?.stop();controls();});
next.addEventListener("click",()=>active?.next());
window.addEventListener("pageshow",()=>{if(!active)load();});
controls();load();
