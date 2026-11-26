// Product UI regression with the real Chromium and current controller
// code. No network session, actual device, capture, or production records.
import assert from "node:assert/strict";
import {chromium} from "playwright";
import {serve} from "./serve.mjs";
const endpoint="wss://127.0.0.1:9443/v1/ws";
const {server,url}=await serve(0,null,endpoint,{stun:null,turn:[],relayOnly:false,control:true,extended:true});
let browser;
try{
    browser=await chromium.launch({headless:true,args:["--disable-background-networking"]});
    const context=await browser.newContext({permissions:[],hasTouch:true,isMobile:true,viewport:{width:390,height:844}});
    const page=await context.newPage(),sockets=[],outside=[];
    await context.route("**/*",r=>{if(new URL(r.request().url()).origin!==url){outside.push("unexpected");return r.abort();}return r.continue();});
    page.on("websocket",()=>sockets.push("unexpected"));
    await page.goto(url+"/mobile.html");
    await page.waitForFunction(()=>!document.querySelector("#mobile-connect").hidden,null,{timeout:45000});
    assert(await page.locator("#home-connect").isVisible());
    assert(await page.locator("#home-devices").isHidden());
    assert(await page.locator("#mobile-recent").getByText("No saved devices.").isVisible());
    const fits=()=>page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1);
    assert(await fits());
    await page.locator('.mobile-nav [data-home-target="home-settings"]').click();
    assert(await page.locator("#home-settings").isVisible());
    await page.locator("#mobile-theme").selectOption("gray");
    assert.equal(await page.locator("html").getAttribute("data-theme"),"gray");
    await page.goto(url+"/identity.html");
    assert.equal(await page.locator("html").getAttribute("data-theme"),"gray");
    await page.locator("#identity-create").click();
    await page.waitForFunction(()=>document.querySelector("#identity-status").dataset.state==="verified");
    await page.goto(url+"/mobile.html");
    await page.waitForFunction(()=>!document.querySelector("#mobile-connect").hidden,null,{timeout:45000});
    await page.evaluate(async endpoint=>{
        const book=await import("/connection-book.js");
        localStorage.setItem(book.BOOK_KEY,book.writeBook([{endpoint,serverId:"01".repeat(32),hostKey:"02".repeat(32),label:"CI-only saved device"}]));
    },endpoint);
    await page.reload();
    await page.waitForFunction(()=>!document.querySelector("#mobile-connect").hidden&&document.querySelector("#mobile-recent").textContent.includes("CI-only"),null,{timeout:45000});
    await page.locator('.mobile-nav [data-home-target="home-devices"]').click();
    await page.locator("#mobile-device-search").fill("no match");
    assert(await page.locator("#mobile-devices").getByText("No saved devices.").isVisible());
    await page.locator("#mobile-device-search").fill("CI-only");
    await page.locator("#mobile-devices .saved-row").click();
    await page.waitForURL(url+"/view.html");
    await page.waitForFunction(()=>document.querySelector("#view-status").dataset.state==="idle"&&!document.querySelector("#view-recheck").disabled,null,{timeout:45000});
    assert.equal(await page.locator("#view-server").inputValue(),"01".repeat(32));
    assert.equal(await page.locator("#view-host").inputValue(),"02".repeat(32));
    for(const id of ["view-control","view-extended","view-consent"])assert(!(await page.locator("#"+id).isChecked()));
    assert(await page.locator("#view-connect").isDisabled());
    assert(await fits());
    await page.setViewportSize({width:844,height:390});
    assert(await fits());
    assert.deepEqual(sockets,[]);assert.deepEqual(outside,[]);
    console.log("PASS M7 mobile navigation, real empty/saved book, appearance persistence, one-shot review without consent or socket, portrait/landscape width");
}finally{await browser?.close();await new Promise(r=>server.close(r));}
