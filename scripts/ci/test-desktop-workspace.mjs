// Render the actual compiled Vue frontend in a disposable browser.
// No native Tauri bridge, mock online devices, real connection records,
// server registration, or desktop access. This is layout/navigation only.
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import http from "node:http";
import {fileURLToPath} from "node:url";
import {createRequire} from "node:module";
const require=createRequire(new URL("../../apps/web/package.json",import.meta.url));
const {chromium}=require("playwright");
const root=fileURLToPath(new URL("../../apps/desktop/dist/",import.meta.url));
const routes=new Map([["/",["index.html","text/html; charset=utf-8"]]]);
for(const item of await fs.readdir(path.join(root,"assets"),{withFileTypes:true})){
    assert(item.isFile()&&!item.isSymbolicLink());
    const type=item.name.endsWith(".js")?"text/javascript":item.name.endsWith(".css")?"text/css":item.name.endsWith(".svg")?"image/svg+xml":null;
    if(type)routes.set("/assets/"+item.name,["assets/"+item.name,type]);
}
const server=http.createServer(async(req,res)=>{
    const item=routes.get(req.url);
    if(req.method!=="GET"||!item){res.writeHead(404);res.end();return;}
    try{const bytes=await fs.readFile(path.join(root,item[0]));res.writeHead(200,{"Content-Type":item[1],"Cache-Control":"no-store"});res.end(bytes);}
    catch{res.writeHead(500);res.end();}
});
await new Promise((resolve,reject)=>{server.once("error",reject);server.listen(0,"127.0.0.1",resolve);});
const url="http://127.0.0.1:"+server.address().port;
let browser;
try{
    const out=process.env.REMOTE_EX_UI_PREVIEW_DIR;
    if(out)await fs.mkdir(out,{recursive:true});
    browser=await chromium.launch({headless:true,args:["--disable-background-networking"]});
    const context=await browser.newContext({locale:"en-US",colorScheme:"light",viewport:{width:1180,height:760}});
    const page=await context.newPage(),errors=[],outside=[],sockets=[];
    page.on("pageerror",e=>errors.push(e.name));page.on("websocket",()=>sockets.push("unexpected"));
    await context.route("**/*",r=>{if(new URL(r.request().url()).origin!==url){outside.push("unexpected");return r.abort();}return r.continue();});
    await page.goto(url);
    const nav=page.getByRole("navigation",{name:"Workspace"});
    const goto=label=>nav.getByRole("button",{name:label,exact:true}).click();
    const fits=()=>page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1&&document.querySelector(".body").scrollWidth<=document.querySelector(".body").clientWidth+1);
    const snap=async name=>{if(out)await page.screenshot({path:path.join(out,name+".png"),animations:"disabled"});};
    await page.getByRole("heading",{name:"Ready to connect"}).waitFor();
    assert(await page.locator("#quick-invite").isVisible());
    assert(await page.getByRole("button",{name:"Review invitation",exact:true}).isDisabled());
    assert(await fits());await snap("desktop-home-light");
    await goto("Trusted devices");assert(await page.getByRole("heading",{name:"Choose a saved server"}).isVisible());assert(await page.locator("#workspace-server-id").isDisabled());
    await goto("Connection history");assert(await page.getByRole("heading",{name:"Connection history",exact:true}).isVisible());
    await goto("File transfer");
    const fileMode=page.locator(".lan-panel:visible select").filter({has:page.locator('option[value="file"]')});
    assert.equal(await fileMode.inputValue(),"file");assert(await fits());await snap("desktop-file-connection");
    assert(await page.locator(".file-mode-bar:visible").isVisible());
    assert(await page.locator(".transfer-panes:visible").getByText("Local device",{exact:true}).isVisible());
    assert(await page.locator(".transfer-panes:visible").getByText("Remote device",{exact:true}).isVisible());
    await page.locator(".file-mode-bar:visible button").nth(3).click();assert.equal(await fileMode.inputValue(),"catalog_batch");
    await page.locator(".file-mode-bar:visible button").first().click();assert.equal(await fileMode.inputValue(),"file");
    await goto("Preferences");await page.locator("#theme").selectOption("gray");
    assert.equal(await page.locator("html").getAttribute("data-theme"),"gray");
    await goto("Remote control");await snap("desktop-home-gray");
    await goto("Preferences");await page.locator("#theme").selectOption("dark");await goto("Remote control");
    await snap("desktop-home-dark");
    await page.setViewportSize({width:620,height:720});assert(await fits());
    await snap("desktop-compact");
    assert.deepEqual(errors,[]);assert.deepEqual(outside,[]);assert.deepEqual(sockets,[]);
    console.log("PASS M7 built desktop workspace: real home/nav, native-only review refusal, scoped device/history views, file preset, theme and compact layout; no native session");
}finally{await browser?.close();await new Promise(r=>server.close(r));}
