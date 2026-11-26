import init,{FixturePeer} from "./pkg/noise.js";
import {publicVectors,newFixtureIdentity,random,authenticateNoise,signedSdpPair,sdpFingerprint,dtlsFingerprint} from "./crypto.js";
const $=id=>document.getElementById(id);
const source=$("source"),video=$("remote"),start=$("start"),stop=$("stop"),consent=$("consent"),status=$("status"),error=$("error");
const paint=source.getContext("2d",{alpha:false});
let active=null,wasmReady=null;
function state(value,label){status.dataset.state=value;status.textContent=label;}
function step(name){document.querySelector(`[data-step="${name}"]`).dataset.pass="true";}
function resetSteps(){for(const node of document.querySelectorAll("[data-step]"))delete node.dataset.pass;}
function guard(s){if(active!==s||s.abort.signal.aborted||document.hidden)throw Error("Local fixture canceled");}
function checkFeature(){
    if(!window.isSecureContext||!crypto?.subtle||typeof RTCPeerConnection!=="function"||typeof source.captureStream!=="function"||!paint)throw Error("A secure browser with WebCrypto, WebRTC and canvas capture is required");
}
function delay(s,ms){return new Promise((resolve,reject)=>{
    const signal=s.abort.signal;if(signal.aborted){reject(Error("Fixture canceled"));return;}
    const cancel=()=>{clearTimeout(timer);reject(Error("Fixture canceled"));};
    const timer=setTimeout(()=>{signal.removeEventListener("abort",cancel);resolve();},ms);
    signal.addEventListener("abort",cancel,{once:true});
});}
async function wait(s,predicate,limit,label){
    const until=performance.now()+limit;
    do{guard(s);if(await predicate()){guard(s);return;}await delay(s,60);}while(performance.now()<until);
    throw Error(label);
}
function close(s,next="stopped",label="Disconnected"){
    if(active!==s)return;
    active=null;s.abort.abort();clearTimeout(s.deadline);clearInterval(s.paintTimer);clearInterval(s.statsTimer);
    if(s.frameCallback!=null&&video.cancelVideoFrameCallback)video.cancelVideoFrameCallback(s.frameCallback);
    s.stream?.getTracks().forEach(t=>t.stop());s.received?.getTracks().forEach(t=>t.stop());
    for(const pc of s.peers){try{pc.close();}catch{}}
    video.pause();video.srcObject=null;paint?.fillRect(0,0,source.width,source.height);
    consent.checked=false;consent.disabled=false;start.disabled=true;stop.disabled=true;state(next,label);
}
function draw(s){
    const n=++s.sequence,x=(n*4)%260;
    paint.fillStyle="#172336";paint.fillRect(0,0,320,180);
    paint.fillStyle="#30a6a0";paint.fillRect(x,82,60,58);
    paint.fillStyle="#dbeafe";paint.font="bold 18px system-ui";paint.fillText("REMOTE EX",16,30);
    paint.font="12px system-ui";paint.fillText("SYNTHETIC • LOCAL ONLY",16,51);
    paint.fillText("Generated frame "+n,16,164);
}
function peers(s){
    const opts={iceServers:[],iceTransportPolicy:"all",bundlePolicy:"max-bundle"};
    const c=new RTCPeerConnection(opts),h=new RTCPeerConnection(opts);s.peers.push(c,h);
    for(const pc of [c,h])pc.addEventListener("connectionstatechange",()=>{
        if(active===s&&["failed","disconnected","closed"].includes(pc.connectionState))close(s,"error","Connection lost");
    });
    return [c,h];
}
async function gather(s,pc){await wait(s,()=>pc.iceGatheringState==="complete",7000,"Local ICE gathering timed out");}
async function run(){
    if(active||!consent.checked)return;
    const s={abort:new AbortController(),peers:[],stream:null,received:null,sequence:0,decoded:0,deadline:null,paintTimer:null,statsTimer:null,frameCallback:null};
    active=s;resetSteps();error.textContent="";consent.disabled=true;start.disabled=true;stop.disabled=false;
    $("frames").textContent="—";$("resolution").textContent="—";$("transport").textContent="—";state("running","Checking prerequisites");
    s.deadline=setTimeout(()=>close(s,"stopped","30-second limit reached"),30000);
    try{
        checkFeature();guard(s);
        const response=await fetch("v1-fixture.json",{signal:s.abort.signal,cache:"no-store"});
        if(!response.ok)throw Error("Public fixture unavailable");
        await publicVectors(await response.json());guard(s);step("vectors");
        if(!wasmReady)wasmReady=init().catch(e=>{wasmReady=null;throw e;});
        await wasmReady;guard(s);
        const controller=await newFixtureIdentity(),host=await newFixtureIdentity();guard(s);step("identity");
        const session=random(16),cn=random(32),hn=random(32);
        await authenticateNoise(FixturePeer,controller,host,session,cn,hn);guard(s);step("noise");
        state("running","Negotiating local WebRTC");
        const [c,h]=peers(s);
        c.addTransceiver("video",{direction:"recvonly"});
        // An inert ordered data channel establishes a DTLS transport without
        // attaching a track or sending any application/input payload.
        const channel=c.createDataChannel("synthetic-fixture",{ordered:true});
        let hostChannel=null;
        h.ondatachannel=e=>{if(e.channel.label==="synthetic-fixture")hostChannel=e.channel;else e.channel.close();};
        const initialOffer=await c.createOffer();guard(s);
        await c.setLocalDescription(initialOffer);guard(s);await gather(s,c);
        const offer=c.localDescription.sdp;
        // Required to produce an answer; no track or data is attached/sent yet.
        const cf=sdpFingerprint(offer);
        await h.setRemoteDescription({type:"offer",sdp:offer});guard(s);
        const tr=h.getTransceivers().find(t=>t.receiver.track.kind==="video");
        if(!tr)throw Error("No negotiated video transceiver");
        tr.direction="sendonly";
        const initialAnswer=await h.createAnswer();guard(s);
        await h.setLocalDescription(initialAnswer);guard(s);await gather(s,h);
        const answer=h.localDescription.sdp,hf=sdpFingerprint(answer);
        if(cf===hf)throw Error("Fixture must use independent DTLS certificates");
        await signedSdpPair(controller,host,session,cn,hn,offer,answer);guard(s);step("sdp");
        c.ontrack=e=>{if(active!==s||e.track.kind!=="video"){e.track.stop();return;}s.received=new MediaStream([e.track]);video.srcObject=s.received;};
        await c.setRemoteDescription({type:"answer",sdp:answer});guard(s);
        await wait(s,()=>c.connectionState==="connected"&&h.connectionState==="connected"&&channel.readyState==="open"&&hostChannel?.readyState==="open",9000,"Local WebRTC transport did not connect");
        await wait(s,async()=>await dtlsFingerprint(c,cf,hf)&&await dtlsFingerprint(h,hf,cf),4000,"Actual DTLS certificate stats unavailable");
        step("dtls");$("transport").textContent="DTLS verified";
        // Only after both proof checks and actual connected certificate checks,
        // attach locally generated pixels. No getDisplayMedia/getUserMedia.
        draw(s);s.stream=source.captureStream(15);
        const track=s.stream.getVideoTracks()[0];if(!track)throw Error("Synthetic track unavailable");
        await tr.sender.replaceTrack(track);guard(s);
        s.paintTimer=setInterval(()=>{if(active===s)draw(s);},66);
        await wait(s,()=>video.srcObject!==null,3000,"No synthetic received track");
        await video.play();guard(s);
        // Actual inbound RTP decoder stats, not an estimate or source count.
        const measure=async()=>{
            if(active!==s)return;
            const report=await c.getStats();if(active!==s)return;
            const inbound=[...report.values()].find(r=>r.type==="inbound-rtp"&&(r.kind==="video"||r.mediaType==="video"));
            if(inbound&&Number.isSafeInteger(inbound.framesDecoded)){
                s.decoded=inbound.framesDecoded;$("frames").textContent=String(s.decoded);
                if(video.videoWidth&&video.videoHeight)$("resolution").textContent=video.videoWidth+" × "+video.videoHeight;
            }
        };
        await wait(s,async()=>{await measure();return s.decoded>=3&&video.videoWidth>0&&video.videoHeight>0;},4500,"No decoded synthetic frames");
        // Sample actual video pixels: generated source is never blank.
        const check=document.createElement("canvas");check.width=video.videoWidth;check.height=video.videoHeight;
        const ctx=check.getContext("2d",{willReadFrequently:true});if(!ctx)throw Error("Pixel check unavailable");
        ctx.drawImage(video,0,0);const pixels=ctx.getImageData(0,0,check.width,check.height).data;
        let bright=0;for(let i=0;i<pixels.length;i+=4)if(pixels[i+1]>100&&pixels[i+2]>70)bright++;
        if(bright<100)throw Error("Decoded image lacks generated pattern");
        guard(s);step("video");state("verified","Synthetic video verified");
        s.statsTimer=setInterval(()=>{void measure().catch(()=>close(s,"error","Stats failed"));},500);
    }catch(e){
        if(active!==s)return;
        error.textContent=String(e?.message??e).slice(0,250);close(s,"error","Test failed");
    }
}
consent.addEventListener("change",()=>{start.disabled=!!active||!consent.checked;});
start.addEventListener("click",()=>void run());
stop.addEventListener("click",()=>{if(active)close(active);});
document.addEventListener("visibilitychange",()=>{if(document.hidden&&active)close(active);});
window.addEventListener("pagehide",()=>{if(active)close(active);});
