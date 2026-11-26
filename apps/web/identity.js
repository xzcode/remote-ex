// Explicit, origin-local browser controller identity. No registration,
// host capability, export of private material, or network requests.
import {hex,sha,newFixtureIdentity,random,text,concat} from "./crypto.js";
const DB="remote-ex-controller-v1", STORE="identity", SLOT="controller";
function require(ok,msg){if(!ok)throw Error(msg);}
function open(){
    require(globalThis.isSecureContext&&globalThis.indexedDB&&globalThis.crypto?.subtle,"Secure context, WebCrypto and IndexedDB required");
    return new Promise((resolve,reject)=>{
        let blocked=false;
        const request=indexedDB.open(DB,1);
        request.onupgradeneeded=()=>request.result.createObjectStore(STORE);
        request.onblocked=()=>{blocked=true;reject(Error("Identity store is blocked by another tab"));};
        request.onerror=()=>reject(Error("Cannot open identity store"));
        request.onsuccess=()=>{
            const db=request.result;
            db.onversionchange=()=>db.close();
            if(blocked)db.close();else resolve(db);
        };
    });
}
async function operation(mode,action){
    const db=await open();
    try{
        return await new Promise((resolve,reject)=>{
            const tx=db.transaction(STORE,mode);
            let req;
            tx.oncomplete=()=>resolve(req.result);
            tx.onabort=()=>reject(Error("Identity transaction aborted; no change committed"));
            tx.onerror=()=>{}; // abort handles failures, including a competing add
            try{req=action(tx.objectStore(STORE));}
            catch(e){try{tx.abort();}catch{}reject(e);}
        });
    }finally{db.close();}
}
async function validate(record){
    require(record&&record.version===1&&typeof record.id==="string"&&/^[0-9a-f]{64}$/.test(record.id),"Invalid identity record");
    const {privateKey,publicKey}=record;
    require(privateKey instanceof CryptoKey&&privateKey.type==="private"&&!privateKey.extractable
        &&privateKey.algorithm.name==="Ed25519"&&privateKey.usages.length===1&&privateKey.usages[0]==="sign","Unsafe stored private key");
    require(publicKey instanceof CryptoKey&&publicKey.type==="public"&&publicKey.algorithm.name==="Ed25519"
        &&publicKey.usages.length===1&&publicKey.usages[0]==="verify","Invalid stored public key");
    const pub=new Uint8Array(await crypto.subtle.exportKey("raw",publicKey));
    require(pub.length===32&&hex(await sha(pub))===record.id,"Stored identity ID mismatch");
    // A fresh domain-separated local proof detects mismatched key pairs.
    const challenge=concat(text("Remote EX/browser/local-vault-check/v1\0"),random(32));
    const sig=await crypto.subtle.sign("Ed25519",privateKey,challenge);
    require(await crypto.subtle.verify("Ed25519",publicKey,sig,challenge),"Stored identity pair mismatch");
    return {id:record.id,pub,key:{privateKey,publicKey}};
}
export async function readIdentity(){
    const r=await operation("readonly",store=>store.get(SLOT));
    return r===undefined?null:validate(r);
}
export async function createIdentity(){
    // add (not put) guarantees that another tab cannot replace an existing
    // controller identity. Explicit deletion is required before regeneration.
    const {key,pub}=await newFixtureIdentity();
    const r={version:1,id:hex(await sha(pub)),privateKey:key.privateKey,publicKey:key.publicKey};
    const identity=await validate(r);
    await operation("readwrite",store=>store.add(r,SLOT));
    return identity;
}
export async function deleteIdentity(){
    await operation("readwrite",store=>store.delete(SLOT));
}
