// Offline, operator-side consistency check. Reuse the actual rendezvous
// executable's existing no-bind preflight and protocol report. Do not start
// services, connect to a host, read TURN secrets here, or alter configuration.
import fs from "node:fs/promises";
import path from "node:path";
import {isIP} from "node:net";
import {X509Certificate} from "node:crypto";
import {execFile} from "node:child_process";
import {promisify} from "node:util";
import {fileURLToPath} from "node:url";
import {snapshot} from "../deploy/controller-site.mjs";
import {exactOrigin} from "./verify-controller-site.mjs";
import {viewEndpoint} from "../apps/web/serve.mjs";
import {canonicalIce,normalizePolicy} from "../apps/web/ice-policy.js";

const run=promisify(execFile);
const ANDROID_ORIGIN="https://appassets.androidplatform.net";
class ReviewError extends Error{}
function need(value,message){if(!value)throw new ReviewError(message);}
function exact(v,keys){return v&&typeof v==="object"&&!Array.isArray(v)&&Object.keys(v).sort().join(",")===keys.slice().sort().join(",");}
function hex32(v){return typeof v==="string"&&/^[0-9a-f]{64}$/.test(v)&&v!=="0".repeat(64);}
function opt(env,key){
    const v=env[key];
    need(v===undefined||typeof v==="string"&&v.length>0&&v.length<=4096&&!/[\x00-\x1f\x7f]/.test(v),"Invalid "+key);
    return v;
}
async function regular(file,max,label){
    need(typeof file==="string"&&path.isAbsolute(file),"Absolute "+label+" path required");
    let s;
    try{s=await fs.stat(file);}catch{throw new ReviewError("Cannot inspect "+label);}
    need(s.isFile()&&s.size>0&&s.size<=max,"Invalid "+label+" file size/type");
    const b=await fs.readFile(file);
    need(b.length===s.size&&b.length<=max,label+" changed during read");
    return b;
}
export function plan(value){
    need(exact(value,["schema","tree_sha256","site_origin","signaling_endpoint","server_id","android"])&&value.schema===1&&
        hex32(value.tree_sha256)&&hex32(value.server_id)&&typeof value.android==="boolean","Invalid reviewed deployment plan");
    const origin=exactOrigin(value.site_origin),endpoint=viewEndpoint(value.signaling_endpoint);
    need(origin.hostname!=="localhost"&&!origin.hostname.endsWith(".localhost"),"Reviewed deployment requires an explicit site DNS origin");
    return {...value,site_origin:origin.origin,signaling_endpoint:endpoint.href};
}
function origins(env){
    const one=opt(env,"REMOTE_EX_WEB_ORIGIN"),many=opt(env,"REMOTE_EX_WEB_ORIGINS");
    need(Boolean(one)!==Boolean(many),"Set exactly one reviewed Web origin configuration");
    const list=one?[one]:many.split(",");
    need(list.length>=1&&list.length<=8&&new Set(list).size===list.length,"Invalid or duplicate Web origins");
    // Match the public HTTPS origin policy, not the HTTP CI-only exception.
    for(const value of list){
        let u;try{u=new URL(value);}catch{throw new ReviewError("Invalid configured Web origin");}
        need(value.length<=512&&u.protocol==="https:"&&u.origin===value&&!u.username&&!u.password&&!u.search&&!u.hash&&u.pathname==="/",
            "Configured Web origin is not exact HTTPS");
    }
    return list;
}
export async function relayPolicy(env){
    const rawStun=opt(env,"REMOTE_EX_STUN_URL"),single=opt(env,"REMOTE_EX_TURN_URL"),secret=opt(env,"REMOTE_EX_TURN_SECRET_FILE"),
        pool=opt(env,"REMOTE_EX_RELAY_POOL_FILE"),region=opt(env,"REMOTE_EX_RELAY_REGION");
    const stun=rawStun?canonicalIce(rawStun,["stun"]):null;
    need(!(pool&&(single||secret))&&Boolean(single)===Boolean(secret)&&(!region||pool),"Conflicting TURN configuration");
    if(!pool)return {stun,eligible:single?[canonicalIce(single,["turn","turns"])]:[],count:single?1:0};
    let data;
    try{data=JSON.parse(await regular(pool,65536,"relay pool"));}catch{throw new ReviewError("Invalid relay pool configuration");}
    need(exact(data,["schema","relays"])&&data.schema===1&&Array.isArray(data.relays)&&data.relays.length>=1&&data.relays.length<=8,"Invalid relay pool shape");
    const ids=new Set(),urls=new Set(),token=(s,n)=>typeof s==="string"&&s.length<=n&&/^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?$/.test(s);
    const rows=data.relays.map(r=>{
        need(exact(r,["id","region","priority","url","secret_file"])&&token(r.id,32)&&token(r.region,24)&&
            Number.isInteger(r.priority)&&r.priority>=1&&r.priority<=1000&&typeof r.secret_file==="string"&&path.isAbsolute(r.secret_file)&&r.secret_file.length<=2048,"Invalid relay pool entry");
        const uri=canonicalIce(r.url,["turn","turns"]);
        need(!ids.has(r.id)&&!urls.has(uri),"Duplicate relay pool entry");
        ids.add(r.id);urls.add(uri);
        return {region:r.region,priority:r.priority,uri};
    });
    const candidates=rows.filter(r=>region===undefined||r.region===region);
    need(candidates.length>0,"No eligible relay in selected region");
    const priority=Math.min(...candidates.map(r=>r.priority));
    return {stun,eligible:candidates.filter(r=>r.priority===priority).map(r=>r.uri).sort(),count:rows.length};
}
export function checkProtocol(report,client){
    need(report?.schema===1&&report.signaling_version===1&&report.input_version===1&&
        report.noise_pattern==="Noise_NN_25519_ChaChaPoly_BLAKE2s"&&report.max_sealed_clear_bytes===32768&&
        report.max_input_bytes===1024&&report.max_sdp_bytes===49152&&report.max_message_bytes===98304&&
        report.requires_verified_peer_keys===true&&report.requires_local_consent===true&&report.network_started===false&&
        Array.isArray(report.profiles)&&report.profiles.length<=64,"Incompatible rendezvous protocol report");
    const required=[1,...(client.control?[2]:[]),...(client.extended?[22,...(client.control?[23]:[])]:[])];
    if(client.extended)need(report.extended_route_seconds===420,"Incompatible extended route lease");
    for(const code of required){
        const rows=report.profiles.filter(p=>p?.code===code),input=code===2||code===23,grant=input?3:1;
        need(rows.length===1,"Missing or duplicate controller profile");
        const p=rows[0];
        need(p.session_owner_grant===grant&&Array.isArray(p.binding_accepts)&&p.binding_accepts.length===1&&p.binding_accepts[0]===grant&&
            p.real_screen===true&&p.real_input===input&&p.system_audio===false&&p.fixture_only===false&&p.extended===(code>=22),
            "Incompatible signed controller profile");
    }
    return required;
}
async function localReport(binary,arg,env){
    need(path.isAbsolute(binary),"Absolute reviewed rendezvous executable required");
    try{
        const info=await fs.stat(binary);
        need(info.isFile(),"Reviewed rendezvous executable must be a regular file");
        const {stdout}=await run(binary,[arg],{env,encoding:"utf8",timeout:12000,maxBuffer:65536,windowsHide:true});
        return JSON.parse(stdout);
    }catch{throw new ReviewError("Rendezvous rejected "+arg+" or did not return a bounded JSON report");}
}
async function checkCertificate(env,endpoint){
    const cert=await regular(opt(env,"REMOTE_EX_TLS_CERT"),256*1024,"signaling certificate");
    let leaf;
    try{leaf=new X509Certificate(cert);}catch{throw new ReviewError("Cannot parse signaling certificate");}
    const host=new URL(endpoint).hostname.replace(/^\[|\]$/g,""),now=Date.now();
    need(Date.parse(leaf.validFrom)<=now&&now<Date.parse(leaf.validTo),"Signaling certificate is outside its validity period");
    need(isIP(host)?leaf.checkIP(host):leaf.checkHost(host),"Signaling certificate does not cover reviewed WSS hostname");
    // Native --check-config separately validates loading/matching its key.
    // Never read or print that key here.
}
export async function verifyDeployment(root,review,binary,env=process.env){
    const approved=plan(review);
    const site=await snapshot(root,{publicOptIn:false});
    need(site.verified.tree_sha256===approved.tree_sha256,"Reviewed controller tree digest mismatch");
    const client=JSON.parse(site.table.get("/view-config.json").body);
    need(client.schema===1&&client.mode==="direct-read-only"&&client.endpoint===approved.signaling_endpoint,"Reviewed WSS endpoint differs from staged controller");
    need(opt(env,"REMOTE_EX_SERVER_ID")===approved.server_id,"Reviewed server ID differs from rendezvous configuration");
    const allowed=origins(env);
    need(allowed.includes(approved.site_origin),"Controller site origin is not allowed by rendezvous");
    if(approved.android)need(allowed.includes(ANDROID_ORIGIN),"Android bundled origin is not allowed by rendezvous");
    const ice=normalizePolicy(client.ice),relay=await relayPolicy(env);
    need(ice.stun===relay.stun,"Staged STUN policy differs from rendezvous");
    need(JSON.stringify([...ice.turn].sort())===JSON.stringify(relay.eligible),"Staged TURN allowlist differs from all eligible rendezvous relays");
    need(!ice.relayOnly||relay.eligible.length>0,"Forced relay requires eligible TURN");
    await checkCertificate(env,approved.signaling_endpoint);
    const actual=await localReport(binary,"--check-config",env);
    need(actual?.schema===1&&actual.valid===true&&actual.mode==="wss"&&actual.network_started===false&&
        actual.enrollment_required===true&&Number.isInteger(actual.enrolled_count)&&actual.enrolled_count>=2&&actual.enrolled_count<=4096&&
        actual.web_origin_configured===true&&actual.web_origin_count===allowed.length&&actual.stun_configured===(relay.stun!==null)&&
        actual.turn_configured===(relay.count>0)&&actual.relay_count===relay.count&&actual.eligible_relay_count===relay.eligible.length,
        "Rendezvous preflight does not match the reviewed controller policy or lacks two enrolled devices");
    const required=checkProtocol(await localReport(binary,"--protocol-info",env),client);
    return {schema:1,valid:true,tree_sha256:approved.tree_sha256,server_id_matched:true,origin_matched:true,android_origin_checked:approved.android,
        origin_count:allowed.length,enrolled_count:actual.enrolled_count,relay_count:relay.count,eligible_relay_count:relay.eligible.length,
        forced_relay:ice.relayOnly,control:client.control,extended:client.extended,required_profiles:required,protocol_version:1,
        certificate_name_and_validity_checked:true,network_started:false,service_changed:false};
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
    try{
        need(process.argv.length===5,"Usage: node scripts/verify-controller-deployment.mjs STAGED_DIRECTORY REVIEWED_PLAN_JSON ABSOLUTE_RENDEZVOUS_EXECUTABLE");
        const review=JSON.parse(await regular(path.resolve(process.argv[3]),8192,"deployment plan"));
        console.log(JSON.stringify(await verifyDeployment(path.resolve(process.argv[2]),review,process.argv[4]),null,2));
    }catch(e){
        console.error(e instanceof ReviewError?e.message:"Offline deployment consistency check failed");
        process.exitCode=1;
    }
}
