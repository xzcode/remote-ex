// CI-only Chromium controller for the independently running, inert Rust
// routed_diagnostic host. No real desktop capture, user input event hooks,
// configurable public endpoint, file operations or arbitrary input API.
import init,{FixturePeer} from "./pkg/noise.js";
import {readIdentity,createIdentity} from "./identity.js";
import {text,concat,hex,sha,random,unhex,equal,sdpFingerprint,dtlsFingerprint} from "./crypto.js";
import {Socket,register,sendSignal,sendSealed,proofMessage,sdpProof,wait,checkEndpoint,require,nonzero,type,keys} from "./wss-fixture.js";
const READY=text("remote-ex-routed-test/ready/v1");
const READY_ACK=text("remote-ex-routed-test/ready-ack/v1");
const STOP=text("remote-ex-routed-test/stop/v1");
const RELEASED=text("remote-ex-routed-test/released/v1");
function context(){
    require(window.isSecureContext&&location.hostname==="127.0.0.1","Disposable local browser required");
}
export async function prepare(){
    context();
    const identity=await createIdentity(); // fresh isolated browser context
    return {id:identity.id,key:hex(identity.pub)};
}
function fixedInertRecords(sid){
    // Same two synthetic protobuf messages as native controller's existing
    // test, with its exact sequence, version and bound session ID.
    const header=concat(Uint8Array.of(8,1,18,16),sid);
    return [
        concat(header,Uint8Array.of(24,1,82,5,8,100,16,200,1)),
        concat(header,Uint8Array.of(24,2,98,4,8,4,16,1))
    ];
}
function mediaShape(sdp){
    const m=sdp.split(/\r?\n/).filter(l=>l.startsWith("m="));
    require(m.length===2&&m[0].startsWith("m=video ")&&m[1].startsWith("m=application "),"Expected video and negotiated data-only SDP");
    require(/(?:^|\r?\n)a=rtpmap:96 VP8\/90000(?:\r?\n|$)/i.test(sdp),"Expected native VP8 profile");
}
function packet(m,session){
    const wrapper=type(m,"signal",["session","payload"]);
    require(wrapper.session===session,"Native fixture route mismatch");
    const value=type(wrapper.payload,"sealed",["packet"]).packet;
    require(typeof value==="string"&&value.length>=34&&value.length<=65568&&value.length%2===0,"Invalid encrypted native record size");
    return unhex(value,value.length/2);
}
async function iceConfig(socket,session){
    socket.send({type:"ice_config",session});
    const pending=[];
    // The native offer can race the ICE-config response. Preserve the exact
    // ciphertext order, without parsing or opening anything early.
    for(let i=0;i<5;i++){
        const message=await socket.receive(15000);
        if(message?.type==="ice_config"){
            const m=type(message,"ice_config",["session","config"]);
            require(m.session===session&&keys(m.config,["stun","turn"])&&m.config.stun===null&&m.config.turn===null,"External ICE forbidden in this fixture");
            return pending;
        }
        require(pending.length<4,"Native ICE config queue limit");
        pending.push(packet(message,session));
    }
    throw Error("Missing native ICE configuration");
}
export async function run(endpoint,serverId,hostKey,expectedControllerId){
    context();checkEndpoint(endpoint);nonzero(serverId,32);
    const host={pub:nonzero(hostKey,32)};host.id=hex(await sha(host.pub));
    // Re-read and verify the non-extractable local key after navigation/reload.
    const controller=await readIdentity();
    require(controller&&controller.id===expectedControllerId&&controller.id!==host.id,"Unexpected stored controller identity");
    await init();
    const abort=new AbortController(),signal=abort.signal;
    const deadline=setTimeout(()=>abort.abort(),60000);
    let c,peer,pc,dc,received,video,session;
    const frames=[];
    const cleanup=()=>{
        received?.getTracks().forEach(t=>t.stop());
        if(video){video.pause();video.srcObject=null;}
        try{dc?.close();}catch{}
        try{pc?.close();}catch{}
    };
    const hidden=()=>{if(document.hidden)abort.abort();};
    const leaving=()=>abort.abort();
    document.addEventListener("visibilitychange",hidden);
    window.addEventListener("pagehide",leaving,{once:true});
    signal.addEventListener("abort",cleanup,{once:true});
    const guard=()=>require(!signal.aborted&&c&&!c.failed&&c.queue.length===0,"Native fixture canceled or unexpected signaling record");
    try{
        c=new Socket(endpoint,signal);
        await register(c,serverId,controller,true);
        const cn=random(32);
        c.send({type:"request",target:host.id,requested:3,controller_nonce:hex(cn)});
        const req=type(await c.receive(15000),"requested",["session","target"]);
        session=req.session;const sid=nonzero(session,16);
        require(req.target===host.id,"Native fixture target mismatch");
        const decision=type(await c.receive(15000),"decision",["session","grant","host_key","host_nonce"]);
        require(decision.session===session&&decision.grant===3&&decision.host_key===hostKey,"Native host key or grant mismatch");
        const hn=nonzero(decision.host_nonce,32);
        peer=new FixturePeer(true,sid,controller.pub,host.pub,cn,hn);peer.enable_native_order();
        sendSignal(c,session,{type:"noise_handshake",step:1,packet:hex(peer.write_handshake())});
        const wrapper=type(await c.receive(15000),"signal",["session","payload"]);
        require(wrapper.session===session,"Native handshake route mismatch");
        const two=type(wrapper.payload,"noise_handshake",["step","packet"]);
        require(two.step===2,"Native handshake step mismatch");peer.read_handshake(unhex(two.packet,48));
        sendSealed(c,session,peer.send_proof(await proofMessage(controller,peer.signature_message())));
        peer.receive_proof(packet(await c.receive(15000),session));
        require(peer.ready(),"Native route signature failed");
        sendSealed(c,session,peer.send_quality());
        peer.receive_quality(packet(await c.receive(15000),session));
        const pending=await iceConfig(c,session);
        const sealed=async()=>pending.length?pending.shift():packet(await c.receive(20000),session);
        const offer=peer.receive_sdp(await sealed());mediaShape(offer);
        const hf=sdpFingerprint(offer);
        pc=new RTCPeerConnection({iceServers:[],bundlePolicy:"max-bundle"});
        // Externally negotiated channel matches the native GStreamer owner.
        dc=pc.createDataChannel("remote-ex-control",{negotiated:true,id:0,ordered:true,protocol:"remote-ex-input-v1"});
        dc.binaryType="arraybuffer";
        const messages=[],candidates=[];
        dc.onmessage=e=>{
            if(!(e.data instanceof ArrayBuffer)||e.data.byteLength===0||e.data.byteLength>1024||messages.length>=4){abort.abort();return;}
            messages.push(new Uint8Array(e.data));
        };
        dc.onerror=()=>abort.abort();
        pc.ondatachannel=e=>{e.channel.close();abort.abort();};
        pc.ontrack=e=>{
            if(e.track.kind!=="video"||received){e.track.stop();abort.abort();return;}
            received=new MediaStream([e.track]);
        };
        pc.onicecandidate=e=>{
            if(!e.candidate?.candidate)return;
            if(e.candidate.sdpMLineIndex==null||candidates.length>=128){abort.abort();return;}
            candidates.push({mline:e.candidate.sdpMLineIndex,candidate:e.candidate.candidate});
        };
        await pc.setRemoteDescription({type:"offer",sdp:offer});
        const tr=pc.getTransceivers().filter(t=>t.receiver.track.kind==="video");
        require(tr.length===1,"Missing one native video transceiver");tr[0].direction="recvonly";
        const answer=await pc.createAnswer();mediaShape(answer.sdp);
        const cf=sdpFingerprint(answer.sdp);require(cf!==hf,"Expected distinct DTLS keys");
        await pc.setLocalDescription(answer);
        require(!signal.aborted,"Canceled during SDP");
        sendSealed(c,session,peer.send_sdp(answer.sdp));
        peer.receive_sdp_proof(await sealed());
        require(!peer.sdp_verified(),"Premature SDP verification");
        const cs=await sdpProof(controller,1,sid,controller,host,cn,hn,offer,answer.sdp);
        sendSealed(c,session,peer.send_sdp_proof(cs));
        require(peer.sdp_verified(),"Native SDP signature verification failed");
        await wait(signal,()=>pc.iceGatheringState==="complete",15000,"Browser ICE gathering timed out");
        require(candidates.length>0,"No local browser ICE");
        sendSealed(c,session,peer.send_ice_batch(JSON.stringify(candidates)));
        const remoteIce=JSON.parse(peer.receive_ice_batch(await sealed()));
        // GStreamer 1.24 may emit explicit empty end-of-candidates records.
        // Apply actual candidates first, then mark the respective m-lines.
        for(const item of remoteIce.filter(i=>i.candidate))await pc.addIceCandidate({sdpMLineIndex:item.mline,candidate:item.candidate});
        for(const item of remoteIce.filter(i=>!i.candidate))await pc.addIceCandidate({sdpMLineIndex:item.mline,candidate:""});
        require(peer.native_ice_verified()&&pending.length===0,"Native ICE exchange incomplete");
        await wait(signal,()=>pc.connectionState==="connected"&&dc.readyState==="open",15000,"Native media/data transport did not connect");
        await wait(signal,()=>dtlsFingerprint(pc,cf,hf),5000,"Native DTLS certificate stats unavailable");
        function send(data){guard();require(dc.readyState==="open"&&dc.bufferedAmount+data.length<=65536,"Inert channel unavailable");dc.send(data);}
        async function response(expected){
            await wait(signal,()=>{guard();return messages.length>0;},5000,"Native inert channel acknowledgement timed out");
            require(equal(messages.shift(),expected),"Unexpected native inert acknowledgement");
        }
        send(READY);await response(READY_ACK);
        for(const inert of fixedInertRecords(sid))send(inert);
        await wait(signal,()=>{guard();return !!received;},3000,"Native video track missing");
        video=document.getElementById("remote");require(video,"Missing video fixture element");
        video.srcObject=received;video.muted=true;video.playsInline=true;await video.play();
        let decoded=0;
        await wait(signal,async()=>{
            guard();
            const stats=await pc.getStats();
            const r=[...stats.values()].find(v=>v.type==="inbound-rtp"&&(v.kind==="video"||v.mediaType==="video"));
            decoded=r?.framesDecoded??0;
            return decoded>=30&&video.videoWidth===960&&video.videoHeight===540;
        },8500,"Native VP8 frames missing or wrong dimensions");
        const sample=document.createElement("canvas");sample.width=960;sample.height=540;
        const ctx=sample.getContext("2d",{willReadFrequently:true});require(ctx,"No pixel verification context");
        function pattern(){
            ctx.drawImage(video,0,0);
            const data=ctx.getImageData(0,0,960,540).data;
            let nonblack=0,blue=0;
            for(let i=0;i<data.length;i+=40){if(data[i]+data[i+1]+data[i+2]>40)nonblack++;if(data[i+2]>65&&data[i+2]<175)blue++;}
            return nonblack>1000&&blue>1000;
        }
        require(pattern(),"Native generated pixel pattern absent");
        frames.push(decoded);
        // Verify host's inert dispatcher released the held key; no local
        // keyboard/mouse listeners are registered, and no OS input is possible.
        send(STOP);await response(RELEASED);
        c.send({type:"close",session});
        const closed=type(await c.receive(15000),"closed",["session","reason"]);
        require(closed.session===session&&closed.reason==="closed","Native route closure mismatch");
        return {storedIdentity:true,nativeRoute:true,quality:true,ice:true,dtls:true,video:true,frames:frames[0],release:true,closed:true};
    }finally{
        clearTimeout(deadline);abort.abort();cleanup();c?.close();
        if(peer){try{peer.close();}finally{peer.free();}}
        document.removeEventListener("visibilitychange",hidden);
        window.removeEventListener("pagehide",leaving);
    }
}
