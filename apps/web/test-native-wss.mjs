// Native media engine ↔ browser, three isolated processes and real TLS.
// The native binary is the no-feature routed_diagnostic: only generated
// pixels and its inert input backend, never OS capture or user input.
import assert from "node:assert/strict";
import {chromium} from "playwright";
import {serve} from "./serve.mjs";
import {spawn,execFileSync} from "node:child_process";
import {mkdtempSync,mkdirSync,writeFileSync,rmSync} from "node:fs";
import {tmpdir} from "node:os";
import path from "node:path";
import {fileURLToPath} from "node:url";
import {randomBytes,createHash} from "node:crypto";
import {createInterface} from "node:readline";
import {once} from "node:events";
import {freePort,certificates,installRoot,ready,stopChild} from "./wss-test-support.mjs";
function reader(child){
    const it=createInterface({input:child.stdout})[Symbol.asyncIterator]();
    return async(prefix,limit=15000)=>{
        let timer;
        try{
            const line=await Promise.race([it.next(),new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error("Native host output timeout: "+prefix)),limit);})]);
            assert(!line.done&&line.value.length<=1024,"Native host output unavailable");
            assert(line.value.startsWith(prefix),"Expected native host output "+prefix+"; got "+line.value);
            return line.value;
        }finally{clearTimeout(timer);}
    };
}
const root=mkdtempSync(path.join(tmpdir(),"remote-ex-native-wss-"));
let browser,fixture,server,host;
try{
    const home=path.join(root,"home");mkdirSync(home,{mode:0o700});
    const certs=certificates(root);installRoot(home,certs.ca);
    const port=await freePort(),endpoint="wss://127.0.0.1:"+port+"/v1/ws";
    fixture=await serve(0,endpoint);
    // Use actual host ICE addresses rather than browser mDNS names only on
    // this disposable hosted runner, to test native libnice interoperability.
    browser=await chromium.launch({headless:true,env:{...process.env,HOME:home,XDG_CONFIG_HOME:path.join(home,".config"),XDG_DATA_HOME:path.join(home,".local/share")},args:["--disable-background-networking","--autoplay-policy=no-user-gesture-required","--disable-features=WebRtcHideLocalIpsWithMdns"]});
    const context=await browser.newContext({permissions:[],ignoreHTTPSErrors:false});
    const unexpected=[],errors=[];
    await context.route("**/*",route=>{
        if(new URL(route.request().url()).origin!==fixture.url){unexpected.push("Unexpected HTTP origin");return route.abort();}
        return route.continue();
    });
    await context.addInitScript(()=>{
        window.__forbiddenCalls=0;
        if(navigator.mediaDevices)for(const name of ["getDisplayMedia","getUserMedia"])Object.defineProperty(navigator.mediaDevices,name,{configurable:true,value:async()=>{window.__forbiddenCalls++;throw Error("Forbidden browser capture");}});
    });
    const page=await context.newPage();
    page.on("pageerror",e=>errors.push(String(e.message)));
    page.on("websocket",ws=>{if(ws.url()!==endpoint)unexpected.push("Unexpected WSS endpoint");});
    await page.goto(fixture.url,{waitUntil:"networkidle"});
    const identity=await page.evaluate(async()=>{const lab=await import("/native-wss-fixture.js");return lab.prepare();});
    assert.match(identity.id,/^[0-9a-f]{64}$/);assert.match(identity.key,/^[0-9a-f]{64}$/);
    assert.equal(createHash("sha256").update(Buffer.from(identity.key,"hex")).digest("hex"),identity.id);
    await page.reload({waitUntil:"networkidle"}); // registration uses the persisted key
    const serverId=randomBytes(32).toString("hex"),env=Object.fromEntries(Object.entries(process.env).filter(([name])=>!name.startsWith("REMOTE_EX_")));
    const exe=fileURLToPath(new URL("../../crates/media/target/debug/routed_diagnostic",import.meta.url));
    host=spawn(exe,["--host",endpoint,serverId,"--ca",certs.ca,"--desktop-session","--defer-test-registration"],{env,stdio:["pipe","pipe","inherit"]});
    const hostExit=once(host,"exit");hostExit.catch(()=>{});
    const read=reader(host);
    const hostKey=(await read("KEY ")).slice(4);
    assert.match(hostKey,/^[0-9a-f]{64}$/);assert.notEqual(hostKey,identity.key);
    const hostId=createHash("sha256").update(Buffer.from(hostKey,"hex")).digest("hex");
    const allow=path.join(root,"allowed.txt");writeFileSync(allow,[hostId,identity.id].join("\n")+"\n",{mode:0o600});
    Object.assign(env,{REMOTE_EX_SIGNAL_BIND:"127.0.0.1:"+port,REMOTE_EX_TLS_CERT:certs.cert,REMOTE_EX_TLS_KEY:certs.key,REMOTE_EX_SERVER_ID:serverId,REMOTE_EX_ALLOWED_DEVICES_FILE:allow,REMOTE_EX_WEB_ORIGIN:fixture.url});
    const rendezvous=fileURLToPath(new URL("../../target/debug/remote-ex-rendezvous",import.meta.url));
    const preflight=JSON.parse(execFileSync(rendezvous,["--check-config"],{env,encoding:"utf8",timeout:10000}));
    assert.equal(preflight.mode,"wss");assert.equal(preflight.enrolled_count,2);assert.equal(preflight.web_origin_configured,true);
    server=spawn(rendezvous,[],{env,stdio:["ignore","pipe","inherit"]});await ready(server);
    host.stdin.write("REGISTER TEST\n");
    assert.equal(await read("ONLINE "), "ONLINE "+hostId);
    await read("PEER_KEY ");
    host.stdin.write(identity.key+"\n");await read("CONSENT (type ALLOW ROUTED TEST;");
    host.stdin.write("ALLOW ROUTED TEST\n");
    const result=await page.evaluate(async({endpoint,serverId,hostKey,id})=>{
        const lab=await import("/native-wss-fixture.js");return lab.run(endpoint,serverId,hostKey,id);
    },{endpoint,serverId,hostKey,id:identity.id});
    for(const name of ["storedIdentity","nativeRoute","quality","ice","dtls","video","release","closed"])assert.equal(result[name],true,name);
    assert(result.frames>=30);
    await read("CONNECTED ROUTED TEST");
    await read("TEST_INPUT_HELD");
    const pass=await read("PASS ROUTED HOST:");
    assert.match(pass,/sent_frames=\d+, input_released=true$/);
    const [code]=await hostExit;assert.equal(code,0);
    assert.equal(await page.evaluate(()=>window.__forbiddenCalls),0);
    assert.equal(await page.evaluate(()=>document.querySelector("video").srcObject===null),true);
    assert.deepEqual(unexpected,[]);assert.deepEqual(errors,[]);
    await context.close();
    console.log("PASS independently enrolled native GStreamer VP8 host -> Chromium: persistent browser identity, Noise, exact SDP, ICE, actual DTLS, 960x540 decoded pixels, fixed inert input release, route closure");
}finally{
    await browser?.close();
    await stopChild(host);
    await stopChild(server);
    if(fixture)await new Promise(resolve=>fixture.server.close(resolve));
    rmSync(root,{recursive:true,force:true});
}
