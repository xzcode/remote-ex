// Real Web controller -> independent GStreamer -> Rust Dispatcher.
// Only generated pixels and an inert backend; no real desktop/OS events.
import assert from "node:assert/strict";
import {chromium} from "playwright";
import {serve} from "./serve.mjs";
import {spawn,spawnSync} from "node:child_process";
import {mkdtempSync,mkdirSync,writeFileSync,rmSync} from "node:fs";
import {tmpdir} from "node:os";
import path from "node:path";
import {fileURLToPath} from "node:url";
import {randomBytes,createHash} from "node:crypto";
import {createInterface} from "node:readline";
import {once} from "node:events";
import {freePort,certificates,installRoot,ready,stopChild} from "./wss-test-support.mjs";
assert.equal(process.env.RUNNER_ENVIRONMENT,"github-hosted");
function reader(child){
    const lines=createInterface({input:child.stdout})[Symbol.asyncIterator]();
    return async(prefix,limit=15000)=>{
        let timer;
        try{
            const line=await Promise.race([lines.next(),new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error("Inert host output timeout: "+prefix)),limit);})]);
            assert(!line.done&&line.value.length<=1024,"Inert host output unavailable");
            assert(line.value.startsWith(prefix),"Expected inert host output "+prefix+"; got "+line.value);
            return line.value;
        }finally{clearTimeout(timer);}
    };
}
const root=mkdtempSync(path.join(tmpdir(),"remote-ex-browser-control-"));
let browser,web,server,host;
try{
    const home=path.join(root,"home");mkdirSync(home,{mode:0o700});
    const certs=certificates(root);installRoot(home,certs.ca);
    const port=await freePort(),endpoint="wss://127.0.0.1:"+port+"/v1/ws";
    web=await serve(0,null,endpoint,{stun:null,turn:[],relayOnly:false,control:true,extended:false});
    browser=await chromium.launch({headless:true,env:{...process.env,HOME:home,XDG_CONFIG_HOME:path.join(home,".config"),XDG_DATA_HOME:path.join(home,".local/share")},args:["--disable-background-networking","--disable-features=WebRtcHideLocalIpsWithMdns","--autoplay-policy=no-user-gesture-required"]});
    const context=await browser.newContext({permissions:[],ignoreHTTPSErrors:false,viewport:{width:1440,height:1000}});
    const errors=[],unexpected=[],sockets=[];
    await context.route("**/*",r=>{if(new URL(r.request().url()).origin!==web.url){unexpected.push("Unexpected HTTP origin");return r.abort();}return r.continue();});
    await context.addInitScript(()=>{
        window.__forbiddenCalls=0;
        if(navigator.mediaDevices)for(const name of ["getDisplayMedia","getUserMedia"])Object.defineProperty(navigator.mediaDevices,name,{configurable:true,value:async()=>{window.__forbiddenCalls++;throw Error("Forbidden capture");}});
    });
    const page=await context.newPage();
    page.on("pageerror",e=>errors.push(e.message));
    page.on("websocket",ws=>{sockets.push(ws.url());if(ws.url()!==endpoint)unexpected.push("Unexpected websocket");});
    await page.goto(web.url+"/identity.html");
    await page.locator("#identity-create").click();
    await page.waitForFunction(()=>document.querySelector("#identity-status").dataset.state==="verified");
    const controllerId=await page.locator("#device-id").inputValue(),controllerKey=await page.locator("#public-key").inputValue();
    assert.equal(createHash("sha256").update(Buffer.from(controllerKey,"hex")).digest("hex"),controllerId);
    await page.goto(web.url+"/view.html");
    await page.waitForFunction(()=>document.querySelector("#view-status").dataset.state==="idle"&&document.querySelector("#view-id").value!=="");
    assert.equal(await page.locator("#view-id").inputValue(),controllerId);
    const exe=fileURLToPath(new URL("../../crates/media/target/debug/browser_control_fixture",import.meta.url));
    const serverId=randomBytes(32).toString("hex");
    const env=Object.fromEntries(Object.entries(process.env).filter(([name])=>!name.startsWith("REMOTE_EX_")));
    const refused=spawnSync(exe,["--host","wss://example.invalid/v1/ws",serverId,"--ca",certs.ca],{env,encoding:"utf8",timeout:5000});
    assert.notEqual(refused.status,0);assert.match(refused.stderr,/Only numeric IPv4 loopback WSS permitted/);
    host=spawn(exe,["--host",endpoint,serverId,"--ca",certs.ca],{env,stdio:["pipe","pipe","inherit"]});
    const exited=once(host,"exit");exited.catch(()=>{});
    const read=reader(host),hostKey=(await read("KEY ")).slice(4),hostId=createHash("sha256").update(Buffer.from(hostKey,"hex")).digest("hex");
    const allow=path.join(root,"allowed.txt");writeFileSync(allow,[hostId,controllerId].join("\n")+"\n",{mode:0o600});
    Object.assign(env,{REMOTE_EX_SIGNAL_BIND:"127.0.0.1:"+port,REMOTE_EX_TLS_CERT:certs.cert,REMOTE_EX_TLS_KEY:certs.key,REMOTE_EX_SERVER_ID:serverId,REMOTE_EX_ALLOWED_DEVICES_FILE:allow,REMOTE_EX_WEB_ORIGIN:web.url});
    const rendezvous=fileURLToPath(new URL("../../target/debug/remote-ex-rendezvous",import.meta.url));
    server=spawn(rendezvous,[],{env,stdio:["ignore","pipe","inherit"]});await ready(server);
    // Save/load connection details; changing a favorite never stores a grant.
    await page.locator("#view-server").fill(serverId);
    await page.locator("#view-host").fill(hostKey);
    await page.locator("#connection-label").fill("Inert CI fixture");
    await page.locator("#view-control").check();await page.locator("#view-consent").check();
    await page.locator("#connection-save").click();
    await page.locator("#saved-connection").selectOption("");
    await page.locator("#saved-connection").selectOption({label:"Inert CI fixture"});
    assert(!(await page.locator("#view-control").isChecked()));assert(!(await page.locator("#view-consent").isChecked()));
    assert(await page.locator("#view-connect").isDisabled());assert.deepEqual(sockets,[]);
    await page.locator("#view-control").check();await page.locator("#view-consent").check();
    host.stdin.write("REGISTER TEST\n");assert.equal(await read("ONLINE "),"ONLINE "+hostId);
    await read("PEER_KEY ");host.stdin.write(controllerKey+"\n");await read("CONSENT (type ALLOW BROWSER INERT TEST;");host.stdin.write("ALLOW BROWSER INERT TEST\n");
    await page.locator("#view-connect").click();
    await page.waitForFunction(()=>["viewing","error"].includes(document.querySelector("#view-status").dataset.state),{},{timeout:65000});
    assert.equal(await page.locator("#view-status").getAttribute("data-state"),"viewing",await page.locator("#view-status").textContent());
    await read("CONNECTED BROWSER INERT CONTROL");
    await page.waitForFunction(()=>Number(document.querySelector("#view-frames").textContent)>=10,{},{timeout:10000});
    assert.equal(await page.locator("#view-resolution").textContent(),"960 × 540");
    const surface=page.locator("#view-video");
    assert(await page.locator("#view-tools-toggle").isVisible());
    await page.locator("#view-tools-toggle").click();
    assert(await page.locator("#view-tools").isHidden());
    assert(await page.locator("#view-disconnect").isVisible());
    assert.equal(await page.locator("#view-tools-toggle").getAttribute("aria-expanded"),"false");
    await page.locator("#view-tools-toggle").click();
    assert(await page.locator("#view-tools").isVisible());
    // Decoded media and host permission do not activate input implicitly.
    assert(await page.locator("#input-disable").isDisabled());
    await surface.click();await page.keyboard.press("a");
    await page.locator("#input-enable").click();
    await page.waitForFunction(()=>document.activeElement===document.querySelector("#view-video")&&!document.querySelector("#input-disable").disabled);
    await surface.scrollIntoViewIfNeeded();
    const rect=await surface.boundingBox();assert(rect&&rect.width>0&&rect.height>0);
    await page.mouse.move(rect.x+rect.width/2,rect.y+rect.height/2);
    await page.mouse.down({button:"left"});
    await page.keyboard.down("a");
    await page.mouse.wheel(0,120);
    // Keep both key and button held; focus loss must release both through
    // the actual data channel, not merely clear browser-local state.
    await page.locator("#view-disconnect").focus();
    await page.waitForFunction(()=>document.querySelector("#input-disable").disabled);
    await read("BROWSER_INERT_RELEASE_VERIFIED",10000);
    await page.keyboard.up("a");await page.mouse.up({button:"left"});
    await page.mouse.move(rect.x+rect.width/2,rect.y+rect.height/2);
    await page.keyboard.press("a");
    await page.locator("#view-disconnect").click();
    await page.waitForFunction(()=>["stopped","error"].includes(document.querySelector("#view-status").dataset.state),{},{timeout:12000});
    assert.equal(await page.locator("#view-status").getAttribute("data-state"),"stopped",await page.locator("#view-status").textContent());
    assert(await page.evaluate(()=>document.querySelector("#view-video").srcObject===null));
    await read("PASS BROWSER INERT CONTROL:");
    const [code]=await exited;assert.equal(code,0);
    assert.equal(await page.evaluate(()=>window.__forbiddenCalls),0);
    assert.deepEqual(errors,[]);assert.deepEqual(unexpected,[]);assert.deepEqual(sockets,[endpoint]);
    await context.close();
    console.log("PASS real Chromium profile-2 WSS/Noise/SDP/DTLS and native Dispatcher: explicit activation, geometry, click/key/wheel and focus-loss release, fresh saved-connection consent, route closed; inert only");
}finally{
    await browser?.close();await stopChild(host);await stopChild(server);
    if(web)await new Promise(resolve=>web.server.close(resolve));
    rmSync(root,{recursive:true,force:true});
}
