import {createIdentity,readIdentity,deleteIdentity} from "./identity.js";
import {hex} from "./crypto.js";
const $=id=>document.getElementById(id);
const status=$("identity-status"),create=$("identity-create"),remove=$("identity-delete"),refresh=$("identity-refresh"),confirm=$("identity-confirm");
let busy=false,present=false,valid=false;
function controls(){
    create.disabled=busy||!valid||present;refresh.disabled=busy;confirm.disabled=busy||(!present&&valid);
    remove.disabled=busy||confirm.disabled||!confirm.checked;
}
function render(identity){
    present=!!identity;valid=true;confirm.checked=false;
    $("device-id").value=identity?.id??"";
    $("public-key").value=identity?hex(identity.pub):"";
    status.dataset.state=identity?"verified":"idle";status.textContent=identity?"Local identity ready":"No local identity";
}
async function run(action){
    if(busy)return;
    busy=true;$("identity-error").textContent="";status.dataset.state="running";status.textContent="Working…";controls();
    try{render(await action());}
    catch{
        present=false;valid=false;confirm.checked=false;$("device-id").value="";$("public-key").value="";
        status.dataset.state="error";status.textContent="Identity unavailable";
        $("identity-error").textContent="Browser identity could not be verified or stored. Refresh, or explicitly remove the local record to recover.";
    }finally{busy=false;controls();}
}
create.addEventListener("click",()=>run(createIdentity));
refresh.addEventListener("click",()=>run(readIdentity));
confirm.addEventListener("change",controls);
remove.addEventListener("click",()=>{if(confirm.checked)run(async()=>{await deleteIdentity();return readIdentity();});});
window.addEventListener("pageshow",()=>run(readIdentity));
document.addEventListener("visibilitychange",()=>{if(!document.hidden)run(readIdentity);});
run(readIdentity);
