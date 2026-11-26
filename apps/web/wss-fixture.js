// Disposable integration fixture only. Not served by the normal app or shipped
// in its archive. Uses generated same-page identities, exact local WSS and
// profile 0. No arbitrary endpoint, captured desktop, input or data transfer.
import init,{FixturePeer} from "./pkg/noise.js";
import {newFixtureIdentity,random,hex,unhex,sha,text,concat,sdpFingerprint,dtlsFingerprint} from "./crypto.js";
const MAX=96*1024;
let identities=null;
function require(ok,msg){if(!ok)throw Error(msg);}
function keys(value,names){
    return value&&typeof value==="object"&&!Array.isArray(value)&&Object.keys(value).sort().join(",")===names.slice().sort().join(",");
}
function type(m,name,fields){
    require(keys(m,["type",...fields])&&m.type===name,"Unexpected "+name+" response");return m;
}
function nonzero(value,n){
    const bytes=unhex(value,n);require(bytes.some(x=>x!==0),"Zero fixture field");return bytes;
}
function checkEndpoint(value){
    const u=new URL(value);
    require(u.protocol==="wss:"&&u.hostname==="127.0.0.1"&&u.port&&u.pathname==="/v1/ws"&&!u.username&&!u.password&&!u.search&&!u.hash&&u.href===value,"Only exact local WSS fixture endpoint permitted");
    return value;
}
class Socket{
    constructor(endpoint,signal){
        this.socket=new WebSocket(checkEndpoint(endpoint));this.queue=[];this.waiter=null;this.failed=false;
        this.onAbort=()=>this.fail("Fixture canceled");this.signal=signal;
        signal.addEventListener("abort",this.onAbort,{once:true});
        if(signal.aborted){this.fail("Fixture canceled");return;}
        this.socket.onmessage=e=>{
            if(this.failed)return;
            try{
                require(typeof e.data==="string"&&e.data.length<=MAX&&text(e.data).length<=MAX,"Invalid WSS frame size");
                const frame=JSON.parse(e.data);require(keys(frame,["version","message"])&&frame.version===1,"Invalid signaling envelope");
                if(this.waiter){const w=this.waiter;this.waiter=null;clearTimeout(w.timer);w.resolve(frame.message);}
                else{require(this.queue.length<8,"WSS queue limit reached");this.queue.push(frame.message);}
            }catch{this.fail("Invalid WSS response");}
        };
        this.socket.onerror=()=>this.fail("WSS/TLS connection refused");
        this.socket.onclose=()=>this.fail("WSS transport closed");
    }
    fail(message){
        if(this.failed)return;this.failed=true;
        if(this.waiter){const w=this.waiter;this.waiter=null;clearTimeout(w.timer);w.reject(Error(message));}
        this.queue.length=0;this.signal.removeEventListener("abort",this.onAbort);
        try{this.socket.close();}catch{}
    }
    close(){this.fail("WSS fixture closed");}
    receive(timeout=7000){
        if(this.failed)return Promise.reject(Error("WSS fixture unavailable"));
        if(this.queue.length)return Promise.resolve(this.queue.shift());
        if(this.waiter)return Promise.reject(Error("Concurrent WSS receive"));
        return new Promise((resolve,reject)=>{
            const timer=setTimeout(()=>this.fail("WSS fixture response timeout"),timeout);
            this.waiter={resolve,reject,timer};
        });
    }
    send(message){
        require(!this.failed&&this.socket.readyState===WebSocket.OPEN,"WSS not connected");
        const json=JSON.stringify({version:1,message});
        require(text(json).length<=MAX&&this.socket.bufferedAmount<=2*MAX,"WSS write bound");
        this.socket.send(json);
    }
}
async function register(socket,expected,identity,controller){
    const ch=type(await socket.receive(),"challenge",["server","nonce"]);
    const server=nonzero(ch.server,32),nonce=nonzero(ch.nonce,32);
    require(ch.server===expected,"Configured server identity mismatch");
    const prefix=controller?"Remote EX/register-controller-only/v1\0":"Remote EX/register-proof/v1\0";
    const sig=new Uint8Array(await crypto.subtle.sign("Ed25519",identity.key.privateKey,concat(text(prefix),server,nonce)));
    socket.send({type:controller?"register_controller":"register",public_key:hex(identity.pub),signature:hex(sig)});
    const m=type(await socket.receive(),"registered",["device_id"]);
    require(m.device_id===identity.id,"Registration ID mismatch");
}
function sendSignal(socket,session,payload){socket.send({type:"signal",session,payload});}
async function receiveSignal(socket,session,name,fields){
    const m=type(await socket.receive(),"signal",["session","payload"]);
    require(m.session===session,"Signaling route mismatch");
    return type(m.payload,name,fields);
}
async function sealed(socket,session){
    const m=await receiveSignal(socket,session,"sealed",["packet"]);
    require(typeof m.packet==="string"&&m.packet.length>=34&&m.packet.length<=65568&&m.packet.length%2===0,"Invalid sealed frame");
    return unhex(m.packet,m.packet.length/2);
}
function sendSealed(socket,session,packet){sendSignal(socket,session,{type:"sealed",packet:hex(packet)});}
async function proofMessage(identity,bytes){return new Uint8Array(await crypto.subtle.sign("Ed25519",identity.key.privateKey,bytes));}
async function sdpProof(identity,role,sid,controller,host,cn,hn,offer,answer){
    return proofMessage(identity,concat(text("Remote EX/session-transcript/v1\0"),Uint8Array.of(role),
        sid,controller.pub,host.pub,cn,hn,Uint8Array.of(3,3),await sha(text(offer)),await sha(text(answer))));
}
async function wait(signal,predicate,limit,label){
    const until=performance.now()+limit;
    while(performance.now()<until){
        require(!signal.aborted,"Fixture canceled");
        if(await predicate())return;
        await new Promise(resolve=>setTimeout(resolve,60));
    }throw Error(label);
}
export async function prepare(){
    require(window.isSecureContext&&location.hostname==="127.0.0.1","Local secure browser fixture only");
    require(!identities,"Fixture already prepared");
    await init();
    const controller=await newFixtureIdentity(),host=await newFixtureIdentity();
    controller.id=hex(await sha(controller.pub));host.id=hex(await sha(host.pub));
    identities={controller,host};return [controller.id,host.id];
}
export async function probe(endpoint,expected){
    nonzero(expected,32);
    const abort=new AbortController();
    const timer=setTimeout(()=>abort.abort(),8000);
    const socket=new Socket(endpoint,abort.signal);
    try{
        const message=type(await socket.receive(),"challenge",["server","nonce"]);
        nonzero(message.server,32);nonzero(message.nonce,32);
        // A mismatched independently supplied pin is rejected before proof.
        return {challengeSeen:true,matched:message.server===expected};
    }catch{return {challengeSeen:false,matched:false};}
    finally{clearTimeout(timer);abort.abort();socket.close();}
}
export async function run(endpoint,expected){
    checkEndpoint(endpoint);nonzero(expected,32);
    const prepared=identities;identities=null;require(prepared,"Fresh local fixture preparation required");
    const {controller,host}=prepared;
    const abort=new AbortController(),signal=abort.signal;
    const deadline=setTimeout(()=>abort.abort(),45000);
    const sockets=[],peers=[],cryptoPeers=[];
    let stream,received,paintTimer,video;
    let result;
    const cleanupMedia=()=>{clearInterval(paintTimer);stream?.getTracks().forEach(t=>t.stop());received?.getTracks().forEach(t=>t.stop());if(video){video.pause();video.srcObject=null;}for(const peer of peers){try{peer.close();}catch{}}};
    signal.addEventListener('abort',cleanupMedia,{once:true});
    const onHidden=()=>{if(document.hidden)abort.abort();};
    document.addEventListener("visibilitychange",onHidden);
    const onPageHide=()=>abort.abort();
    window.addEventListener("pagehide",onPageHide,{once:true});
    try{
        const c=new Socket(endpoint,signal),h=new Socket(endpoint,signal);sockets.push(c,h);
        await Promise.all([register(c,expected,controller,true),register(h,expected,host,false)]);
        const cn=random(32),hn=random(32);
        c.send({type:"request",target:host.id,requested:3,controller_nonce:hex(cn)});
        const req=type(await c.receive(),"requested",["session","target"]);
        const incoming=type(await h.receive(),"incoming",["session","controller","controller_key","requested","controller_nonce"]);
        const session=req.session,sid=nonzero(session,16);
        require(req.target===host.id&&incoming.session===session&&incoming.controller===controller.id&&incoming.controller_key===hex(controller.pub)&&incoming.requested===3&&incoming.controller_nonce===hex(cn),"Synthetic request binding mismatch");
        // Both identities and their approval originate in this disposable test.
        // No actual remote desktop, capture or input grant is created.
        h.send({type:"decision",session,grant:3,host_nonce:hex(hn)});
        const decision=type(await c.receive(),"decision",["session","grant","host_key","host_nonce"]);
        require(decision.session===session&&decision.grant===3&&decision.host_key===hex(host.pub)&&decision.host_nonce===hex(hn),"Synthetic grant mismatch");
        const cp=new FixturePeer(true,sid,controller.pub,host.pub,cn,hn),hp=new FixturePeer(false,sid,controller.pub,host.pub,cn,hn);cryptoPeers.push(cp,hp);cp.enable_native_order();hp.enable_native_order();
        sendSignal(c,session,{type:"noise_handshake",step:1,packet:hex(cp.write_handshake())});
        const one=await receiveSignal(h,session,"noise_handshake",["step","packet"]);require(one.step===1,"Wrong handshake step");hp.read_handshake(unhex(one.packet,32));
        sendSignal(h,session,{type:"noise_handshake",step:2,packet:hex(hp.write_handshake())});
        const two=await receiveSignal(c,session,"noise_handshake",["step","packet"]);require(two.step===2,"Wrong handshake step");cp.read_handshake(unhex(two.packet,48));
        sendSealed(c,session,cp.send_proof(await proofMessage(controller,cp.signature_message())));
        sendSealed(h,session,hp.send_proof(await proofMessage(host,hp.signature_message())));
        cp.receive_proof(await sealed(c,session));hp.receive_proof(await sealed(h,session));
        require(cp.ready()&&hp.ready()&&!cp.sdp_verified()&&!hp.sdp_verified(),"Noise gate failed");
        // Match the native v1 peers' explicit mutual compat quality record.
        sendSealed(c,session,cp.send_quality());sendSealed(h,session,hp.send_quality());
        cp.receive_quality(await sealed(c,session));hp.receive_quality(await sealed(h,session));
        // Existing v1 server admits ICE only after both encrypted proof records.
        // The disposable server intentionally provides no STUN/TURN services.
        c.send({type:"ice_config",session});h.send({type:"ice_config",session});
        for(const socket of [c,h]){
            const m=type(await socket.receive(),"ice_config",["session","config"]);
            require(m.session===session&&keys(m.config,["stun","turn"])&&m.config.stun===null&&m.config.turn===null,"External ICE is prohibited in fixture");
        }
        const opts={iceServers:[],bundlePolicy:"max-bundle"};
        const hostPc=new RTCPeerConnection(opts),controllerPc=new RTCPeerConnection(opts);peers.push(hostPc,controllerPc);
        // Collect actual non-trickle ICE events separately from signed SDP.
        const collect=pc=>{
            const items=[];
            pc.addEventListener("icecandidate",e=>{
                if(!e.candidate?.candidate)return;
                if(items.length>=128||e.candidate.sdpMLineIndex==null){abort.abort();return;}
                items.push({mline:e.candidate.sdpMLineIndex,candidate:e.candidate.candidate});
            });
            return items;
        };
        const hostIce=collect(hostPc),controllerIce=collect(controllerPc);
        const tr=hostPc.addTransceiver("video",{direction:"sendonly"});
        const dc=hostPc.createDataChannel("synthetic-wss-fixture",{ordered:true});
        let remoteChannel=null;
        controllerPc.ondatachannel=e=>{if(e.channel.label==="synthetic-wss-fixture")remoteChannel=e.channel;else e.channel.close();};
        let approvedMedia=false;
        controllerPc.ontrack=e=>{
            if(e.track.kind!=="video"){e.track.stop();abort.abort();return;}
            received=new MediaStream([e.track]);
        };
        const initialOffer=await hostPc.createOffer();
        await hostPc.setLocalDescription(initialOffer);
        const offer=initialOffer.sdp,hf=sdpFingerprint(offer);
        sendSealed(h,session,hp.send_sdp(offer));
        const receivedOffer=cp.receive_sdp(await sealed(c,session));require(receivedOffer===offer,"Offer changed in transit");
        await controllerPc.setRemoteDescription({type:"offer",sdp:receivedOffer});
        const remoteTr=controllerPc.getTransceivers().find(t=>t.receiver.track.kind==="video");
        require(remoteTr,"Missing video transceiver");remoteTr.direction="recvonly";
        const initialAnswer=await controllerPc.createAnswer();
        await controllerPc.setLocalDescription(initialAnswer);
        const answer=initialAnswer.sdp,cf=sdpFingerprint(answer);require(cf!==hf,"Distinct DTLS certificates required");
        sendSealed(c,session,cp.send_sdp(answer));
        const receivedAnswer=hp.receive_sdp(await sealed(h,session));require(receivedAnswer===answer,"Answer changed in transit");
        // No generated track is attached or rendered while the SDP proofs
        // are incomplete, even if WebRTC begins internal ICE/DTLS setup.
        await hostPc.setRemoteDescription({type:"answer",sdp:receivedAnswer});
        const hs=await sdpProof(host,2,sid,controller,host,cn,hn,offer,answer),cs=await sdpProof(controller,1,sid,controller,host,cn,hn,offer,answer);
        sendSealed(h,session,hp.send_sdp_proof(hs));cp.receive_sdp_proof(await sealed(c,session));
        require(!cp.sdp_verified()&&!hp.sdp_verified(),"Premature SDP proof gate");
        sendSealed(c,session,cp.send_sdp_proof(cs));hp.receive_sdp_proof(await sealed(h,session));
        require(cp.sdp_verified()&&hp.sdp_verified(),"Encrypted exact SDP proof failed");
        await wait(signal,()=>hostPc.iceGatheringState==="complete"&&controllerPc.iceGatheringState==="complete",9000,"Native-order ICE gathering timed out");
        require(hostIce.length>0&&controllerIce.length>0,"Missing real browser ICE candidates");
        sendSealed(h,session,hp.send_ice_batch(JSON.stringify(hostIce)));
        sendSealed(c,session,cp.send_ice_batch(JSON.stringify(controllerIce)));
        for(const item of JSON.parse(cp.receive_ice_batch(await sealed(c,session))))await controllerPc.addIceCandidate({sdpMLineIndex:item.mline,candidate:item.candidate});
        for(const item of JSON.parse(hp.receive_ice_batch(await sealed(h,session))))await hostPc.addIceCandidate({sdpMLineIndex:item.mline,candidate:item.candidate});
        require(cp.native_ice_verified()&&hp.native_ice_verified(),"Native-order ICE proof gate failed");
        await wait(signal,()=>hostPc.connectionState==="connected"&&controllerPc.connectionState==="connected"&&dc.readyState==="open"&&remoteChannel?.readyState==="open",9000,"Routed fixture DTLS did not connect");
        await wait(signal,async()=>await dtlsFingerprint(hostPc,hf,cf)&&await dtlsFingerprint(controllerPc,cf,hf),4000,"Negotiated certificate stats unavailable");
        require(!signal.aborted,"Fixture canceled");approvedMedia=true;
        const canvas=document.createElement("canvas");canvas.width=320;canvas.height=180;
        const ctx=canvas.getContext("2d",{alpha:false});require(ctx,"Canvas unavailable");
        let seq=0;const draw=()=>{ctx.fillStyle="#152332";ctx.fillRect(0,0,320,180);ctx.fillStyle="#35b6a2";ctx.fillRect((seq++*5)%260,60,60,60);};
        require(approvedMedia&&!signal.aborted,"Media gate closed");draw();
        stream=canvas.captureStream(15);const track=stream.getVideoTracks()[0];require(track,"Missing generated video");
        await tr.sender.replaceTrack(track);require(!signal.aborted,"Fixture canceled");paintTimer=setInterval(draw,66);
        await wait(signal,()=>!!received,3000,"Missing received synthetic track");
        video=document.getElementById("remote");require(video,"Fixture video element missing");video.muted=true;video.playsInline=true;video.srcObject=received;
        await video.play();
        let frames=0;await wait(signal,async()=>{
            const report=await controllerPc.getStats();
            const r=[...report.values()].find(v=>v.type==="inbound-rtp"&&(v.kind==="video"||v.mediaType==="video"));
            frames=r?.framesDecoded??0;
            return frames>=3&&video.videoWidth===320&&video.videoHeight===180;
        },5000,"No actual routed decoded frames");
        const sample=document.createElement("canvas");sample.width=320;sample.height=180;
        const pixel=sample.getContext("2d",{willReadFrequently:true});require(pixel,"Pixel check unavailable");
        pixel.drawImage(video,0,0);const data=pixel.getImageData(0,0,320,180).data;
        let colored=0;for(let i=0;i<data.length;i+=4)if(data[i+1]>100&&data[i+2]>70)colored++;
        require(colored>100,"Generated pattern absent");
        c.send({type:"close",session});
        for(const socket of [c,h]){const closed=type(await socket.receive(),"closed",["session","reason"]);require(closed.session===session&&closed.reason==="closed","Route close mismatch");}
        result={wss:true,registration:true,route:true,noise:true,nativeQuality:true,encryptedSdp:true,encryptedIce:true,dtls:true,frames,video:true,closed:true};
    }finally{
        clearTimeout(deadline);abort.abort();cleanupMedia();document.removeEventListener("visibilitychange",onHidden);window.removeEventListener("pagehide",onPageHide);
        for(const peer of cryptoPeers){try{peer.close();}finally{peer.free();}}
        for(const socket of sockets)socket.close();
    }
    return result;
}

// Reused by the separate disposable native diagnostic; not shipped.
export {Socket,register,sendSignal,sendSealed,proofMessage,sdpProof,wait,checkEndpoint,require,nonzero,type,keys};
