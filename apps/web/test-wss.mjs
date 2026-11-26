// Real WSS integration using the repository's existing rendezvous executable,
// Chromium, and a new private CA in an isolated disposable browser HOME.
// No public host, no production secrets, no certificate bypass, no OS capture.
import assert from "node:assert/strict";
import {chromium} from "playwright";
import {serve} from "./serve.mjs";
import {spawn,execFileSync} from "node:child_process";
import {mkdtempSync,mkdirSync,writeFileSync,rmSync,chmodSync} from "node:fs";
import {tmpdir} from "node:os";
import path from "node:path";
import net from "node:net";
import {fileURLToPath} from "node:url";
import {randomBytes} from "node:crypto";
import {once} from "node:events";

async function freePort(){
 const server=net.createServer();
 await new Promise((resolve,reject)=>{server.once("error",reject);server.listen(0,"127.0.0.1",resolve);});
 const port=server.address().port;await new Promise(resolve=>server.close(resolve));return port;
}
function command(cmd,args){execFileSync(cmd,args,{stdio:["ignore","ignore","pipe"],timeout:15000});}
function certificates(root){
 const ca=path.join(root,"ca.pem"),caKey=path.join(root,"ca.key"),key=path.join(root,"server.key"),csr=path.join(root,"server.csr"),cert=path.join(root,"server.pem"),ext=path.join(root,"extensions.cnf");
 command("openssl",["req","-x509","-newkey","rsa:2048","-nodes","-sha256","-days","1","-subj","/CN=Remote EX disposable test CA","-addext","basicConstraints=critical,CA:TRUE","-addext","keyUsage=critical,keyCertSign,cRLSign","-keyout",caKey,"-out",ca]);
 command("openssl",["req","-new","-newkey","rsa:2048","-nodes","-subj","/CN=127.0.0.1","-keyout",key,"-out",csr]);
 writeFileSync(ext,"basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=IP:127.0.0.1\n");
 command("openssl",["x509","-req","-in",csr,"-CA",ca,"-CAkey",caKey,"-CAcreateserial","-days","1","-sha256","-extfile",ext,"-out",cert]);
 for(const p of [key,caKey])chmodSync(p,0o600);
 return {ca,key,cert};
}
function installRoot(home,ca){
 // Chromium versions use either of these per-user Linux NSS locations.
 // Both are inside a newly generated temporary directory, never the user's
 // trust database or the runner's global OS trust store.
 for(const dir of [path.join(home,".pki","nssdb"),path.join(home,".local","share","pki","nssdb")]){
    mkdirSync(dir,{recursive:true,mode:0o700});
    command("certutil",["-N","--empty-password","-d","sql:"+dir]);
    command("certutil",["-A","-d","sql:"+dir,"-n","Remote EX disposable test CA","-t","C,,","-i",ca]);
 }
}
async function ready(child){
 let timer;
 await new Promise((resolve,reject)=>{
    let out="";
    const finish=e=>{clearTimeout(timer);child.stdout.off("data",onData);child.off("exit",onExit);child.off("error",onError);if(e)reject(e);else resolve();};
    const onData=b=>{out+=b.toString();if(out.length>8192)finish(Error("Unexpected fixture startup output"));else if(out.includes("Remote EX loopback WSS development signaling:"))finish();};
    const onExit=()=>finish(Error("WSS fixture exited before ready"));
    const onError=()=>finish(Error("Cannot launch WSS fixture"));
    child.stdout.on("data",onData);child.once("exit",onExit);child.once("error",onError);
    timer=setTimeout(()=>finish(Error("WSS fixture startup timed out")),10000);
 });
}
async function stopChild(child){
 if(!child||child.exitCode!==null)return;
 const exited=once(child,"exit");child.kill("SIGTERM");
 let timer;try{await Promise.race([exited,new Promise(resolve=>{timer=setTimeout(resolve,6500);})]);}finally{clearTimeout(timer);}
 if(child.exitCode===null){child.kill("SIGKILL");await exited;}
}
const root=mkdtempSync(path.join(tmpdir(),"remote-ex-web-wss-"));
let browser,fixture,wrong,child;
try{
 const home=path.join(root,"home");mkdirSync(home,{mode:0o700});
 const certs=certificates(root);installRoot(home,certs.ca);
 const port=await freePort(),endpoint=`wss://127.0.0.1:${port}/v1/ws`;
 fixture=await serve(0,endpoint);
 wrong=await serve(0,endpoint);
 browser=await chromium.launch({headless:true,env:{...process.env,HOME:home,XDG_CONFIG_HOME:path.join(home,".config"),XDG_DATA_HOME:path.join(home,".local/share")},args:["--disable-background-networking","--autoplay-policy=no-user-gesture-required"]});
 const context=await browser.newContext({permissions:[],ignoreHTTPSErrors:false});
 const unexpected=[];
 await context.route("**/*",route=>{
    if(![fixture.url,wrong.url].includes(new URL(route.request().url()).origin)){unexpected.push("unexpected HTTP origin");return route.abort();}
    return route.continue();
 });
 await context.addInitScript(()=>{
    window.__forbiddenCalls=0;
    if(navigator.mediaDevices)for(const name of ["getDisplayMedia","getUserMedia"])Object.defineProperty(navigator.mediaDevices,name,{configurable:true,value:async()=>{window.__forbiddenCalls++;throw Error("Forbidden capture");}});
 });
 const page=await context.newPage(),other=await context.newPage();
 for(const tab of [page,other])tab.on("websocket",ws=>{if(ws.url()!==endpoint)unexpected.push("unexpected WebSocket endpoint");});
 const errors=[];page.on("pageerror",e=>errors.push(String(e.message)));
 await page.goto(fixture.url,{waitUntil:"networkidle"});
 const ids=await page.evaluate(async()=>{const lab=await import("/wss-fixture.js");return lab.prepare();});
 assert.equal(ids.length,2);assert.notEqual(ids[0],ids[1]);assert(ids.every(id=>/^[a-f0-9]{64}$/.test(id)&&!/^0+$/.test(id)));
 const allow=path.join(root,"allowed.txt");writeFileSync(allow,ids.join("\n")+"\n",{mode:0o600});
 const serverId=randomBytes(32).toString("hex"),wrongId=(serverId[0]==="0"?"1":"0")+serverId.slice(1);
 const env=Object.fromEntries(Object.entries(process.env).filter(([name])=>!name.startsWith("REMOTE_EX_")));
 Object.assign(env,{REMOTE_EX_SIGNAL_BIND:`127.0.0.1:${port}`,REMOTE_EX_TLS_CERT:certs.cert,REMOTE_EX_TLS_KEY:certs.key,REMOTE_EX_SERVER_ID:serverId,REMOTE_EX_ALLOWED_DEVICES_FILE:allow,REMOTE_EX_WEB_ORIGIN:fixture.url});
 const exe=fileURLToPath(new URL("../../target/debug/remote-ex-rendezvous",import.meta.url));
 const preflight=JSON.parse(execFileSync(exe,["--check-config"],{env,encoding:"utf8",timeout:10000}));
 assert.equal(preflight.mode,"wss");assert.equal(preflight.network_started,false);assert.equal(preflight.enrolled_count,2);assert.equal(preflight.web_origin_configured,true);
 child=spawn(exe,[],{env,stdio:["ignore","pipe","inherit"]});await ready(child);
 const probe=(tab,id)=>tab.evaluate(async({endpoint,id})=>{const lab=await import("/wss-fixture.js");return lab.probe(endpoint,id);},{endpoint,id});
 assert.deepEqual(await probe(page,serverId),{challengeSeen:true,matched:true});
 assert.deepEqual(await probe(page,wrongId),{challengeSeen:true,matched:false});
 await other.goto(wrong.url,{waitUntil:"networkidle"});
 assert.deepEqual(await probe(other,serverId),{challengeSeen:false,matched:false});
 await other.close();
 await page.bringToFront();
 const result=await page.evaluate(async({endpoint,serverId})=>{const lab=await import("/wss-fixture.js");return lab.run(endpoint,serverId);},{endpoint,serverId});
 for(const key of ["wss","registration","route","noise","encryptedSdp","dtls","video","closed"])assert.equal(result[key],true,key);
 assert(result.frames>=3);
 assert.equal(await page.evaluate(()=>window.__forbiddenCalls),0);
 assert.equal(await page.evaluate(()=>document.querySelector("video").srcObject===null),true);
 assert.deepEqual(unexpected,[]);assert.deepEqual(errors,[]);
 await context.close();
 console.log("PASS real Chromium WSS: private CA validation, exact Origin/pin, two enrolled keys, authenticated v1 route, Noise-encrypted offer/answer/proofs, negotiated DTLS, decoded frames and route close");
}finally{
 await browser?.close();
 await stopChild(child);
 for(const s of [fixture,wrong])if(s)await new Promise(resolve=>s.server.close(resolve));
 rmSync(root,{recursive:true,force:true});
}
