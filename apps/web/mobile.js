import {probeCapabilities} from "./capabilities.js";
const status=document.getElementById("mobile-status"),checks=document.getElementById("mobile-checks"),open=document.getElementById("mobile-connect");
const policy=document.getElementById("mobile-policy");
async function run(){
    status.dataset.state="checking";
    const results=await probeCapabilities();
    checks.replaceChildren();
    for(const result of results){
        const item=document.createElement("li");
        item.dataset.capability=result.id;item.dataset.code=result.code;
        item.dataset.pass=String(result.ok);
        item.textContent=result.label+": "+(result.ok?"available":"unavailable")+" ("+result.code+")";
        checks.append(item);
    }
    const all=results.length===6&&results.every(v=>v.ok);
    status.dataset.state=all?"ready":"unsupported";
    status.textContent=all?"Required browser APIs detected; actual connection still needs verification.":"Required APIs missing. Update Android System WebView or your browser; no security downgrade is available.";
    try{
        const r=await fetch("/view-config.json",{cache:"no-store"});const v=await r.json();
        const configured=r.ok&&v.schema===1&&v.mode==="direct-read-only"&&typeof v.endpoint==="string";
        if(configured&&all){open.hidden=false;policy.textContent="Operator endpoint configured. Independent identity verification and host approval required.";}
        else {open.hidden=true;policy.textContent="No enabled compatible connection configuration. Identity management is local only.";}
    }catch{open.hidden=true;policy.textContent="No connection configuration. Identity management is local only.";}
}
run().catch(()=>{status.dataset.state="error";status.textContent="Compatibility probe failed; no connection attempted.";open.hidden=true;});
