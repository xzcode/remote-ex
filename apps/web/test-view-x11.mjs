// Actual capture of an isolated Xvfb screen; never run on a user's desktop.
import assert from "node:assert/strict";
import {chromium,webkit} from "playwright";
import {stage} from "../../scripts/stage-mobile-client.mjs";
import {prepare,listen} from "../../deploy/controller-site.mjs";
import {verifyWebKitPixels} from "../../scripts/ci/webkit-receiver-pixels.mjs";
import {serve} from "./serve.mjs";
import {spawn,execFileSync} from "node:child_process";
import {mkdtempSync,mkdirSync,writeFileSync,rmSync} from "node:fs";
import {tmpdir} from "node:os";
import path from "node:path";
import {fileURLToPath} from "node:url";
import {randomBytes,createHash} from "node:crypto";
import {createInterface} from "node:readline";
import {once} from "node:events";
import dgram from "node:dgram";
import {freePort,certificates,installRoot,ready,stopChild} from "./wss-test-support.mjs";
assert.equal(process.env.RUNNER_ENVIRONMENT,"github-hosted");
assert.equal(process.env.XDG_SESSION_TYPE,"x11");
assert.match(process.env.DISPLAY??"",/^:\d+$/);
const relayOnly=process.argv.slice(2).includes("--relay-only");
const extended=process.argv.slice(2).includes("--extended");
const useWebKit=process.argv.slice(2).includes("--webkit");
assert(process.argv.slice(2).every(x=>["--relay-only","--extended","--webkit"].includes(x)));
assert(!useWebKit||!relayOnly&&!extended,"First WebKit receiver acceptance is direct read-only only");
function reader(child){
    const lines=createInterface({input:child.stdout})[Symbol.asyncIterator]();
    return async(prefix,limit=15000)=>{
        let timer;const end=Date.now()+limit;
        try{
            while(Date.now()<end){
                const line=await Promise.race([lines.next(),new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error("Host output timeout: "+prefix)),Math.max(1,end-Date.now()));})]);
                clearTimeout(timer);
                assert(!line.done&&line.value.length<=4096,"Host output unavailable");
                if(line.value.startsWith(prefix))return line.value;
                // Ignore only fixed statistics emitted by the authorized host.
                if(!line.value.startsWith("NET_STATS ")&&!line.value.startsWith("STREAM_QUALITY "))throw Error("Unexpected host output: "+line.value);
            }
            throw Error("Host output timeout");
        }finally{clearTimeout(timer);}
    };
}
async function relayFixture(root){
    // Reuse existing project's coturn disposable private-interface pattern.
    const interfaces=JSON.parse(execFileSync("ip",["-j","-4","addr","show","scope","global"],{encoding:"utf8",timeout:5000}));
    const ip=interfaces.flatMap(x=>x.addr_info??[]).find(x=>x.family==="inet")?.local;
    assert.match(ip??"",/^\d+\.\d+\.\d+\.\d+$/);
    const sock=dgram.createSocket("udp4");
    await new Promise((resolve,reject)=>{sock.once("error",reject);sock.bind(0,ip,resolve);});
    const port=sock.address().port;await new Promise(resolve=>sock.close(resolve));
    const secret=randomBytes(32).toString("hex"),secretFile=path.join(root,"turn-secret"),configFile=path.join(root,"turn.conf");
    writeFileSync(secretFile,secret+"\n",{mode:0o600,flag:"wx"});
    const config=[
        "listening-ip="+ip,"relay-ip="+ip,"listening-port="+port,"realm=remote-ex-web-ci",
        "use-auth-secret","static-auth-secret="+secret,"fingerprint","no-cli","no-tls","no-dtls","no-multicast-peers",
        "no-tcp-relay","min-port=40000","max-port=40063","total-quota=8","user-quota=2",
        "pidfile="+path.join(root,"turn.pid"),"userdb="+path.join(root,"turn.db"),
        "allowed-peer-ip="+ip,"denied-peer-ip=0.0.0.0-255.255.255.255"
    ].join("\n")+"\n";
    writeFileSync(configFile,config,{mode:0o600,flag:"wx"});
    const process=spawn("turnserver",["-c",configFile],{stdio:"ignore"});
    // No credentials or coturn logs emitted; service remains private runner only.
    return {process,stun:"stun://"+ip+":"+port,turn:"turn://"+ip+":"+port,secretFile};
}
const root=mkdtempSync(path.join(tmpdir(),"remote-ex-web-view-"));let server,host,browser,web,relay,page;
try{
    const home=path.join(root,"home");mkdirSync(home,{mode:0o700});
    const certs=useWebKit?{
        ca:process.env.REMOTE_EX_WEBKIT_CA,cert:process.env.REMOTE_EX_WEBKIT_CERT,key:process.env.REMOTE_EX_WEBKIT_KEY
    }:certificates(root);
    if(useWebKit)assert(Object.values(certs).every(v=>typeof v==="string"&&path.isAbsolute(v)),"Disposable WebKit TLS files required");
    else installRoot(home,certs.ca);
    const port=await freePort(),endpoint="wss://127.0.0.1:"+port+"/v1/ws";
    if(relayOnly){relay=await relayFixture(root);await new Promise(resolve=>setTimeout(resolve,1000));assert.equal(relay.process.exitCode,null);}
    if(useWebKit){
        const dir=path.join(root,"reviewed-assets"),sitePort=await freePort();
        await stage(dir,{endpoint,ice:{stun:null,turn:[],relayOnly:false},control:false,extended:false});
        const reviewed=await prepare(dir,{REMOTE_EX_SITE_HOST:"localhost",REMOTE_EX_SITE_PORT:String(sitePort),REMOTE_EX_SITE_TLS_CERT:certs.cert,REMOTE_EX_SITE_TLS_KEY:certs.key});
        web={server:await listen(reviewed),url:"https://localhost:"+sitePort};
        // WebKit headless already works without DISPLAY in the identity CI.
        // Do not allow its renderer to share the native sender's Xvfb.
        const browserEnv=Object.fromEntries(Object.entries(process.env).filter(([name])=>!["DISPLAY","XAUTHORITY","WAYLAND_DISPLAY","WAYLAND_SOCKET"].includes(name)));
        browser=await webkit.launch({headless:true,env:browserEnv});
    }else{
        web=await serve(0,null,endpoint,{stun:relay?.stun??null,turn:relay?[relay.turn]:[],relayOnly,control:false,extended});
        browser=await chromium.launch({headless:true,env:{...process.env,HOME:home,XDG_CONFIG_HOME:path.join(home,".config"),XDG_DATA_HOME:path.join(home,".local/share")},args:["--disable-background-networking","--disable-features=WebRtcHideLocalIpsWithMdns","--autoplay-policy=no-user-gesture-required"]});
    }
    const context=await browser.newContext({permissions:[],ignoreHTTPSErrors:false,...(useWebKit?{viewport:{width:390,height:844},isMobile:true,hasTouch:true}:{})});
    const unexpected=[],errors=[];
    await context.route("**/*",r=>{if(new URL(r.request().url()).origin!==web.url){unexpected.push("Unexpected HTTP origin");return r.abort();}return r.continue();});
    await context.addInitScript(()=>{window.__forbiddenCalls=0;if(navigator.mediaDevices)for(const name of ["getDisplayMedia","getUserMedia"])Object.defineProperty(navigator.mediaDevices,name,{configurable:true,value:async()=>{window.__forbiddenCalls++;throw Error("Forbidden capture");}});});
    page=await context.newPage();
    page.on("crash",()=>console.log("REMOTE EX WEB RECEIVER: browser page crashed"));
    browser.on("disconnected",()=>{if(page&&!page.isClosed())console.log("REMOTE EX WEB RECEIVER: browser disconnected");});
    page.on("pageerror",e=>errors.push(e.message));page.on("websocket",ws=>{if(ws.url()!==endpoint)unexpected.push("Unexpected websocket");});
    await page.goto(web.url+"/identity.html");
    if(useWebKit)assert(await page.evaluate(()=>isSecureContext&&location.protocol==="https:"));
    await page.locator("#identity-create").click();
    await page.waitForFunction(()=>document.querySelector("#identity-status").dataset.state==="verified");
    const controllerId=await page.locator("#device-id").inputValue(),controllerKey=await page.locator("#public-key").inputValue();
    assert.match(controllerId,/^[0-9a-f]{64}$/);assert.equal(createHash("sha256").update(Buffer.from(controllerKey,"hex")).digest("hex"),controllerId);
    await page.goto(web.url+"/view.html");
    await page.waitForFunction(()=>document.querySelector("#view-status").dataset.state==="idle"&&document.querySelector("#view-id").value!=="");
    assert.equal(await page.locator("#view-id").inputValue(),controllerId);
    assert.equal(await page.locator("#view-endpoint").inputValue(),endpoint);
    assert(await page.locator("#view-connect").isDisabled());
    const exe=fileURLToPath(new URL("../../crates/media/target/debug/routed_x11_view",import.meta.url));
    execFileSync("xsetroot",["-solid","#26b773"],{stdio:"ignore",timeout:10000});
    const probe=execFileSync(exe,["--probe-x11"],{encoding:"utf8",timeout:10000}).trim().split(/\s+/);
    assert.deepEqual(probe.slice(0,3),["PASS","X11","CAPTURE"]);
    const serverId=randomBytes(32).toString("hex");
    const env=Object.fromEntries(Object.entries(process.env).filter(([name])=>!name.startsWith("REMOTE_EX_")));
    host=spawn(exe,["--host",endpoint,serverId,"--ca",certs.ca,"--desktop-session","--view",probe[3],"--enroll-before-register",...(relayOnly?["--relay-only"]:[]),...(extended?["--extended"]:[])],{env,stdio:["pipe","pipe","inherit"]});
    const exited=once(host,"exit");exited.catch(()=>{});
    const read=reader(host),hostKey=(await read("KEY ")).slice(4),hostId=createHash("sha256").update(Buffer.from(hostKey,"hex")).digest("hex");
    assert.equal(await read("ENROLL "),"ENROLL "+hostId);
    const allow=path.join(root,"allowed.txt");writeFileSync(allow,[hostId,controllerId].join("\n")+"\n",{mode:0o600});
    Object.assign(env,{REMOTE_EX_SIGNAL_BIND:"127.0.0.1:"+port,REMOTE_EX_TLS_CERT:certs.cert,REMOTE_EX_TLS_KEY:certs.key,REMOTE_EX_SERVER_ID:serverId,REMOTE_EX_ALLOWED_DEVICES_FILE:allow,REMOTE_EX_WEB_ORIGIN:web.url});
    if(relay)Object.assign(env,{REMOTE_EX_STUN_URL:relay.stun,REMOTE_EX_TURN_URL:relay.turn,REMOTE_EX_TURN_SECRET_FILE:relay.secretFile});
    const rendezvous=fileURLToPath(new URL("../../target/debug/remote-ex-rendezvous",import.meta.url));
    server=spawn(rendezvous,[],{env,stdio:["ignore","pipe","inherit"]});await ready(server);
    // Wrong server pin must fail before registration, while host remains at
    // the explicit enrollment barrier. Then perform the actual view.
    await page.locator("#view-server").fill((serverId[0]==="0"?"1":"0")+serverId.slice(1));
    await page.locator("#view-host").fill(hostKey);if(extended)await page.locator("#view-extended").check();await page.locator("#view-consent").check();await page.locator("#view-connect").click();
    await page.waitForFunction(()=>document.querySelector("#view-status").dataset.state==="error");
    assert.match(await page.locator("#view-status").textContent(),/Server ID mismatch/);
    assert(await page.locator("#view-video").isHidden());
    host.stdin.write("REGISTER DEVICE\n");assert.equal(await read("ONLINE "),"ONLINE "+hostId);
    await read("PEER_KEY ");host.stdin.write(controllerKey+"\n");const consent=extended?"ALLOW ROUTED SCREEN FOR UP TO 300 SECONDS":"ALLOW ROUTED SCREEN";await read("CONSENT (type "+consent+";");host.stdin.write(consent+"\n");
    console.log("REMOTE EX WEB RECEIVER: independently enrolled read-only host and controller");
    await page.locator("#view-server").fill(serverId);
    await page.locator("#view-consent").check();await page.locator("#view-connect").click();
    await page.waitForFunction(()=>["viewing","error"].includes(document.querySelector("#view-status").dataset.state),{},{timeout:65000});
    assert.equal(await page.locator("#view-status").getAttribute("data-state"),"viewing",await page.locator("#view-status").textContent());
    await page.waitForFunction(()=>Number(document.querySelector("#view-frames").textContent)>=10,{},{timeout:10000});
    assert.equal(await page.locator("#view-resolution").textContent(),"960 × 540");
    if(relayOnly)assert.equal(await page.locator("#view-route").textContent(),"Transport: TURN relay");
    if(useWebKit){
        await verifyWebKitPixels(page);
    }else{
        const green=await page.evaluate(()=>{const v=document.querySelector("#view-video");const c=document.createElement("canvas");c.width=v.videoWidth;c.height=v.videoHeight;const ctx=c.getContext("2d");ctx.drawImage(v,0,0);const pixel=ctx.getImageData(480,270,1,1).data;return pixel[1]>120&&pixel[1]>pixel[0]+50&&pixel[1]>pixel[2]+25;});
        assert(green,"Actual captured Xvfb background absent");
    }
    assert(await page.locator("#view-next").isDisabled()); // exactly one approved display
    if(extended){
        const start=Date.now();
        assert(Number((await page.locator("#view-time").textContent()).split(" ")[0])>240);
        await page.locator("#quality-mode").selectOption("3");await page.locator("#quality-apply").click();
        await page.waitForFunction(()=>document.querySelector("#quality-status").textContent.includes("Low / Low")&&document.querySelector("#view-resolution").textContent==="480 × 270",{},{timeout:15000});
        await page.locator("#quality-mode").selectOption("1");
        await page.waitForTimeout(1100);await page.locator("#quality-apply").click();
        await page.waitForFunction(()=>document.querySelector("#quality-status").textContent.includes("Full / Full")&&document.querySelector("#view-resolution").textContent==="960 × 540",{},{timeout:15000});
        await page.waitForTimeout(Math.max(0,35000-(Date.now()-start)));
        assert.equal(await page.locator("#view-status").getAttribute("data-state"),"viewing");
        assert(Number((await page.locator("#view-time").textContent()).split(" ")[0])>200);
    }
    await page.locator("#view-disconnect").click();
    await page.waitForFunction(()=>["stopped","error"].includes(document.querySelector("#view-status").dataset.state),{},{timeout:12000});
    assert.equal(await page.locator("#view-status").getAttribute("data-state"),"stopped",await page.locator("#view-status").textContent());
    assert(await page.evaluate(()=>document.querySelector("#view-video").srcObject===null));
    await read("CONNECTED ROUTED VIEW");await read("INPUT_RELEASED");await read("PASS ROUTED HOST:");
    const [code]=await exited;assert.equal(code,0);
    assert.equal(await page.evaluate(()=>window.__forbiddenCalls),0);
    assert.deepEqual(unexpected,[]);assert.deepEqual(errors,[]);await context.close();
    console.log("PASS "+(useWebKit?"WebKit staged HTTPS receiver; ":"")+(relayOnly?"forced TURN relay; ":"direct ICE; ")+(extended?"extended quality and >30s viewing; ":"")+"actual isolated X11 screen -> Web UI: explicit enrollment, incorrect server pin rejected, read-only grant, verified captured pixels, stop and route closure");
}catch(error){
    if(useWebKit&&page&&!page.isClosed()){
        const diagnostic=await page.locator("#view-status").evaluate(e=>({
            state:e.dataset.state??"unknown",
            message:(e.textContent??"").replace(/[^A-Za-z0-9 _.,:;()/-]/g," ").slice(0,160)
        })).catch(()=>null);
        if(diagnostic)console.log("WEBKIT_NATIVE_RECEIVER "+JSON.stringify(diagnostic));
    }
    throw error;
}finally{
    await browser?.close();await stopChild(host);await stopChild(server);await stopChild(relay?.process);
    if(web)await new Promise(resolve=>web.server.close(resolve));
    rmSync(root,{recursive:true,force:true});
}
