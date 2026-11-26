// Only an isolated loopback TLS server and synthetic static assets. No real
// remote host, production certificates, external services or device keys.
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import https from "node:https";
import net from "node:net";
import {execFileSync} from "node:child_process";
import {names,config,policyFor} from "./stage-mobile-client.mjs";
import {prepare,report,listen,siteSettings} from "../deploy/controller-site.mjs";
const dir=await fs.mkdtemp(path.join(os.tmpdir(),"remote-ex-site-"));
let server;
try{
    const root=path.join(dir,"site");await fs.mkdir(root);await fs.mkdir(path.join(root,"pkg"));
    const landing=Buffer.from("<!doctype html><title>Inert test fixture</title>\n");
    for(const n of names)await fs.writeFile(path.join(root,n),n==="pkg/noise_bg.wasm"?Buffer.from([0,97,115,109,1,0,0,0]):n==="mobile.html"?landing:"fixture\n");
    await fs.writeFile(path.join(root,"index.html"),landing);
    const v=config(null);await fs.writeFile(path.join(root,"view-config.json"),JSON.stringify(v));await fs.writeFile(path.join(root,"headers.json"),JSON.stringify(policyFor(v)));
    const cert=path.join(dir,"cert.pem"),key=path.join(dir,"key.pem");
    execFileSync("openssl",["req","-x509","-newkey","rsa:2048","-nodes","-days","1","-subj","/CN=localhost","-addext","subjectAltName=DNS:localhost","-keyout",key,"-out",cert],{stdio:"ignore",timeout:10000});
    const sock=net.createServer();await new Promise((r,j)=>{sock.once("error",j);sock.listen(0,"127.0.0.1",r);});
    const port=sock.address().port;
    const env={REMOTE_EX_SITE_HOST:"localhost",REMOTE_EX_SITE_PORT:String(port),REMOTE_EX_SITE_TLS_CERT:cert,REMOTE_EX_SITE_TLS_KEY:key};
    const p=await prepare(root,env);const info=report(p);
    assert.equal(info.network_started,false);assert.equal(info.tls,true);assert.equal(info.mode,"disabled");assert.equal(info.public_opt_in,false);
    assert(!JSON.stringify(info).includes(cert));
    assert.throws(()=>siteSettings({...env,REMOTE_EX_SITE_BIND:"0.0.0.0"}));
    assert.throws(()=>siteSettings({...env,REMOTE_EX_SITE_HOST:"*.example.invalid"}));
    assert.throws(()=>siteSettings({...env,REMOTE_EX_SITE_HOST:"user@example.invalid"}));
    assert.throws(()=>siteSettings({...env,REMOTE_EX_SITE_PORT:"0"}));
    assert.throws(()=>siteSettings({...env,REMOTE_EX_SITE_PUBLIC_BIND:"yes"}));
    await assert.rejects(prepare(root,{...env,REMOTE_EX_SITE_HOST:"other.example.invalid"}),/Certificate/);
    await assert.rejects(prepare(root,{...env,REMOTE_EX_SITE_PUBLIC_BIND:"1"}),/Public site/);
    await new Promise(r=>sock.close(r));
    server=await listen(p);
    const ca=await fs.readFile(cert);
    const request=(target,method="GET",host="localhost:"+port,trust=ca)=>new Promise((resolve,reject)=>{
        const req=https.request({host:"127.0.0.1",port,servername:"localhost",path:target,method,headers:{Host:host},ca:trust,agent:false,timeout:5000},res=>{
            const chunks=[];res.on("data",c=>chunks.push(c));res.on("end",()=>resolve({code:res.statusCode,headers:res.headers,body:Buffer.concat(chunks)}));
        });
        req.on("error",reject);req.on("timeout",()=>req.destroy(Error("request timeout")));req.end();
    });
    const home=await request("/");assert.equal(home.code,200);assert.deepEqual(home.body,landing);
    for(const [k,value] of Object.entries(policyFor(v)))assert.equal(home.headers[k.toLowerCase()],value,k);
    assert.equal(home.headers["strict-transport-security"],"max-age=86400");
    const wasm=await request("/pkg/noise_bg.wasm");assert.equal(wasm.code,200);assert.equal(wasm.headers["content-type"],"application/wasm");
    assert.equal((await request("/","HEAD")).body.length,0);
    for(const target of ["/headers.json","/pkg/","/../cert.pem","/%2e%2e/key.pem","/view.html?x=1","/wss-fixture.js","/v1/ws","/unknown"]){
        const r=await request(target);assert.equal(r.code,404,target);assert.equal(r.headers["content-security-policy"],home.headers["content-security-policy"]);
    }
    assert.equal((await request("/","POST")).code,404);
    assert.equal((await request("/","GET","other.example.invalid")).code,404);
    await assert.rejects(request("/","GET","localhost:"+port,[]));
    // Active server serves its verified snapshot, not subsequent disk edits.
    await fs.writeFile(path.join(root,"mobile.html"),"changed");
    assert.deepEqual((await request("/mobile.html")).body,landing);
    await assert.rejects(prepare(root,env),/Landing page/);
    console.log("PASS dedicated loopback TLS site: no-bind preflight, verified immutable snapshot, exact host/routes/headers, WASM MIME, no proxy/secret/fixture access, normal certificate trust");
}finally{
    if(server)await new Promise(r=>server.close(r));
    await fs.rm(dir,{recursive:true,force:true});
}
