// Fixed, synthetic-only lab. Native WebCrypto handles identity signatures.
// The Rust WASM module handles the existing Snow Noise state and proof gate.
// No storage, remote address, network session, or arbitrary signing UI.
const encoder=new TextEncoder();
export const text=s=>encoder.encode(s);
export function concat(...parts){const size=parts.reduce((n,p)=>n+p.length,0);const out=new Uint8Array(size);let i=0;for(const p of parts){out.set(p,i);i+=p.length;}return out;}
export const hex=b=>Array.from(b,x=>x.toString(16).padStart(2,"0")).join("");
export function unhex(s,n){if(typeof s!=="string"||s.length!==2*n||!/^[0-9a-f]+$/.test(s))throw Error("Invalid fixture hex");return Uint8Array.from(s.match(/../g),x=>parseInt(x,16));}
export const sha=async b=>new Uint8Array(await crypto.subtle.digest("SHA-256",b));
export const equal=(a,b)=>a.length===b.length&&a.every((v,i)=>v===b[i]);
function assert(ok,message){if(!ok)throw Error(message);}
async function verify(key,body,sig){return crypto.subtle.verify("Ed25519",key,sig,body);}
async function sign(key,body){return new Uint8Array(await crypto.subtle.sign("Ed25519",key,body));}
export async function publicVectors(v){
    assert(v?.schema===1&&v.fixture_only===true,"Not a public synthetic fixture");
    const b=(name,n)=>unhex(v[name],n), c=b("controller_key",32), h=b("host_key",32);
    const controller=await crypto.subtle.importKey("raw",c,"Ed25519",false,["verify"]);
    const host=await crypto.subtle.importKey("raw",h,"Ed25519",false,["verify"]);
    assert(hex(await sha(c))===v.controller_id,"Fixture device ID mismatch");
    const reg=concat(text("Remote EX/register-proof/v1\0"),b("server_id",32),b("challenge",32));
    const ctl=concat(text("Remote EX/register-controller-only/v1\0"),b("server_id",32),b("challenge",32));
    assert(await verify(controller,reg,b("registration_signature",64)),"Registration vector mismatch");
    assert(await verify(controller,ctl,b("controller_registration_signature",64)),"Controller vector mismatch");
    assert(!await verify(controller,ctl,b("registration_signature",64)),"Registration scope not separated");
    const body=concat(b("session_id",16),c,h,b("controller_nonce",32),b("host_nonce",32),Uint8Array.of(3,3));
    const ctx=concat(text("Remote EX/routed-noise/v1\0"),body,Uint8Array.of(0,0));
    assert(hex(ctx)===v.route_context,"Route vector context mismatch");
    const route=role=>concat(ctx,Uint8Array.of(role),b("noise_hash",32));
    assert(await verify(controller,route(1),b("controller_route_signature",64)),"Controller route proof mismatch");
    assert(await verify(host,route(2),b("host_route_signature",64)),"Host route proof mismatch");
    const altered=route(1);altered[ctx.length-1]^=1;
    assert(!await verify(controller,altered,b("controller_route_signature",64)),"Changed route proof accepted");
    const sdp=async role=>concat(text("Remote EX/session-transcript/v1\0"),Uint8Array.of(role),body,await sha(text(v.offer)),await sha(text(v.answer)));
    assert(await verify(controller,await sdp(1),b("controller_sdp_signature",64)),"Controller SDP fixture mismatch");
    assert(await verify(host,await sdp(2),b("host_sdp_signature",64)),"Host SDP fixture mismatch");
}
export async function newFixtureIdentity(){
    const key=await crypto.subtle.generateKey("Ed25519",false,["sign","verify"]);
    assert(!key.privateKey.extractable,"Private key must be non-extractable");
    const pub=new Uint8Array(await crypto.subtle.exportKey("raw",key.publicKey));
    assert(pub.length===32,"Unexpected identity key length");
    return {key,pub};
}
export function random(n){return crypto.getRandomValues(new Uint8Array(n));}
export async function authenticateNoise(FixturePeer,controller,host,session,cn,hn){
    let c,h;
    try{
        c=new FixturePeer(true,session,controller.pub,host.pub,cn,hn);
        h=new FixturePeer(false,session,controller.pub,host.pub,cn,hn);
        h.read_handshake(c.write_handshake());
        c.read_handshake(h.write_handshake());
        assert(!c.ready()&&!h.ready(),"Fixture proof gate bypassed");
        const cMessage=c.signature_message(),hMessage=h.signature_message();
        // Both roles use one exact Noise hash and one exact context.
        assert(equal(cMessage.slice(-32),hMessage.slice(-32)),"Noise hashes differ");
        const cs=await sign(controller.key.privateKey,cMessage);
        const hs=await sign(host.key.privateKey,hMessage);
        const cp=c.send_proof(cs),hp=h.send_proof(hs);
        c.receive_proof(hp);h.receive_proof(cp);
        assert(c.ready()&&h.ready(),"Mutual fixture authentication failed");
        const packet=c.send_fixture();h.receive_fixture(packet);c.receive_fixture(h.send_fixture());
        // A replay must close the receiving Noise state.
        let rejected=false;try{h.receive_fixture(packet);}catch{rejected=true;}
        assert(rejected&&!h.ready(),"Replayed Noise record was accepted");
    }finally{
        for(const peer of [c,h])if(peer){try{peer.close();}finally{peer.free();}}
    }
}
export async function signedSdpPair(controller,host,session,cn,hn,offer,answer){
    assert(typeof offer==="string"&&typeof answer==="string"&&offer.length>0&&answer.length>0,"Invalid SDP fixture");
    const ob=text(offer),ab=text(answer);
    assert(ob.length<=32768&&ab.length<=32768&&!offer.includes("\0")&&!answer.includes("\0"),"SDP fixture exceeds bound");
    const body=concat(session,controller.pub,host.pub,cn,hn,Uint8Array.of(3,3),await sha(ob),await sha(ab));
    const context=role=>concat(text("Remote EX/session-transcript/v1\0"),Uint8Array.of(role),body);
    const cm=context(1),hm=context(2);
    const cs=await sign(controller.key.privateKey,cm),hs=await sign(host.key.privateKey,hm);
    assert(await verify(controller.key.publicKey,cm,cs)&&await verify(host.key.publicKey,hm,hs),"Actual SDP proofs failed");
    const changed=cm.slice();changed[changed.length-1]^=1;
    assert(!await verify(controller.key.publicKey,changed,cs),"Changed SDP proof accepted");
}
// A fixture has one certificate per peer. Reject mixed/malformed fingerprints,
// including a weaker algorithm. Only the exact SHA-256 value is compared.
export function sdpFingerprint(sdp){
    const lines=sdp.split(/\r?\n/).filter(l=>l.startsWith("a=fingerprint:"));
    assert(lines.length>0&&lines.length<=8,"Missing or excessive SDP fingerprints");
    const values=lines.map(l=>{
        const m=/^a=fingerprint:sha-256 ((?:[0-9a-f]{2}:){31}[0-9a-f]{2})$/i.exec(l);
        assert(m,"Invalid SDP SHA-256 fingerprint");return m[1].replaceAll(":","").toLowerCase();
    });
    assert(values.every(x=>x===values[0]),"Conflicting SDP fingerprints");return values[0];
}
export async function dtlsFingerprint(pc,local,remote){
    const stats=await pc.getStats();
    const active=[...stats.values()].filter(s=>s.type==="transport"&&s.dtlsState==="connected");
    if(active.length!==1)return false;
    const t=active[0],lc=stats.get(t.localCertificateId),rc=stats.get(t.remoteCertificateId);
    const value=s=>{
        if(s?.type!=="certificate"||s.fingerprintAlgorithm?.toLowerCase()!=="sha-256"||!/^([0-9a-f]{2}:){31}[0-9a-f]{2}$/i.test(s.fingerprint??""))return null;
        return s.fingerprint.replaceAll(":","").toLowerCase();
    };
    if(!lc||!rc)return false;
    assert(value(lc)===local&&value(rc)===remote,"Negotiated DTLS certificate differs from signed SDP");
    return true;
}
