// Explicit outgoing profile-1 controller. No host, input, clipboard, files,
// screen capture, audio, recording, unattended access or automatic reconnect.
import init,{ViewPeer} from "./pkg/noise.js";
import {readIdentity} from "./identity.js";
import {text,concat,hex,sha,random,unhex,equal,sdpFingerprint,dtlsFingerprint} from "./crypto.js";
const MAX=96*1024,READY=text("remote-ex-routed-test/ready/v1"),READY_ACK=text("remote-ex-routed-test/ready-ack/v1");
const STOP=text("remote-ex-routed-test/stop/v1"),ACK=text("remote-ex-routed-test/released/v1"),HEARTBEAT=text("remote-ex-routed/heartbeat/v1"),NEXT=text("remote-ex-view/next/v1");
function need(ok,message){if(!ok)throw Error(message);}
function fields(v,names){return v&&typeof v==="object"&&!Array.isArray(v)&&Object.keys(v).sort().join(",")===names.slice().sort().join(",");}
function message(v,name,names){need(fields(v,["type",...names])&&v.type===name,"Unexpected "+name+" record");return v;}
function key(value,n){const b=unhex(value,n);need(b.some(x=>x!==0),"Zero identity or session field");return b;}
export function endpoint(value){
    need(typeof value==="string"&&value.length<=512,"Invalid configured endpoint");
    const u=new URL(value);
    need(u.protocol==="wss:"&&u.pathname==="/v1/ws"&&u.hostname&&!u.username&&!u.password&&!u.hash&&!u.search&&u.href===value,"Use an exact WSS URL ending in /v1/ws");
    return u;
}
export function pin(value){return hex(key(value,32));}
const sleep=ms=>new Promise(resolve=>setTimeout(resolve,ms));
class Transport{
    constructor(url,abort){
        this.queue=[];this.abort=abort;this.ws=new WebSocket(endpoint(url).href);
        this.ws.onmessage=e=>{
            if(abort.signal.aborted)return;
            try{
                need(typeof e.data==="string"&&e.data.length<=MAX&&text(e.data).length<=MAX,"Signaling frame too large");
                const frame=JSON.parse(e.data);need(fields(frame,["version","message"])&&frame.version===1,"Bad signaling version");
                need(this.queue.length<8,"Signaling queue full");this.queue.push(frame.message);
            }catch(e){abort.abort(e);}
        };
        this.ws.onerror=()=>abort.abort(Error("WSS/TLS connection failed"));
        this.ws.onclose=()=>abort.abort(Error("Signaling connection closed"));
        abort.signal.addEventListener("abort",()=>this.close(),{once:true});
    }
    guard(){if(this.abort.signal.aborted)throw this.abort.signal.reason??Error("Connection stopped");}
    close(){try{this.ws.close();}catch{}this.queue.length=0;}
    send(m){this.guard();const frame=JSON.stringify({version:1,message:m});need(this.ws.readyState===WebSocket.OPEN&&text(frame).length<=MAX&&this.ws.bufferedAmount<=2*MAX,"Signaling unavailable");this.ws.send(frame);}
    async receive(ms=15000){const end=performance.now()+ms;while(performance.now()<end){this.guard();if(this.queue.length)return this.queue.shift();await sleep(25);}throw Error("Signaling response timed out");}
}
function routed(value,id){const m=message(value,"signal",["session","payload"]);need(m.session===id,"Wrong session");return m.payload;}
function cipherPacket(value,id){const v=message(routed(value,id),"sealed",["packet"]).packet;need(typeof v==="string"&&v.length>=34&&v.length<=65568&&v.length%2===0,"Invalid ciphertext size");return unhex(v,v.length/2);}
function sdpShape(sdp,answer=false){
    const lines=sdp.split(/\r?\n/),m=lines.filter(l=>l.startsWith("m="));
    need(m.length===2&&m[0].startsWith("m=video ")&&m[1].startsWith("m=application ")&&/^m=video [1-9]\d* /.test(m[0]),"Expected video and data-only SDP");
    need(lines.some(l=>/^a=rtpmap:96 VP8\/90000$/i.test(l)),"Only native VP8 supported");
    if(answer)need(lines.includes("a=recvonly")&&!lines.includes("a=sendrecv")&&!lines.includes("a=sendonly"),"Controller must be receive-only");
}
async function sign(identity,data){return new Uint8Array(await crypto.subtle.sign("Ed25519",identity.key.privateKey,data));}
async function proof(identity,sid,host,cn,hn,offer,answer){
    return sign(identity,concat(text("Remote EX/session-transcript/v1\0"),Uint8Array.of(1),sid,identity.pub,host,cn,hn,Uint8Array.of(1,1),await sha(text(offer)),await sha(text(answer))));
}
export class ViewSession{
    constructor({url,serverId,hostKey,identityId,video,onState,onMetrics}){
        this.url=endpoint(url).href;this.serverId=pin(serverId);this.hostKey=pin(hostKey);this.identityId=pin(identityId);
        this.video=video;this.onState=onState;this.onMetrics=onMetrics;
        this.abort=new AbortController();this.phase="new";this.stopRequested=false;this.nextRequested=false;
        this.channelMessages=[];this.count=0;this.layout=0;this.frames=0;this.lastSwitch=0;
        this.hidden=()=>{if(document.hidden)this.cancel("Tab hidden — session stopped");};
        this.pagehide=()=>this.cancel("Page left — session stopped");
        this.abort.signal.addEventListener("abort",()=>this.cleanup(),{once:true});
    }
    state(phase,label){this.phase=phase;this.onState(phase,label);}
    blank(){this.video.pause();this.video.srcObject=null;this.video.hidden=true;}
    cleanup(){this.blank();this.received?.getTracks().forEach(t=>t.stop());try{this.dc?.close();}catch{}try{this.pc?.close();}catch{}this.socket?.close();}
    cancel(reason="Disconnected"){if(!this.abort.signal.aborted)this.abort.abort(Error(reason));}
    stop(){this.blank();if(this.phase==="viewing"){this.stopRequested=true;}else this.cancel();}
    next(){if(this.phase==="viewing"&&!this.stopRequested&&this.count>1&&performance.now()-this.lastSwitch>=500)this.nextRequested=true;}
    guard(){if(this.abort.signal.aborted)throw this.abort.signal.reason;need(!document.hidden,"Tab hidden");}
    async wait(pred,ms,error){const end=performance.now()+ms;while(performance.now()<end){this.guard();if(await pred())return;await sleep(30);}throw Error(error);}
    sendSignal(p){this.socket.send({type:"signal",session:this.session,payload:p});}
    sealed(bytes){this.sendSignal({type:"sealed",packet:hex(bytes)});}
    async incoming(ms=20000){return this.pending.length?this.pending.shift():cipherPacket(await this.socket.receive(ms),this.session);}
    channel(bytes){this.guard();need(this.dc?.readyState==="open"&&this.dc.bufferedAmount+bytes.length<=65536,"View transport unavailable");this.dc.send(bytes);}
    async reply(expected,ms){await this.wait(()=>this.channelMessages.length>0,ms,"Host readiness/stop timeout");need(equal(this.channelMessages.shift(),expected),"Unexpected host readiness/stop message");}
    layoutRecord(v){need(v.length===3&&v[0]===0xD1&&v[2]===this.count&&v[1]<this.count+(this.count>1?1:0),"Unexpected authorized display layout");this.layout=v[1];}
    async closeRoute(){
        this.socket.send({type:"close",session:this.session});
        for(let n=0;n<2;n++){
            const m=await this.socket.receive(5000);
            if(m?.type==="pong"){message(m,"pong",[]);continue;}
            const closed=message(m,"closed",["session","reason"]);need(closed.session===this.session&&closed.reason==="closed","Wrong route closure");return;
        }
        throw Error("Host route closure not confirmed");
    }
    async run(){
        need(this.phase==="new"&&window.isSecureContext&&!document.hidden,"Secure, visible browser required");
        document.addEventListener("visibilitychange",this.hidden);window.addEventListener("pagehide",this.pagehide,{once:true});
        const deadline=setTimeout(()=>this.cancel("Connection deadline reached"),100000);
        let normal=false;
        try{
            this.state("connecting","Verifying local identity");
            const identity=await readIdentity();this.guard();
            need(identity&&identity.id===this.identityId,"Local identity changed — refresh first");
            const host=key(this.hostKey,32),target=hex(await sha(host));this.guard();
            need(target!==identity.id,"Choose a different host identity");
            await init();this.guard();
            const socket=this.socket=new Transport(this.url,this.abort);
            const ch=message(await socket.receive(10000),"challenge",["server","nonce"]);
            need(ch.server===this.serverId,"Server ID mismatch — registration refused");
            const signature=await sign(identity,concat(text("Remote EX/register-controller-only/v1\0"),key(ch.server,32),key(ch.nonce,32)));
            socket.send({type:"register_controller",public_key:hex(identity.pub),signature:hex(signature)});
            need(message(await socket.receive(),"registered",["device_id"]).device_id===identity.id,"Registration identity mismatch");
            this.state("approval","Waiting for host read-only approval");
            const cn=random(32);
            socket.send({type:"request",target,requested:1,controller_nonce:hex(cn)});
            const requested=message(await socket.receive(),"requested",["session","target"]);
            this.session=requested.session;const sid=key(this.session,16);need(requested.target===target,"Target substitution refused");
            const decision=message(await socket.receive(20000),"decision",["session","grant","host_key","host_nonce"]);
            need(decision.session===this.session&&decision.grant===1&&decision.host_key===this.hostKey,"Host rejected view-only grant or key mismatch");
            const hn=key(decision.host_nonce,32),peer=this.peer=new ViewPeer(sid,identity.pub,host,cn,hn);
            this.state("authenticating","Authenticating encrypted route");
            this.sendSignal({type:"noise_handshake",step:1,packet:hex(peer.write_handshake())});
            const hs=message(routed(await socket.receive(),this.session),"noise_handshake",["step","packet"]);need(hs.step===2,"Invalid Noise step");peer.read_handshake(unhex(hs.packet,48));
            this.sealed(peer.send_proof(await sign(identity,peer.signature_message())));
            this.pending=[];peer.receive_proof(await this.incoming());need(peer.ready(),"Route authentication failed");
            this.sealed(peer.send_quality());peer.receive_quality(await this.incoming());
            socket.send({type:"ice_config",session:this.session});
            let gotIce=false;
            for(let i=0;i<5;i++){
                const m=await socket.receive();
                if(m?.type==="ice_config"){
                    const v=message(m,"ice_config",["session","config"]);
                    need(v.session===this.session&&fields(v.config,["stun","turn"])&&v.config.stun===null&&v.config.turn===null,"This view candidate supports direct host ICE only; disable STUN/TURN for the test server");
                    gotIce=true;break;
                }
                need(this.pending.length<4,"ICE configuration race limit");this.pending.push(cipherPacket(m,this.session));
            }
            need(gotIce,"ICE configuration missing");
            const offer=peer.receive_sdp(await this.incoming());sdpShape(offer);
            const hf=sdpFingerprint(offer);
            const pc=this.pc=new RTCPeerConnection({iceServers:[],bundlePolicy:"max-bundle"});
            const dc=this.dc=pc.createDataChannel("remote-ex-control",{negotiated:true,id:0,ordered:true,protocol:"remote-ex-input-v1"});
            dc.binaryType="arraybuffer";dc.onerror=()=>this.cancel("Data transport failed");
            dc.onmessage=e=>{if(!(e.data instanceof ArrayBuffer)||!e.data.byteLength||e.data.byteLength>1024||this.channelMessages.length>=8)this.cancel("Invalid view response");else this.channelMessages.push(new Uint8Array(e.data));};
            pc.ondatachannel=e=>{e.channel.close();this.cancel("Unexpected data channel");};
            pc.ontrack=e=>{if(e.track.kind!=="video"||this.received){e.track.stop();this.cancel("Unexpected media track");return;}this.received=new MediaStream([e.track]);};
            pc.onconnectionstatechange=()=>{if(["failed","disconnected","closed"].includes(pc.connectionState)&&!this.abort.signal.aborted)this.cancel("Media transport disconnected");};
            const candidates=[];
            pc.onicecandidate=e=>{if(e.candidate?.candidate){if(e.candidate.sdpMLineIndex==null||candidates.length>=128)this.cancel("ICE limit");else candidates.push({mline:e.candidate.sdpMLineIndex,candidate:e.candidate.candidate});}};
            await pc.setRemoteDescription({type:"offer",sdp:offer});this.guard();
            const tr=pc.getTransceivers();need(tr.length===1&&tr[0].receiver.track.kind==="video","Unexpected transceiver");tr[0].direction="recvonly";
            const answer=await pc.createAnswer();sdpShape(answer.sdp,true);
            const cf=sdpFingerprint(answer.sdp);need(cf!==hf,"DTLS identity collision");
            await pc.setLocalDescription(answer);this.guard();
            this.sealed(peer.send_sdp(answer.sdp));peer.receive_sdp_proof(await this.incoming());
            this.sealed(peer.send_sdp_proof(await proof(identity,sid,host,cn,hn,offer,answer.sdp)));
            need(peer.sdp_verified(),"Exact SDP signatures failed");
            this.state("transport","Negotiating authenticated video transport");
            await this.wait(()=>pc.iceGatheringState==="complete",15000,"ICE gathering timed out");
            this.sealed(peer.send_ice_batch(JSON.stringify(candidates)));
            const ice=JSON.parse(peer.receive_ice_batch(await this.incoming()));
            for(const item of ice.filter(x=>x.candidate))await pc.addIceCandidate({sdpMLineIndex:item.mline,candidate:item.candidate});
            for(const item of ice.filter(x=>!x.candidate))await pc.addIceCandidate({sdpMLineIndex:item.mline,candidate:""});
            need(peer.native_ice_verified()&&this.pending.length===0,"Encrypted ICE failed");
            await this.wait(()=>pc.connectionState==="connected"&&dc.readyState==="open",15000,"Media connection timed out");
            await this.wait(()=>dtlsFingerprint(pc,cf,hf),5000,"DTLS certificate verification unavailable");
            this.state("capture","Waiting for approved host screen");
            this.geometry=JSON.parse(peer.receive_geometry(await this.incoming(45000)));
            this.count=peer.receive_layout(await this.incoming());need(peer.view_metadata_verified(),"View metadata not verified");
            this.channel(READY);await this.reply(READY_ACK,42000);
            await this.wait(()=>this.channelMessages.length>0,3000,"Host layout unavailable");this.layoutRecord(this.channelMessages.shift());
            // Keep the host alive immediately after its READY_ACK; do not attach
            // any track until signed route, DTLS and approved capture readiness.
            this.channel(HEARTBEAT);
            await this.wait(()=>!!this.received,1000,"Approved video track missing");
            this.video.muted=true;this.video.playsInline=true;this.video.srcObject=this.received;
            await this.video.play();this.guard();this.video.hidden=false;
            this.state("viewing","Connected — read only");
            let lastHeartbeat=performance.now(),lastPing=performance.now(),ping=0,lastStats=0,lastFrame=performance.now(),lastDecoded=0,started=performance.now();
            while(!this.stopRequested){
                this.guard();const now=performance.now();
                if(now-lastHeartbeat>=450){this.channel(HEARTBEAT);lastHeartbeat=now;}
                if(this.nextRequested){this.nextRequested=false;this.channel(NEXT);this.lastSwitch=now;}
                if(!ping&&now-lastPing>=8000){socket.send({type:"ping"});ping=now;lastPing=now;}
                if(ping&&now-ping>5000)throw Error("Signaling heartbeat lost");
                while(socket.queue.length){
                    const m=socket.queue.shift();
                    if(m?.type==="pong"&&ping){message(m,"pong",[]);ping=0;}
                    else throw Error("Host or signaling service ended the session");
                }
                while(this.channelMessages.length){
                    const v=this.channelMessages.shift();
                    if(equal(v,ACK)){this.blank();await this.closeRoute();normal=true;return;}
                    this.layoutRecord(v);
                }
                if(now-lastStats>=300){
                    const stats=await pc.getStats();this.guard();
                    const v=[...stats.values()].find(x=>x.type==="inbound-rtp"&&(x.kind==="video"||x.mediaType==="video"));
                    const decoded=v?.framesDecoded??0;
                    if(decoded>lastDecoded){lastDecoded=decoded;lastFrame=now;}
                    this.frames=decoded;lastStats=now;
                    this.onMetrics({frames:decoded,width:this.video.videoWidth,height:this.video.videoHeight,count:this.count,layout:this.layout,seconds:Math.max(0,Math.ceil((30000-(now-started))/1000))});
                }
                if(now-lastFrame>2500)throw Error("Approved video stalled");
                if(now-started>=30000){this.stopRequested=true;break;}
                await sleep(30);
            }
            this.blank();this.state("closing","Disconnecting");
            this.channel(STOP);
            await this.wait(()=>{
                while(this.channelMessages.length){const v=this.channelMessages.shift();if(equal(v,ACK))return true;this.layoutRecord(v);}
                return false;
            },3000,"Host stop acknowledgement not received");
            await this.closeRoute();normal=true;
        }catch(e){
            this.state("error",String(e?.message??e).slice(0,220));
        }finally{
            clearTimeout(deadline);this.cleanup();if(this.peer){try{this.peer.close();}finally{this.peer.free();}}
            document.removeEventListener("visibilitychange",this.hidden);window.removeEventListener("pagehide",this.pagehide);
            if(normal)this.state("stopped","Disconnected — host confirmed");
        }
    }
}
