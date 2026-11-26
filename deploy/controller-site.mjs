// Dedicated HTTPS static controller site. Not the development fixture or a
// signaling proxy. No credentials in assets, uploads, APIs or remote commands.
import https from "node:https";
import tls from "node:tls";
import fs from "node:fs/promises";
import path from "node:path";
import {createHash,X509Certificate,createPrivateKey} from "node:crypto";
import {isIP} from "node:net";
import {fileURLToPath} from "node:url";
import {verify} from "../scripts/verify-mobile-stage.mjs";
import {names,config,policyFor} from "../scripts/stage-mobile-client.mjs";

const MAX_ASSET=16*1024*1024, MAX_TOTAL=32*1024*1024;
const files=[...names,"index.html","view-config.json","headers.json"].sort();
const served=files.filter(n=>n!=="headers.json");
function require(ok,message){if(!ok)throw Error(message);}
function exactKeys(v,keys){return v&&typeof v==="object"&&!Array.isArray(v)&&Object.keys(v).sort().join(",")===keys.slice().sort().join(",");}
export function siteSettings(env){
    const bind=env.REMOTE_EX_SITE_BIND??"127.0.0.1";
    require(isIP(bind)===4,"Site bind must be an IPv4 address");
    const portText=env.REMOTE_EX_SITE_PORT??"8443";
    require(/^[1-9][0-9]{0,4}$/.test(portText)&&Number(portText)<=65535,"Invalid site port");
    const port=Number(portText);
    const flag=env.REMOTE_EX_SITE_PUBLIC_BIND??"0";
    require(flag==="0"||flag==="1","REMOTE_EX_SITE_PUBLIC_BIND must be 0 or 1");
    const local=bind.startsWith("127.");
    require(local||flag==="1","Non-loopback site bind requires explicit public opt-in");
    const host=env.REMOTE_EX_SITE_HOST;
    require(typeof host==="string"&&host.length<=253&&/^[a-z0-9.-]+$/.test(host)&&
        host.split(".").every(v=>/^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/.test(v))&&
        !isIP(host)&&(host.includes(".")||local&&host==="localhost"),"Set an exact lowercase site DNS hostname");
    require(local||host!=="localhost"&& !host.endsWith(".localhost"),"Public site requires a reviewed DNS hostname");
    const cert=env.REMOTE_EX_SITE_TLS_CERT,key=env.REMOTE_EX_SITE_TLS_KEY;
    require(typeof cert==="string"&&path.isAbsolute(cert)&&typeof key==="string"&&path.isAbsolute(key),"Set absolute TLS certificate and key paths");
    const authority=host+(port===443?"":":"+port);
    return {bind,port,host,authority,cert,key,publicOptIn:flag==="1",local};
}
async function readRegular(root,name,max=MAX_ASSET){
    const file=path.join(root,name),stat=await fs.lstat(file);
    require(stat.isFile()&&!stat.isSymbolicLink()&&stat.size>0&&stat.size<=max,"Invalid controller asset: "+name);
    const bytes=await fs.readFile(file);
    require(bytes.length===stat.size,"Controller asset changed while loading: "+name);
    return bytes;
}
function type(name){
    if(name.endsWith(".html"))return "text/html; charset=utf-8";
    if(name.endsWith(".js"))return "text/javascript; charset=utf-8";
    if(name.endsWith(".css"))return "text/css; charset=utf-8";
    if(name.endsWith(".wasm"))return "application/wasm";
    return "application/json; charset=utf-8";
}
function canonicalConfig(v){
    if(v?.mode==="disabled"){
        require(exactKeys(v,["schema","mode"])&&v.schema===1,"Invalid offline site config");
        return config(null);
    }
    require(exactKeys(v,["schema","mode","endpoint","ice","control","extended"])&&v.schema===1&&v.mode==="direct-read-only","Invalid controller site config");
    return config({endpoint:v.endpoint,ice:v.ice,control:v.control,extended:v.extended});
}
export async function snapshot(root,settings){
    const verified=await verify(root);
    const pkg=await fs.lstat(path.join(root,"pkg"));
    require(pkg.isDirectory()&&!pkg.isSymbolicLink(),"Invalid pkg directory");
    const items=new Map(),digest=createHash("sha256");
    let total=0;
    for(const name of files){
        const data=await readRegular(root,name);
        total+=data.length;require(total<=MAX_TOTAL,"Controller asset tree exceeds memory bound");
        digest.update(name+"\0"+createHash("sha256").update(data).digest("hex")+"\n");
        items.set(name,data);
    }
    require(digest.digest("hex")===verified.tree_sha256,"Controller assets changed after verification");
    const v=canonicalConfig(JSON.parse(items.get("view-config.json")));
    require(JSON.stringify(v)===JSON.stringify(JSON.parse(items.get("view-config.json"))),"Noncanonical site config");
    const headers=policyFor(v);
    require(JSON.stringify(headers)===JSON.stringify(JSON.parse(items.get("headers.json"))),"Controller headers changed after verification");
    // A fixed, reviewed WSS endpoint is required for configured sites. The
    // operator separately enables its exact HTTPS origin in rendezvous.
    require(!settings.publicOptIn||v.mode!=="disabled","Public site requires explicit operator connection configuration");
    const table=new Map(served.map(n=>["/"+n,{body:items.get(n),type:type(n)}]));
    table.set("/",table.get("/index.html"));
    return {table,headers:{...headers,"Strict-Transport-Security":"max-age=86400"},verified};
}
export async function tlsMaterial(settings){
    async function boundedRead(p){
        const stat=await fs.stat(p);
        require(stat.isFile()&&stat.size>0&&stat.size<=256*1024,"Invalid TLS file");
        return fs.readFile(p);
    }
    const cert=await boundedRead(settings.cert),key=await boundedRead(settings.key);
    const leaf=new X509Certificate(cert);
    require(leaf.checkHost(settings.host,{subject:"default"})!==undefined,"Certificate does not cover configured site hostname");
    const now=Date.now();
    require(Date.parse(leaf.validFrom)<=now&&now<Date.parse(leaf.validTo),"Site certificate outside validity period");
    require(leaf.checkPrivateKey(createPrivateKey(key)),"TLS private key does not match certificate");
    const material={cert,key,minVersion:"TLSv1.2"};
    tls.createSecureContext(material); // same TLS configuration as listen
    return material;
}
export async function prepare(root,env){
    const settings=siteSettings(env),assets=await snapshot(root,settings),material=await tlsMaterial(settings);
    return {settings,assets,material};
}
export function report(prepared){
    const {settings,assets}=prepared;
    return {...assets.verified,site_schema:1,tls:true,public_opt_in:settings.publicOptIn,network_started:false};
}
export async function listen(prepared){
    const {settings,assets,material}=prepared;
    const server=https.createServer({...material,maxHeaderSize:8192,handshakeTimeout:5000},(req,res)=>{
        const acceptedHost=req.headers.host===settings.authority;
        const item=assets.table.get(req.url);
        if(!acceptedHost||!["GET","HEAD"].includes(req.method)||!item){
            res.writeHead(404,{...assets.headers,"Content-Type":"text/plain; charset=utf-8","Content-Length":9});
            res.end(req.method==="HEAD"?undefined:"Not found");return;
        }
        res.writeHead(200,{...assets.headers,"Content-Type":item.type,"Content-Length":item.body.length});
        res.end(req.method==="HEAD"?undefined:item.body);
    });
    server.maxConnections=64;server.requestTimeout=5000;server.headersTimeout=5000;server.keepAliveTimeout=1000;server.timeout=8000;
    server.on("clientError",(_error,socket)=>socket.destroy());
    await new Promise((resolve,reject)=>{server.once("error",reject);server.listen(settings.port,settings.bind,resolve);});
    return server;
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
    try{
        const [mode,root,...extra]=process.argv.slice(2);
        require((mode==="--check-config"||mode==="--serve")&&root&&!extra.length,"Usage: node deploy/controller-site.mjs --check-config|--serve STAGED_DIRECTORY");
        const prepared=await prepare(path.resolve(root),process.env);
        if(mode==="--check-config")console.log(JSON.stringify(report(prepared),null,2));
        else{
            const server=await listen(prepared);
            console.log("Remote EX reviewed HTTPS controller site listening (static assets only)");
            const stop=()=>{const timer=setTimeout(()=>{server.closeAllConnections();process.exit(0);},5000);timer.unref();server.close(()=>process.exit(0));};
            for(const sig of ["SIGINT","SIGTERM"])process.once(sig,stop);
        }
    }catch(e){
        // Do not emit PEM values, URLs, path names or raw parser errors.
        const msg=String(e?.message??"");
        console.error("Controller site configuration or startup failed"+(/^[A-Za-z0-9 ._:;-]{1,140}$/.test(msg)?": "+msg:""));
        process.exitCode=1;
    }
}
