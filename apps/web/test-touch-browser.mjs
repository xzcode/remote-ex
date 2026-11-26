// Trusted Chromium mobile touch events; generated video and inert packet sink.
// No remote host, OS input, signaling or local screen capture.
import assert from "node:assert/strict";
import {chromium} from "playwright";
import {serve} from "./serve.mjs";
const {server,url}=await serve(0,null,"wss://127.0.0.1:9443/v1/ws",{stun:null,turn:[],relayOnly:false,control:true});
let browser;
try{
    browser=await chromium.launch({headless:true,args:["--disable-background-networking","--autoplay-policy=no-user-gesture-required"]});
    const context=await browser.newContext({permissions:[],hasTouch:true,isMobile:true,viewport:{width:430,height:900}});
    const page=await context.newPage(),errors=[],sockets=[];
    page.on("pageerror",e=>errors.push(e.message));page.on("websocket",ws=>sockets.push(ws.url()));
    await page.goto(url+"/view.html");
    await page.waitForFunction(()=>document.querySelector("#view-status").dataset.state==="idle"&&!document.querySelector("#view-recheck").disabled);
    const video=page.locator("#view-video");
    await page.evaluate(async()=>{
        const canvas=document.createElement("canvas");canvas.width=960;canvas.height=540;
        const ctx=canvas.getContext("2d");ctx.fillStyle="#287651";ctx.fillRect(0,0,960,540);
        const stream=canvas.captureStream(5),v=document.querySelector("#view-video");
        document.querySelector("#view-placeholder").hidden=true;
        v.hidden=false;v.muted=true;v.srcObject=stream;await v.play();
        window.__touchFixture={stream,packets:[],input:null};
    });
    await video.scrollIntoViewIfNeeded();
    const rect=await video.boundingBox();assert(rect&&rect.width>100&&rect.height>100);
    const x=Math.round(rect.x+rect.width/2),y=Math.round(rect.y+rect.height/2);
    const enable=mode=>page.evaluate(async mode=>{
        const {InputEncoder,SurfaceInput}=await import("/input.js");
        const f=window.__touchFixture;f.input?.dispose();f.packets=[];
        f.input=new SurfaceInput({video:document.querySelector("#view-video"),geometry:{x:120,y:0,width:720,height:540},
            encoder:new InputEncoder(new Uint8Array(16).fill(7)),send:bytes=>f.packets.push([...bytes]),onFailure:()=>{throw Error("Inert sink failed");},onDisabled:()=>{},touchMode:mode});
        f.input.enable();
        if(!f.input.active)throw Error("Touch surface focus unavailable");
    },mode);
    const packets=()=>page.evaluate(()=>window.__touchFixture.packets);
    const kinds=values=>values.map(v=>{
        assert.equal(v[20],24);assert(v[21]<128);return v[22];
    });
    const client=await context.newCDPSession(page);
    const point=(x,y,id=1)=>({x,y,id,radiusX:2,radiusY:2,force:1});
    const touch=(type,points)=>client.send("Input.dispatchTouchEvent",{type,touchPoints:points});
    await enable("tap");await page.touchscreen.tap(x,y);
    assert.deepEqual(kinds(await packets()),[82,90,90]);assert.deepEqual((await packets())[1].slice(-4),[8,1,16,1]);
    await enable("right");await page.touchscreen.tap(x,y);
    assert.deepEqual(kinds(await packets()),[82,90,90]);assert.deepEqual((await packets())[1].slice(-4),[8,3,16,1]);
    await enable("drag");await touch("touchStart",[point(x,y)]);
    await page.waitForTimeout(60);await touch("touchMove",[point(x+25,y+15)]);
    await touch("touchEnd",[]);
    let v=await packets();assert.deepEqual(kinds(v).slice(0,2),[82,90]);assert.equal(kinds(v).at(-1),90);assert.deepEqual(v.at(-1).slice(-4),[8,1,16,0]);
    await enable("scroll");await touch("touchStart",[point(x,y)]);
    await page.waitForTimeout(60);await touch("touchMove",[point(x,y-35)]);
    await touch("touchEnd",[]);
    assert.deepEqual(kinds(await packets()),[82,106]);
    await enable("drag");await touch("touchStart",[point(x,y)]);
    await touch("touchStart",[point(x,y),point(x+20,y+20,2)]);
    await page.waitForFunction(()=>!window.__touchFixture.input.active);
    v=await packets();assert.deepEqual(kinds(v).slice(0,2),[82,90]);
    assert.equal(kinds(v).filter(n=>n===90).length,1);
    assert.equal(kinds(v).at(-1),114);assert.deepEqual(v.at(-1).slice(-2),[114,0]);
    await touch("touchEnd",[]);assert.deepEqual(await packets(),v);
    await enable("drag");await touch("touchStart",[point(x,y)]);await touch("touchCancel",[]);
    await page.waitForFunction(()=>!window.__touchFixture.input.active);
    assert.deepEqual(kinds(await packets()),[82,90,114]);
    // A different physical pointer type cannot own the same button namespace.
    await enable("tap");await page.mouse.click(x,y);
    await page.waitForFunction(()=>!window.__touchFixture.input.active);
    assert.deepEqual(kinds(await packets()),[114]);
    await enable("mouse");await page.touchscreen.tap(x,y);
    await page.waitForFunction(()=>!window.__touchFixture.input.active);
    assert.deepEqual(kinds(await packets()),[114]);
    await page.evaluate(()=>{
        const f=window.__touchFixture;f.input.dispose();f.stream.getTracks().forEach(t=>t.stop());
        const v=document.querySelector("#view-video");v.srcObject=null;v.hidden=true;document.querySelector("#view-placeholder").hidden=false;delete window.__touchFixture;
    });
    await client.detach();await context.close();
    assert.deepEqual(errors,[]);assert.deepEqual(sockets,[]);
    console.log("PASS trusted mobile Chromium tap/right/drag/scroll and second-finger/cancel ReleaseAll, inert only");
}finally{await browser?.close();await new Promise(resolve=>server.close(resolve));}
