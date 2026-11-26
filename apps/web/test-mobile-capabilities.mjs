// Real Chromium mobile landing and fail-closed missing-feature branch.
import assert from "node:assert/strict";
import {chromium} from "playwright";
import {serve} from "./serve.mjs";
const {server,url}=await serve(0,null,"wss://127.0.0.1:9443/v1/ws",{stun:null,turn:[],relayOnly:false,control:false,extended:false});
let browser;
try{
    browser=await chromium.launch({headless:true,args:["--disable-background-networking"]});
    for(const missing of [false,true]){
        const context=await browser.newContext({permissions:[],hasTouch:true,viewport:{width:400,height:800},isMobile:true});
        const page=await context.newPage(),sockets=[],outside=[];
        await context.route("**/*",r=>{if(new URL(r.request().url()).origin!==url){outside.push("outside");return r.abort();}return r.continue();});
        page.on("websocket",ws=>sockets.push(ws.url()));
        if(missing)await context.addInitScript(()=>Object.defineProperty(globalThis,"RTCPeerConnection",{value:undefined,configurable:true}));
        await page.goto(url+"/mobile.html");
        await page.waitForFunction(missing=>["ready","unsupported","error"].includes(document.querySelector("#mobile-status").dataset.state)&&document.querySelector("#mobile-policy").textContent.includes(missing?"Identity management is local only.":"Operator endpoint configured."),missing,{timeout:45000});
        const report=await page.locator("#mobile-checks li").evaluateAll(items=>items.map(i=>[i.dataset.capability,i.dataset.pass,i.dataset.code]));
        assert.equal(report.length,6);
        assert.deepEqual(report.map(x=>x[0]),["secure","ed25519","wasm","storage","webrtc","pointer"]);
        if(missing){
            assert.equal(await page.locator("#mobile-status").getAttribute("data-state"),"unsupported");
            assert.deepEqual(report.find(x=>x[0]==="webrtc"),["webrtc","false","webrtc_api"]);
            assert(await page.locator("#mobile-connect").isHidden());
        }else{
            assert.equal(await page.locator("#mobile-status").getAttribute("data-state"),"ready",JSON.stringify(report));
            assert(report.every(x=>x[1]==="true"));
            assert(await page.locator("#mobile-connect").isVisible());
        }
        await page.goto(url+"/identity.html");
        await page.waitForFunction(()=>!document.querySelector("#identity-create").disabled);
        assert.equal(await page.locator("#device-id").inputValue(),""); // probe never creates enrolled identity
        await page.locator("#identity-create").click();
        await page.waitForFunction(()=>document.querySelector("#identity-status").dataset.state==="verified");
        await page.goto(url+"/view.html");
        await page.waitForFunction(()=>["idle","error"].includes(document.querySelector("#view-status").dataset.state)&&!document.querySelector("#view-recheck").disabled,null,{timeout:45000});
        await page.locator("#view-server").fill("01".repeat(32));
        await page.locator("#view-host").fill("02".repeat(32));
        if(missing){
            assert.equal(await page.locator("#view-status").getAttribute("data-state"),"error");
            assert.match(await page.locator("#view-compat").textContent(),/webrtc: webrtc_api/);
            assert(await page.locator("#view-consent").isDisabled());
            assert(await page.locator("#view-connect").isDisabled());
            await page.locator("#view-recheck").click();
            await page.waitForFunction(()=>document.querySelector("#view-status").dataset.state==="error"&&!document.querySelector("#view-recheck").disabled,null,{timeout:45000});
            assert(await page.locator("#view-connect").isDisabled());
        }else{
            assert.equal(await page.locator("#view-status").getAttribute("data-state"),"idle");
            assert(await page.locator("#view-consent").isEnabled());
            await page.locator("#view-consent").check();
            assert(await page.locator("#view-connect").isEnabled());
            await page.locator("#view-recheck").click();
            await page.waitForFunction(()=>document.querySelector("#view-status").dataset.state==="idle"&&!document.querySelector("#view-recheck").disabled,null,{timeout:45000});
            assert(!(await page.locator("#view-consent").isChecked()));
            assert(await page.locator("#view-connect").isDisabled());
        }
        assert.deepEqual(sockets,[]);assert.deepEqual(outside,[]);
        await context.close();
    }
    // A simulated pre-137 crypto engine must reject identity creation
    // before the user clicks anything. Do not replace the algorithm.
    const legacy=await browser.newContext({permissions:[],hasTouch:true,viewport:{width:400,height:800},isMobile:true});
    await legacy.addInitScript(()=>{
        const original=SubtleCrypto.prototype.generateKey;
        Object.defineProperty(SubtleCrypto.prototype,"generateKey",{configurable:true,value:function(algorithm,...args){
            if((typeof algorithm==="string"?algorithm:algorithm?.name)==="Ed25519")return Promise.reject(new DOMException("Unavailable","NotSupportedError"));
            return original.call(this,algorithm,...args);
        }});
    });
    const lp=await legacy.newPage(),legacySockets=[];
    lp.on("websocket",()=>legacySockets.push("unexpected"));
    await lp.goto(url+"/mobile.html");
    await lp.waitForFunction(()=>document.querySelector("#mobile-status").dataset.state==="unsupported"&&document.querySelector("#mobile-policy").textContent.includes("Identity management is local only."),null,{timeout:45000});
    assert.match(await lp.locator("#mobile-status").textContent(),/137 or newer/);
    assert(await lp.locator("#mobile-connect").isHidden());
    await lp.goto(url+"/identity.html");
    await lp.waitForFunction(()=>document.querySelector("#identity-status").dataset.state==="unsupported");
    assert(await lp.locator("#identity-create").isDisabled());
    assert.equal(await lp.locator("#device-id").inputValue(),"");
    assert.match(await lp.locator("#identity-support").textContent(),/NotSupportedError/);
    assert(await lp.evaluate(async()=>!(await (await import("/identity.js")).readIdentity())));
    await lp.locator("#identity-refresh").click();
    await lp.waitForFunction(()=>document.querySelector("#identity-status").dataset.state==="unsupported");
    assert(await lp.locator("#identity-create").isDisabled());
    assert.deepEqual(legacySockets,[]);
    await legacy.close();
    console.log("PASS mobile Chromium capability, identity-only legacy Ed25519 refusal, direct viewer and recheck gates, no remote socket");
}finally{await browser?.close();await new Promise(resolve=>server.close(resolve));}
