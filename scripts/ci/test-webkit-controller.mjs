// Actual staged HTTPS assets in pinned WebKit. Local CI CA, no TLS bypass,
// production credentials, remote host, capture or OS input.
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import {createRequire} from "node:module";
import {stage} from "../stage-mobile-client.mjs";
import {prepare,report,listen} from "../../deploy/controller-site.mjs";
import {serve} from "../../apps/web/serve.mjs";
import {freePort} from "../../apps/web/wss-test-support.mjs";
const require=createRequire(new URL("../../apps/web/package.json",import.meta.url));
const {webkit}=require("playwright");
assert.equal(process.env.RUNNER_ENVIRONMENT,"github-hosted");
const localMedia=process.argv.slice(2).includes("--local-media");
assert(process.argv.slice(2).every(v=>v==="--local-media"));
const dir=await fs.mkdtemp(path.join(os.tmpdir(),"remote-ex-webkit-"));
const cert=process.env.REMOTE_EX_WEBKIT_CERT,key=process.env.REMOTE_EX_WEBKIT_KEY;
assert(cert&&key&&path.isAbsolute(cert)&&path.isAbsolute(key));
let browser,site,lab;
const results={schema:1,engine:"webkit",https:false,capabilities:[],identity:false,persistence:false,deletion:false,viewer_gate:false,local_media_attempted:localMedia,local_media:false,dtls:false,decoded_frames:false,no_signaling:false};
const phase=name=>console.log("REMOTE EX WEBKIT PHASE: "+name);
try{
    phase("stage");
    const root=path.join(dir,"site"),port=await freePort(),unused=await freePort();
    // The configured WSS endpoint is deliberately not started.
    await stage(root,{endpoint:"wss://127.0.0.1:"+unused+"/v1/ws",ice:{stun:null,turn:[],relayOnly:false},control:false,extended:false});
    const prepared=await prepare(root,{REMOTE_EX_SITE_HOST:"localhost",REMOTE_EX_SITE_PORT:String(port),REMOTE_EX_SITE_TLS_CERT:cert,REMOTE_EX_SITE_TLS_KEY:key});
    assert.equal(report(prepared).network_started,false);
    site=await listen(prepared);const origin="https://localhost:"+port;
    if(localMedia)lab=await serve(0);
    phase("trusted_https");
    browser=await webkit.launch({headless:true});
    const context=await browser.newContext({ignoreHTTPSErrors:false,permissions:[],viewport:{width:390,height:844},isMobile:true,hasTouch:true});
    const sockets=[],outside=[],pageErrors=[];
    await context.route("**/*",r=>{
        if(![origin,lab?.url].includes(new URL(r.request().url()).origin)){outside.push("unexpected HTTP origin");return r.abort();}
        return r.continue();
    });
    await context.addInitScript(()=>{
        window.__forbiddenCalls=0;
        if(navigator.mediaDevices)for(const name of ["getDisplayMedia","getUserMedia"]){
            try{Object.defineProperty(navigator.mediaDevices,name,{configurable:true,value:async()=>{window.__forbiddenCalls++;throw Error("Capture forbidden");}});}catch{}
        }
    });
    const page=await context.newPage();
    page.on("websocket",()=>sockets.push("unexpected socket"));
    page.on("pageerror",e=>pageErrors.push(e.name??"Error"));
    const response=await page.goto(origin+"/mobile.html",{waitUntil:"domcontentloaded",timeout:20000});
    assert(response?.ok(),"Trusted HTTPS response unavailable");
    assert(await page.evaluate(()=>isSecureContext&&location.protocol==="https:"));
    assert.match(response.headers()["content-security-policy"]??"",/default-src 'none'/);
    results.https=true;phase("capabilities");
    await page.waitForFunction(()=>["ready","unsupported","error"].includes(document.querySelector("#mobile-status")?.dataset.state)&&document.querySelector("#mobile-policy")?.textContent!=="No default public server.",null,{timeout:45000});
    const caps=await page.locator("#mobile-checks li").evaluateAll(items=>items.map(i=>({id:i.dataset.capability,ok:i.dataset.pass==="true",code:i.dataset.code})));
    assert.deepEqual(caps.map(x=>x.id),["secure","ed25519","wasm","storage","webrtc","pointer"]);
    assert(caps.every(x=>/^[A-Za-z_]{1,40}$/.test(x.code)));
    results.capabilities=caps;
    console.log("WEBKIT_CAPABILITIES "+JSON.stringify(caps));
    const supported=caps.every(x=>x.ok);
    assert.equal(await page.locator("#mobile-connect").isVisible(),supported);
    const canStore=caps.find(x=>x.id==="ed25519").ok&&caps.find(x=>x.id==="storage").ok;
    if(canStore){
        phase("identity");
        await page.goto(origin+"/identity.html");
        await page.waitForFunction(()=>!document.querySelector("#identity-create").disabled);
        await page.locator("#identity-create").click();
        await page.waitForFunction(()=>["verified","error"].includes(document.querySelector("#identity-status").dataset.state));
        assert.equal(await page.locator("#identity-status").getAttribute("data-state"),"verified");
        const device=await page.locator("#device-id").inputValue();assert.match(device,/^[0-9a-f]{64}$/);
        results.identity=await page.evaluate(async()=>{
            const i=await (await import("/identity.js")).readIdentity();
            let refused=false;try{await crypto.subtle.exportKey("pkcs8",i.key.privateKey);}catch{refused=true;}
            return refused&&!i.key.privateKey.extractable;
        });
        await page.reload();
        await page.waitForFunction(()=>document.querySelector("#identity-status").dataset.state==="verified");
        results.persistence=(await page.locator("#device-id").inputValue())===device;
        assert(results.identity&&results.persistence);
    }
    phase("direct_view_gate");
    await page.goto(origin+"/view.html");
    await page.waitForFunction(()=>["idle","error"].includes(document.querySelector("#view-status").dataset.state)&&!document.querySelector("#view-recheck").disabled,null,{timeout:45000});
    await page.locator("#view-server").fill("01".repeat(32));
    await page.locator("#view-host").fill("02".repeat(32));
    if(supported&&canStore){
        assert.equal(await page.locator("#view-status").getAttribute("data-state"),"idle");
        await page.locator("#view-consent").check();assert(await page.locator("#view-connect").isEnabled());
        await page.locator("#view-recheck").click();
        await page.waitForFunction(()=>document.querySelector("#view-status").dataset.state==="idle"&&!document.querySelector("#view-recheck").disabled,null,{timeout:45000});
        assert(!(await page.locator("#view-consent").isChecked()));assert(await page.locator("#view-connect").isDisabled());
    }else{
        assert.equal(await page.locator("#view-status").getAttribute("data-state"),"error");
        assert(await page.locator("#view-consent").isDisabled());assert(await page.locator("#view-connect").isDisabled());
    }
    results.viewer_gate=true;
    if(canStore){
        phase("identity_delete");
        await page.goto(origin+"/identity.html");
        await page.waitForFunction(()=>document.querySelector("#identity-status").dataset.state==="verified");
        await page.locator("#identity-confirm").check();await page.locator("#identity-delete").click();
        await page.waitForFunction(()=>!document.querySelector("#identity-create").disabled);
        results.deletion=(await page.locator("#device-id").inputValue())==="";assert(results.deletion);
    }
    // Existing local-only signed media fixture, separate from staged assets.
    // Actual DTLS certificate stats must match; no SDP-only fallback.
    if(supported&&localMedia){
        phase("authenticated_local_media");
        await page.goto(lab.url);
        await page.locator("#consent").check();await page.locator("#start").click();
        await page.waitForFunction(()=>["verified","error","stopped"].includes(document.querySelector("#status").dataset.state),null,{timeout:35000});
        const state=await page.locator("#status").getAttribute("data-state");
        const checks=await page.locator("[data-step]").evaluateAll(items=>items.map(i=>({step:i.dataset.step,ok:i.dataset.pass==="true"})));
        assert(checks.every(x=>["vectors","identity","noise","sdp","dtls","video"].includes(x.step)));
        const failure=await page.locator("#error").evaluate(e=>({
            stage:(e.dataset.stage??"").slice(0,100),
            detail:(e.textContent??"").replace(/[^a-zA-Z0-9 _.,:;()/-]/g," ").slice(0,220)
        }));
        // Only disposable fixture diagnostics; no configured URLs, keys or
        // SDP are logged. The stage is fixed UI text, details sanitized.
        console.log("WEBKIT_LOCAL_MEDIA "+JSON.stringify({state,checks,failure}));
        results.dtls=checks.some(v=>v.step==="dtls"&&v.ok);
        results.decoded_frames=checks.some(v=>v.step==="video"&&v.ok)&&Number(await page.locator("#frames").textContent())>=3;
        results.local_media=state==="verified"&&results.dtls&&results.decoded_frames;
        if(results.local_media){
            await page.locator("#stop").click();
            assert(await page.evaluate(()=>document.querySelector("#remote").srcObject===null));
        }
        assert.equal(await page.evaluate(()=>window.__forbiddenCalls),0);
    }
    results.no_signaling=sockets.length===0&&outside.length===0;
    assert(results.no_signaling,"Unexpected external connection");assert.deepEqual(pageErrors,[]);
    // Correctly blocking an unsupported engine is not media compatibility.
    assert(supported&&results.identity&&results.persistence&&results.deletion&&(!localMedia||results.local_media),
        "WebKit compatibility incomplete; inspect WEBKIT_CAPABILITIES and WEBKIT_LOCAL_MEDIA");
    console.log("PASS REMOTE EX WEBKIT: staged HTTPS, identity lifecycle, direct-view consent"+(localMedia?", signed local synthetic media verified":"; native receiver tested separately"));
}finally{
    console.log("WEBKIT_ACCEPTANCE "+JSON.stringify(results));
    await browser?.close();
    if(site)await new Promise(r=>site.close(r));
    if(lab)await new Promise(r=>lab.server.close(r));
    await fs.rm(dir,{recursive:true,force:true});
}
