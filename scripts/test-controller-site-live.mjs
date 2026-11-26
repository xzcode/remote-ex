// Isolated loopback real HTTPS, synthetic assets, disposable certificate.
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import net from "node:net";
import https from "node:https";
import {execFileSync} from "node:child_process";
import {names,config,policyFor} from "./stage-mobile-client.mjs";
import {prepare,listen} from "../deploy/controller-site.mjs";
import {verifyLiveSite,exactOrigin} from "./verify-controller-site.mjs";
const root=await fs.mkdtemp(path.join(os.tmpdir(),"remote-ex-site-live-"));
let site,mutant,reservation;
async function port(){
    reservation=net.createServer();
    await new Promise((resolve,reject)=>{reservation.once("error",reject);reservation.listen(0,"127.0.0.1",resolve);});
    const n=reservation.address().port;await new Promise(r=>reservation.close(r));reservation=null;return n;
}
try{
    const dir=path.join(root,"assets");await fs.mkdir(dir);await fs.mkdir(path.join(dir,"pkg"));
    const landing=Buffer.from("<!doctype html><title>Inert site verification fixture</title>\n");
    for(const name of names)await fs.writeFile(path.join(dir,name),name==="pkg/noise_bg.wasm"?Buffer.from([0,97,115,109,1,0,0,0]):name==="mobile.html"?landing:"fixture\n");
    await fs.writeFile(path.join(dir,"index.html"),landing);
    const cfg=config(null);
    await fs.writeFile(path.join(dir,"view-config.json"),JSON.stringify(cfg));
    await fs.writeFile(path.join(dir,"headers.json"),JSON.stringify(policyFor(cfg)));
    const cert=path.join(root,"cert.pem"),key=path.join(root,"key.pem");
    execFileSync("openssl",["req","-x509","-newkey","rsa:2048","-nodes","-days","1","-subj","/CN=localhost","-addext","subjectAltName=DNS:localhost","-keyout",key,"-out",cert],{stdio:"ignore",timeout:10000});
    const ca=await fs.readFile(cert);
    const p=await port(),env={REMOTE_EX_SITE_HOST:"localhost",REMOTE_EX_SITE_PORT:String(p),REMOTE_EX_SITE_TLS_CERT:cert,REMOTE_EX_SITE_TLS_KEY:key};
    const prepared=await prepare(dir,env),sha=prepared.assets.verified.tree_sha256;
    site=await listen(prepared);const origin="https://localhost:"+p;
    const report=await verifyLiveSite(dir,origin,sha,{ca});
    assert(report.valid&&report.exact_security_headers&&report.denied_routes===9&&report.signaling_connected===false&&report.service_changed===false);
    assert.equal(report.asset_routes,prepared.assets.table.size);
    assert.equal(report.requests,report.asset_routes+2+report.denied_routes);
    assert(!JSON.stringify(report).includes(origin));
    await assert.rejects(verifyLiveSite(dir,origin,"0".repeat(64),{ca}),/independently reviewed digest/);
    await assert.rejects(verifyLiveSite(dir,origin,sha,{ca:[]}),/HTTPS trust/);
    for(const bad of ["http://localhost","https://localhost/","https://user@localhost","https://localhost/a","https://localhost?x=1","https://127.0.0.1","https://LOCALHOST"])assert.throws(()=>exactOrigin(bad));
    let fault="body",requests=0;
    const mp=await port();
    mutant=https.createServer(prepared.material,(req,res)=>{
        requests++;
        const item=prepared.assets.table.get(req.url);
        let headers={...prepared.assets.headers},status=200;
        let body=item?.body??Buffer.from("Not found");
        if(!item)status=404;
        headers["Content-Type"]=item?.type??"text/plain; charset=utf-8";
        if(fault==="body"&&req.url==="/mobile.html"){body=Buffer.from(body);body[0]^=1;}
        if(fault==="csp"&&req.url==="/")headers["Content-Security-Policy"]="default-src *";
        if(fault==="duplicate"&&req.url==="/")headers["Content-Security-Policy"]=[headers["Content-Security-Policy"],headers["Content-Security-Policy"]];
        if(fault==="mime"&&req.url==="/pkg/noise_bg.wasm")headers["Content-Type"]="text/plain";
        if(fault==="redirect"&&req.url==="/"){status=302;headers.Location="https://example.invalid/";}
        if(fault==="extra"&&req.url==="/headers.json"){status=200;body=Buffer.from("unexpected");}
        if(fault==="cookie"&&req.url==="/")headers["Set-Cookie"]="fixture=unexpected";
        if(fault==="encoding"&&req.url==="/")headers["Content-Encoding"]="gzip";
        headers["Content-Length"]=body.length;
        res.writeHead(status,headers);res.end(req.method==="HEAD"?undefined:body);
    });
    await new Promise((resolve,reject)=>{mutant.once("error",reject);mutant.listen(mp,"127.0.0.1",resolve);});
    const m="https://localhost:"+mp;
    for(const [mode,error] of [["body",/Asset digest mismatch/],["csp",/Content-Security-Policy/],["duplicate",/Content-Security-Policy/],
        ["mime",/Content-Type/],["redirect",/HTTP 200/],["extra",/not denied/],["cookie",/Set-Cookie/],["encoding",/Compressed/]]){
        fault=mode;requests=0;
        await assert.rejects(verifyLiveSite(dir,m,sha,{ca}),error,mode);
        assert(requests>0&&requests<=report.requests,"Unbounded requests or unexpected redirect");
    }
    console.log("PASS live HTTPS verification: actual trusted TLS, all fixed assets and MIME/digests, exact security headers, denied paths, redirect/cookie/compression/duplicate header refusal");
}finally{
    if(reservation)await new Promise(r=>reservation.close(r));
    for(const s of [site,mutant])if(s){s.closeAllConnections();await new Promise(r=>s.close(r));}
    await fs.rm(root,{recursive:true,force:true});
}
