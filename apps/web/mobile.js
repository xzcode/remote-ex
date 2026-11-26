import {newFixtureIdentity} from "./crypto.js";
const status=document.getElementById("mobile-status"),checks=document.getElementById("mobile-checks");
function check(label,pass){const item=document.createElement("li");item.textContent=label+": "+(pass?"available":"unavailable");if(pass)item.dataset.pass="true";checks.append(item);return pass;}
async function run(){
    let cryptoOk=false,wasmOk=false;
    try{const identity=await newFixtureIdentity();cryptoOk=!identity.key.privateKey.extractable;}catch{}
    try{const wasm=await import("./pkg/noise.js");await wasm.default();wasmOk=typeof wasm.ViewPeer==="function";}catch{}
    const results=[check("Secure context",window.isSecureContext),check("Ed25519 non-extractable key",cryptoOk),check("WASM session core",wasmOk),check("IndexedDB",!!globalThis.indexedDB),check("WebRTC",typeof RTCPeerConnection==="function"),check("Pointer events",typeof PointerEvent==="function")];
    status.textContent=results.every(Boolean)?"Required browser APIs detected; actual connection still needs verification.":"Required APIs missing. Update your browser or WebView; no downgrade is available.";
    try{
        const r=await fetch("/view-config.json",{cache:"no-store"});const v=await r.json();
        const configured=r.ok&&v.schema===1&&v.mode==="direct-read-only"&&typeof v.endpoint==="string";
        if(configured&&results.every(Boolean)){document.getElementById("mobile-connect").hidden=false;document.getElementById("mobile-policy").textContent="Operator endpoint configured. Independent identity verification and host approval required.";}
        else document.getElementById("mobile-policy").textContent="No enabled compatible connection configuration. Identity management is local only.";
    }catch{document.getElementById("mobile-policy").textContent="No connection configuration. Identity management is local only.";}
}
run().catch(()=>{status.textContent="Compatibility probe failed; no connection attempted.";});
