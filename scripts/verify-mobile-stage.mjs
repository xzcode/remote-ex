// Offline artifact inspection only. Do not serve, deploy, start or alter files.
// Validates exact staged tree/config/headers and reports a digest for review.
import fs from "node:fs/promises";
import path from "node:path";
import {createHash} from "node:crypto";
import {fileURLToPath} from "node:url";
import {config,policyFor,names} from "./stage-mobile-client.mjs";
const MAX=16*1024*1024;
function fail(msg){throw Error(msg);}
function same(a,b){return JSON.stringify(a)===JSON.stringify(b);}
async function boundedFile(root,relative,limit=MAX){
    const p=path.join(root,relative),st=await fs.lstat(p);
    if(!st.isFile()||st.isSymbolicLink()||st.size<1||st.size>limit)fail("Invalid staged regular file: "+relative);
    return fs.readFile(p);
}
export async function verify(root){
    const st=await fs.lstat(root);if(!st.isDirectory()||st.isSymbolicLink())fail("Expected a real staged directory");
    const expected=[...names,"index.html","headers.json","view-config.json"].sort();
    const actual=[];
    async function scan(dir,prefix=""){
        for(const item of await fs.readdir(dir,{withFileTypes:true})){
            const relative=prefix+item.name;
            if(item.isSymbolicLink())fail("Symlink refused: "+relative);
            if(item.isDirectory()){
                if(relative!=="pkg")fail("Unexpected staged directory: "+relative);
                await scan(path.join(dir,item.name),relative+"/");
            }else if(item.isFile())actual.push(relative);
            else fail("Unexpected staged entry: "+relative);
        }
    }
    await scan(root);
    if(!same(actual.sort(),expected))fail("Staged file list does not match reviewed allowlist");
    const cfg=JSON.parse(await boundedFile(root,"view-config.json",8192)),hdr=JSON.parse(await boundedFile(root,"headers.json",8192));
    const canonical=cfg?.mode==="disabled"?config(null):config({endpoint:cfg?.endpoint,ice:cfg?.ice,control:cfg?.control,extended:cfg?.extended});
    if(!same(cfg,canonical))fail("Staged config is noncanonical or has unexpected fields");
    if(!same(hdr,policyFor(canonical)))fail("Staged HTTP security headers do not match canonical policy");
    const index=await boundedFile(root,"index.html"),landing=await boundedFile(root,"mobile.html");
    if(!index.equals(landing))fail("Landing page differs from reviewed mobile landing");
    const wasm=await boundedFile(root,"pkg/noise_bg.wasm");
    if(wasm.length<8||!wasm.subarray(0,8).equals(Buffer.from([0,97,115,109,1,0,0,0])))fail("Invalid WASM header");
    const digest=createHash("sha256");let bytes=0;
    for(const name of expected){const b=await boundedFile(root,name);bytes+=b.length;digest.update(name+"\0"+createHash("sha256").update(b).digest("hex")+"\n");}
    return {schema:1,valid:true,mode:canonical.mode,control:canonical.control===true,extended:canonical.extended===true,file_count:expected.length,total_bytes:bytes,tree_sha256:digest.digest("hex"),network_started:false};
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
    if(process.argv.length!==3)fail("Usage: node scripts/verify-mobile-stage.mjs STAGED_DIRECTORY");
    console.log(JSON.stringify(await verify(path.resolve(process.argv[2])),null,2));
}
