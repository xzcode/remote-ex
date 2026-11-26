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
        assert.deepEqual(sockets,[]);assert.deepEqual(outside,[]);
        await context.close();
    }
    console.log("PASS real mobile Chromium capability/temporary key clone, no enrollment, missing WebRTC disables connection, no remote socket");
}finally{await browser?.close();await new Promise(resolve=>server.close(resolve));}
