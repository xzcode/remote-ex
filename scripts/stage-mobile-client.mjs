// Stage the fixed client assets for a private Android origin or HTTPS site.
// No diagnostic fixture, server binary, private key, or ephemeral credential.
import fs from "node:fs/promises";
import path from "node:path";
import {fileURLToPath} from "node:url";
import {headers,viewEndpoint} from "../apps/web/serve.mjs";
import {normalizePolicy} from "../apps/web/ice-policy.js";
const root=fileURLToPath(new URL("../apps/web/",import.meta.url));
export const names=["mobile.html","mobile.js","identity.html","identity.js","identity-page.js","view.html","view-page.js","view-session.js","crypto.js","ice-policy.js","input.js","touch.js","session-policy.js","connection-book.js","wasm-loader.js","capabilities.js","controller-ui.js","style.css","pkg/noise.js","pkg/noise_bg.wasm"];
export function config(raw){
    if(raw===null)return {schema:1,mode:"disabled"};
    if(!raw||typeof raw!=="object"||Array.isArray(raw)||!["control,endpoint,ice","control,endpoint,extended,ice"].includes(Object.keys(raw).sort().join(","))||typeof raw.control!=="boolean"||(raw.extended!==undefined&&typeof raw.extended!=="boolean"))throw Error("Invalid mobile operator config");
    return {schema:1,mode:"direct-read-only",endpoint:viewEndpoint(raw.endpoint).href,ice:normalizePolicy(raw.ice),control:raw.control,extended:raw.extended===true};
}
export function policyFor(v){
    const result={...headers};
    if(v.mode!=="disabled")result["Content-Security-Policy"]=headers["Content-Security-Policy"].replace("connect-src 'self'","connect-src 'self' "+new URL(v.endpoint).origin);
    return result;
}
export async function stage(out,raw){
    const v=config(raw),policy=policyFor(v),assets=new Map();
    for(const name of names){
        let data=await fs.readFile(path.join(root,name));
        if(data.length===0||data.length>16*1024*1024)throw Error("Invalid staged asset size: "+name);
        if(name==="identity.html"||name==="view.html"){
            data=Buffer.from(data.toString("utf8").replace(/href="\/">Back to (?:the local diagnostic lab|the synthetic compatibility lab)/,'href="/mobile.html">Back to mobile controller'));
        }
        assets.set(name,data);
    }
    assets.set("view-config.json",Buffer.from(JSON.stringify(v)+"\n"));
    assets.set("headers.json",Buffer.from(JSON.stringify(policy,null,2)+"\n"));
    assets.set("index.html",assets.get("mobile.html"));
    await fs.mkdir(out,{recursive:false});await fs.mkdir(path.join(out,"pkg"));
    for(const [name,data] of assets)await fs.writeFile(path.join(out,name),data,{flag:"wx"});
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
    const [out,configPath,...extra]=process.argv.slice(2);
    if(!out||extra.length)throw Error("Usage: node scripts/stage-mobile-client.mjs NEW_DIRECTORY [operator-config.json]");
    let raw=null;
    if(configPath){const data=await fs.readFile(configPath);if(data.length>8192)throw Error("Operator config exceeds limit");raw=JSON.parse(data);}
    await stage(path.resolve(out),raw);console.log("Staged fixed mobile controller assets; no service started.");
}
