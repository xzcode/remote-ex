import {createIdentity,readIdentity,deleteIdentity} from "./identity.js";
import {hex} from "./crypto.js";
import {probeIdentityCapabilities,compatibilityGuidance} from "./capabilities.js";
const $=id=>document.getElementById(id);
const status=$("identity-status"),create=$("identity-create"),remove=$("identity-delete"),refresh=$("identity-refresh"),confirm=$("identity-confirm"),support=$("identity-support");
let busy=false,present=false,valid=false,canCreate=false;
function controls(){
    create.disabled=busy||!valid||present||!canCreate;
    refresh.disabled=busy;confirm.disabled=busy||(!present&&valid);
    remove.disabled=busy||confirm.disabled||!confirm.checked;
}
function render(identity,probe){
    present=!!identity;valid=true;confirm.checked=false;
    $("device-id").value=identity?.id??"";
    $("public-key").value=identity?hex(identity.pub):"";
    canCreate=!identity&&probe?.ok===true;
    if(identity){
        status.dataset.state="verified";status.textContent="Local identity ready";
        support.textContent="Stored non-extractable identity verified.";
    }else if(canCreate){
        status.dataset.state="idle";status.textContent="No local identity";
        support.textContent="Local Ed25519 signing and key storage verified. Create a device identity explicitly.";
    }else{
        status.dataset.state="unsupported";status.textContent="Identity creation unavailable";
        support.textContent="Identity compatibility: "+(probe?.code??"identity_unavailable")+". "+compatibilityGuidance([{id:"ed25519",ok:!["NotSupportedError","ed25519_failed","ed25519_timeout","ed25519_proof","ed25519_export"].includes(probe?.code)}]);
    }
}
async function run(action){
    if(busy)return;
    busy=true;canCreate=false;$("identity-error").textContent="";support.textContent="Checking local identity and storage…";
    status.dataset.state="running";status.textContent="Working…";controls();
    try{
        const value=await action();
        const probe=value?null:await probeIdentityCapabilities();
        render(value,probe);
    }catch{
        present=false;valid=false;confirm.checked=false;$("device-id").value="";$("public-key").value="";
        status.dataset.state="error";status.textContent="Identity unavailable";
        support.textContent="Refresh with a supported browser or WebView. No new identity will be created automatically.";
        $("identity-error").textContent="Browser identity could not be verified or stored. Refresh, or explicitly remove the local record to recover.";
    }finally{busy=false;controls();}
}
create.addEventListener("click",()=>{if(!create.disabled&&canCreate)run(createIdentity);});
refresh.addEventListener("click",()=>run(readIdentity));
confirm.addEventListener("change",controls);
remove.addEventListener("click",()=>{if(confirm.checked&&!remove.disabled)run(async()=>{await deleteIdentity();return readIdentity();});});
window.addEventListener("pageshow",()=>run(readIdentity));
document.addEventListener("visibilitychange",()=>{if(!document.hidden)run(readIdentity);});
run(readIdentity);
