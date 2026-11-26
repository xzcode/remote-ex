// Real Chromium test on a disposable runner, not a physical-device test.
// Browser permissions are deliberately denied and media/input APIs trapped.
import assert from "node:assert/strict";
import {chromium} from "playwright";
import {serve} from "./serve.mjs";
import {spawn} from "node:child_process";
import {createInterface} from "node:readline";
import {fileURLToPath} from "node:url";
import {once} from "node:events";
async function nativeInterop(page){
 const exe=fileURLToPath(new URL("./crypto/target/debug/native_fixture"+(process.platform==="win32"?".exe":""),import.meta.url));
 const child=spawn(exe,[],{stdio:["pipe","pipe","inherit"]});
 const exited=once(child,"exit");exited.catch(()=>{});
 const lines=createInterface({input:child.stdout})[Symbol.asyncIterator]();
 const read=async()=>{let timer;try{const row=await Promise.race([lines.next(),new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error("Native fixture response timed out")),6000);})]);assert(!row.done&&row.value.length<=8192);return JSON.parse(row.value);}finally{clearTimeout(timer);}};
 const write=value=>child.stdin.write(JSON.stringify(value)+"\n");
 try{
  const hello=await read();
  const first=await page.evaluate(async hello=>{
   const wasm=await import("./pkg/noise.js");await wasm.default();
   const {newFixtureIdentity,random}=await import("./crypto.js");
   const controller=await newFixtureIdentity(),session=random(16),nonce=random(32);
   const peer=new wasm.FixturePeer(true,session,controller.pub,Uint8Array.from(hello.host),nonce,Uint8Array.from(hello.nonce));
   window.__nativeFixture={controller,peer};
   return {controller:[...controller.pub],session:[...session],nonce:[...nonce],handshake:[...peer.write_handshake()]};
  },hello);write(first);
  const second=await read();
  const finish=await page.evaluate(async response=>{
   const {controller,peer}=window.__nativeFixture;
   peer.read_handshake(Uint8Array.from(response.handshake));
   const sig=new Uint8Array(await crypto.subtle.sign("Ed25519",controller.key.privateKey,peer.signature_message()));
   const proof=peer.send_proof(sig);peer.receive_proof(Uint8Array.from(response.proof));
   if(!peer.ready())throw Error("Cross-runtime proof gate failed");
   return {proof:[...proof],record:[...peer.send_fixture()]};
  },second);write(finish);child.stdin.end();
  const done=await read();assert.equal(done.fixture_verified,true);
  assert.equal(await page.evaluate(done=>{const {peer}=window.__nativeFixture;peer.receive_fixture(Uint8Array.from(done.record));return peer.ready();},done),true);
  const [code]=await exited;assert.equal(code,0);
  console.log("PASS native Rust responder ↔ Chromium WASM initiator: real mutual Noise signatures and fixed encrypted record");
 }finally{
  if(child.exitCode===null)child.kill();
  await page.evaluate(()=>{const item=window.__nativeFixture;if(item){item.peer.close();item.peer.free();delete window.__nativeFixture;}}).catch(()=>{});
 }
}
const {server,url}=await serve();
let browser;
try{
 browser=await chromium.launch({headless:true,args:["--disable-background-networking","--autoplay-policy=no-user-gesture-required"]});
 const context=await browser.newContext({permissions:[]});
 const outside=[];
 await context.route("**/*",route=>{
   if(new URL(route.request().url()).origin!==url){outside.push(route.request().url());return route.abort();}
   return route.continue();
 });
 await context.addInitScript(()=>{
   window.__forbiddenCalls=0;
   if(navigator.mediaDevices)for(const name of ["getDisplayMedia","getUserMedia"])Object.defineProperty(navigator.mediaDevices,name,{configurable:true,value:async()=>{window.__forbiddenCalls++;throw Error("Forbidden fixture capture");}});
 });
 const page=await context.newPage();const errors=[];
 page.on("pageerror",e=>errors.push(String(e.message)));
 await page.goto(url,{waitUntil:"networkidle"});
 await nativeInterop(page);
 // The separate identity page has no network/session capabilities.
 await page.goto(url+"/identity.html",{waitUntil:"networkidle"});
 await page.waitForFunction(()=>!document.querySelector("#identity-create").disabled);
 assert(await page.locator("#identity-delete").isDisabled());
 await page.locator("#identity-create").click();
 await page.waitForFunction(()=>document.querySelector("#identity-status").dataset.state==="verified");
 const first=await page.locator("#device-id").inputValue(),pub=await page.locator("#public-key").inputValue();
 assert.match(first,/^[0-9a-f]{64}$/);assert.match(pub,/^[0-9a-f]{64}$/);
 assert(await page.evaluate(async()=>{
     const v=await import("/identity.js"),i=await v.readIdentity();
     let exportRejected=false,replacementRejected=false;
     try{await crypto.subtle.exportKey("pkcs8",i.key.privateKey);}catch{exportRejected=true;}
     try{await v.createIdentity();}catch{replacementRejected=true;}
     return exportRejected&&replacementRejected&&(await v.readIdentity()).id===i.id;
 }));
 await page.reload({waitUntil:"networkidle"});
 await page.waitForFunction(()=>document.querySelector("#identity-status").dataset.state==="verified");
 assert.equal(await page.locator("#device-id").inputValue(),first);
 assert.equal(await page.locator("#public-key").inputValue(),pub);
 const otherContext=await browser.newContext({permissions:[]});
 try{
     const other=await otherContext.newPage();await other.goto(url+"/identity.html");
     await other.waitForFunction(()=>!document.querySelector("#identity-create").disabled);
     assert.equal(await other.locator("#device-id").inputValue(),"");
 }finally{await otherContext.close();}
 await page.locator("#identity-confirm").check();await page.locator("#identity-delete").click();
 await page.waitForFunction(()=>!document.querySelector("#identity-create").disabled);
 assert.equal(await page.locator("#device-id").inputValue(),"");
 await page.reload({waitUntil:"networkidle"});
 await page.waitForFunction(()=>!document.querySelector("#identity-create").disabled);
 assert.equal(await page.locator("#public-key").inputValue(),"");
 await page.goto(url,{waitUntil:"networkidle"});
 console.log("PASS browser controller vault: explicit creation, non-extractable key, no overwrite, reload persistence, context isolation and deletion");
 assert(await page.locator("#start").isDisabled());
 await page.locator("#consent").check();await page.locator("#start").click();
 await page.waitForFunction(()=>["verified","error","stopped"].includes(document.querySelector("#status").dataset.state),{},{timeout:32000});
 const outcome=await page.locator("#status").getAttribute("data-state");
 assert.equal(outcome,"verified",await page.locator("#error").textContent());
 assert.equal(await page.locator("[data-pass=true]").count(),6);
 assert(Number(await page.locator("#frames").textContent())>=3);
 assert.equal(await page.locator("#resolution").textContent(),"320 × 180");
 assert.equal(await page.locator("#transport").textContent(),"DTLS verified");
 assert.equal(await page.evaluate(()=>window.__forbiddenCalls),0);
 assert.deepEqual(outside,[]);
 await page.locator("#stop").click();
 assert.equal(await page.locator("#status").getAttribute("data-state"),"stopped");
 assert(await page.evaluate(()=>document.querySelector("video").srcObject===null));
 assert(await page.locator("#start").isDisabled());
 assert(await page.locator("#stop").isDisabled());
 // Cancel during a second explicit fixture; the old session cannot attach
 // tracks or update UI after its generation has been revoked.
 await page.locator("#consent").check();await page.locator("#start").click();await page.locator("#stop").click();
 await page.waitForTimeout(250);
 assert.equal(await page.locator("#status").getAttribute("data-state"),"stopped");
 assert(await page.evaluate(()=>document.querySelector("video").srcObject===null));
 assert.equal(await page.evaluate(()=>window.__forbiddenCalls),0);
 assert.deepEqual(errors,[]);assert.deepEqual(outside,[]);
 await context.close();
 console.log("PASS real Chromium: WASM Noise + WebCrypto mutual proof/replay, exact SDP, actual connected DTLS certificate stats, decoded 320x180 synthetic video, consent/cancel/cleanup");
}finally{
 await browser?.close();
 await new Promise(resolve=>server.close(resolve));
}
