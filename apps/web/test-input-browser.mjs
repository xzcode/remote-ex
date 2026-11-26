// Real Chromium focus/key/pointer lifecycle, in-memory inert byte sink only.
// No native host, OS input injection, signaling or desktop capture.
import assert from "node:assert/strict";
import {chromium} from "playwright";
import {serve} from "./serve.mjs";
const {server,url}=await serve(0,null,"wss://127.0.0.1:9443/v1/ws",{stun:null,turn:[],relayOnly:false,control:true});
let browser;
try{
    browser=await chromium.launch({headless:true,args:["--disable-background-networking","--autoplay-policy=no-user-gesture-required"]});
    const context=await browser.newContext({permissions:[],viewport:{width:1280,height:900}});
    const page=await context.newPage();const errors=[],sockets=[];
    page.on("pageerror",e=>errors.push(e.message));page.on("websocket",s=>sockets.push(s.url()));
    await page.goto(url+"/view.html");
    await page.waitForFunction(()=>document.querySelector("#view-status").dataset.state==="idle");
    assert(await page.locator("#control-option").isVisible());
    assert(!(await page.locator("#view-control").isChecked()));assert(await page.locator("#view-connect").isDisabled());
    await page.evaluate(async()=>{
        const {InputEncoder,SurfaceInput}=await import("/input.js");
        const canvas=document.createElement("canvas");canvas.width=960;canvas.height=540;
        const ctx=canvas.getContext("2d");ctx.fillStyle="#247b4b";ctx.fillRect(0,0,960,540);
        const stream=canvas.captureStream(5),video=document.querySelector("#view-video");
        video.hidden=false;video.muted=true;video.srcObject=stream;
        await video.play();
        const packets=[],encoder=new InputEncoder(new Uint8Array(16).fill(7));
        const input=new SurfaceInput({video,geometry:{x:120,y:0,width:720,height:540},encoder,send:bytes=>packets.push([...bytes]),onFailure:()=>{throw Error("Inert test sender failed");},onDisabled:()=>{}});
        window.__inputFixture={input,packets,stream,encoder};
        input.enable();
    });
    await page.waitForFunction(()=>window.__inputFixture.input.active&&document.querySelector("#view-video").videoWidth===960);
    const surface=page.locator("#view-video"),rect=await surface.boundingBox();
    assert(rect&&rect.width>0&&rect.height>0);
    // Synthetic DOM events are explicitly ignored by the trusted-event gate.
    await surface.dispatchEvent("keydown",{code:"KeyA",key:"a"});
    assert.equal(await page.evaluate(()=>window.__inputFixture.packets.length),0);
    await page.mouse.move(rect.x+rect.width/2,rect.y+rect.height/2);
    await page.mouse.down({button:"left"});await page.keyboard.down("a");
    await page.mouse.wheel(0,120);await page.mouse.up({button:"left"});
    const before=await page.evaluate(()=>window.__inputFixture.packets.length);
    assert(before>=5,"Expected move/button/key/wheel packets");
    // Real focus loss with a held key must emit ReleaseAll and disable input.
    await page.locator("#view-server").focus();
    await page.waitForFunction(()=>!window.__inputFixture.input.active);
    const after=await page.evaluate(()=>window.__inputFixture.packets);
    assert.deepEqual(after.at(-1).slice(-2),[114,0]);
    assert.equal(after.length,before+1);
    await page.keyboard.up("a");await page.mouse.move(rect.x+10,rect.y+10);
    assert.equal(await page.evaluate(()=>window.__inputFixture.packets.length),after.length);
    await page.evaluate(()=>{
        const {input,stream}=window.__inputFixture;input.dispose();stream.getTracks().forEach(t=>t.stop());
        const v=document.querySelector("#view-video");v.srcObject=null;v.hidden=true;delete window.__inputFixture;
    });
    assert.deepEqual(errors,[]);assert.deepEqual(sockets,[]);
    await context.close();
    console.log("PASS Chromium trusted input events, viewport mapping, held-key release on focus loss, no post-disable packets, no socket or OS input");
}finally{await browser?.close();await new Promise(resolve=>server.close(resolve));}
