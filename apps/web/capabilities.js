// Offline capability checks. The only fetched resource is bundled WASM.
// No WSS, ICE gathering, camera, microphone or persistent device enrollment.
import {newFixtureIdentity,random,concat,text} from "./crypto.js";
import {loadControllerWasm,WasmLoadError} from "./wasm-loader.js";
export const CAPABILITY_IDS=Object.freeze(["secure","ed25519","wasm","storage","webrtc","pointer"]);
const labels=["Secure context","Ed25519 non-extractable key","WASM session core","IndexedDB","WebRTC VP8/data channel","Pointer events"];
const timeout=(promise,ms,code)=>new Promise((resolve,reject)=>{
    const timer=setTimeout(()=>reject(new Error(code)),ms);
    Promise.resolve(promise).then(v=>{clearTimeout(timer);resolve(v);},e=>{clearTimeout(timer);reject(e);});
});
function code(error,fallback){
    if(error instanceof WasmLoadError&&/^wasm_[a-z]+$/.test(error.code))return error.code;
    if(typeof error?.message==="string"&&/^[a-z_]{1,40}$/.test(error.message))return error.message;
    if(["NotSupportedError","SecurityError","DataCloneError","InvalidAccessError","OperationError","QuotaExceededError","AbortError"].includes(error?.name))return error.name;
    return fallback;
}
async function identityProbe(){
    const identity=await newFixtureIdentity();
    const challenge=concat(text("Remote EX/mobile/capability/v1\0"),random(32));
    const signature=await crypto.subtle.sign("Ed25519",identity.key.privateKey,challenge);
    if(signature.byteLength!==64||!await crypto.subtle.verify("Ed25519",identity.key.publicKey,signature,challenge))throw Error("ed25519_proof");
    let refused=false;
    try{await crypto.subtle.exportKey("pkcs8",identity.key.privateKey);}catch{refused=true;}
    if(!refused)throw Error("ed25519_export");
    return {identity,challenge,signature};
}
async function storageProbe(proof){
    if(!globalThis.indexedDB)throw Error("storage_api");
    // Separate disposable probe store. Add/read/delete occur in one atomic
    // transaction; no identity material remains even if the page closes.
    const slot=Array.from(random(16),v=>v.toString(16).padStart(2,"0")).join("");
    const db=await new Promise((resolve,reject)=>{
        const req=indexedDB.open("remote-ex-capability-probe-v1",1);
        let expired=false;
        const timer=setTimeout(()=>{expired=true;reject(Error("storage_open_timeout"));},3500);
        req.onupgradeneeded=()=>{if(!req.result.objectStoreNames.contains("probe"))req.result.createObjectStore("probe");};
        req.onblocked=()=>{expired=true;clearTimeout(timer);reject(Error("storage_blocked"));};
        req.onerror=()=>{clearTimeout(timer);reject(req.error??Error("storage_open"));};
        req.onsuccess=()=>{clearTimeout(timer);if(expired)req.result.close();else resolve(req.result);};
    });
    try{
        const value=await new Promise((resolve,reject)=>{
            const tx=db.transaction("probe","readwrite"),store=tx.objectStore("probe");
            let result;
            const timer=setTimeout(()=>{try{tx.abort();}catch{}reject(Error("storage_transaction_timeout"));},3500);
            tx.oncomplete=()=>{clearTimeout(timer);resolve(result);};
            tx.onabort=()=>{clearTimeout(timer);reject(tx.error??Error("storage_abort"));};
            tx.onerror=()=>{};
            try{
                store.add(proof?proof.identity.key:{marker:slot},slot);
                const read=store.get(slot);
                read.onsuccess=()=>{result=read.result;store.delete(slot);};
            }catch(e){clearTimeout(timer);try{tx.abort();}catch{}reject(e);}
        });
        if(proof){
            if(!value?.privateKey||value.privateKey.extractable||value.privateKey.algorithm.name!=="Ed25519")throw Error("storage_key_clone");
            const sig=await crypto.subtle.sign("Ed25519",value.privateKey,proof.challenge);
            if(!await crypto.subtle.verify("Ed25519",proof.identity.key.publicKey,sig,proof.challenge))throw Error("storage_key_proof");
        }else if(value?.marker!==slot)throw Error("storage_roundtrip");
        return proof?"ok_key_clone":"ok_storage_only";
    }finally{db.close();}
}
async function rtcProbe(){
    if(typeof RTCPeerConnection!=="function")throw Error("webrtc_api");
    const pc=new RTCPeerConnection({iceServers:[],bundlePolicy:"max-bundle"});
    try{
        pc.addTransceiver("video",{direction:"recvonly"});
        const dc=pc.createDataChannel("remote-ex-capability",{negotiated:true,id:0,ordered:true,protocol:"remote-ex-input-v1"});
        try{
            const offer=await timeout(pc.createOffer(),3500,"webrtc_offer_timeout");
            // No setLocalDescription: never gather or transmit ICE candidates.
            if(!/^a=rtpmap:\d+ VP8\/90000\r?$/mi.test(offer.sdp)||!/^m=application /m.test(offer.sdp))throw Error("webrtc_vp8_data");
            const stats=await timeout(pc.getStats(),2500,"webrtc_stats_timeout");
            if(typeof stats?.values!=="function")throw Error("webrtc_stats");
        }finally{dc.close();}
    }finally{pc.close();}
}
export async function probeCapabilities({loadModule=()=>import("./pkg/noise.js")}={}){
    let proof=null;
    const checks=[];
    async function check(id,fn,fallback){
        const label=labels[CAPABILITY_IDS.indexOf(id)];
        try{const detail=await fn();checks.push({id,label,ok:true,code:typeof detail==="string"?detail:"ok"});}
        catch(e){checks.push({id,label,ok:false,code:code(e,fallback)});}
    }
    await check("secure",()=>{if(!globalThis.isSecureContext||!globalThis.crypto?.subtle)throw Error("secure_context");},"secure_context");
    await check("ed25519",async()=>{proof=await timeout(identityProbe(),4000,"ed25519_timeout");},"ed25519_failed");
    await check("wasm",async()=>{
        let wasm;try{wasm=await timeout(loadModule(),4000,"wasm_module_timeout");}catch(e){throw new WasmLoadError(e?.message==="wasm_module_timeout"?"wasm_module_timeout":"wasm_module");}
        await timeout(loadControllerWasm(wasm.default),5000,"wasm_load_timeout");
        if(typeof wasm.ViewPeer!=="function")throw Error("wasm_exports");
    },"wasm_failed");
    await check("storage",()=>storageProbe(proof),"storage_failed");
    await check("webrtc",rtcProbe,"webrtc_failed");
    await check("pointer",()=>{if(typeof PointerEvent!=="function"||typeof Element.prototype.setPointerCapture!=="function")throw Error("pointer_api");},"pointer_failed");
    return checks;
}

/** Only fixed diagnostic codes, not browser exception text or identity data. */
export function capabilityBlockers(checks){
    if(!Array.isArray(checks)||checks.length!==CAPABILITY_IDS.length||
        checks.some((v,i)=>!v||v.id!==CAPABILITY_IDS[i]||typeof v.ok!=="boolean"||
            typeof v.code!=="string"||!/^[A-Za-z_]{1,40}$/.test(v.code))){
        return ["capability_report_invalid"];
    }
    return checks.filter(v=>!v.ok).map(v=>v.id+": "+v.code);
}
