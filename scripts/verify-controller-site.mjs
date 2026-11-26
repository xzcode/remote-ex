// Explicit operator-run, read-only HTTPS acceptance. Compare the live site
// against an independently reviewed local tree. Never connect to signaling,
// follow redirects, send credentials, enroll a device, or start a service.
import https from "node:https";
import path from "node:path";
import {isIP} from "node:net";
import {createHash} from "node:crypto";
import {fileURLToPath} from "node:url";
import {snapshot} from "../deploy/controller-site.mjs";

const MAX_ERROR_BODY=64*1024;
const deniedPaths=["/headers.json","/app.js","/v1-fixture.json","/wss-fixture.js","/native-wss-fixture.js","/pkg/","/v1/ws","/view.html?unexpected=1","/remote-ex-not-a-reviewed-resource"];
class ProbeError extends Error{}
function require(ok,message){if(!ok)throw new ProbeError(message);}
export function exactOrigin(value){
    require(typeof value==="string"&&value.length<=512,"Invalid HTTPS origin");
    let u;try{u=new URL(value);}catch{throw new ProbeError("Invalid HTTPS origin");}
    require(u.protocol==="https:"&&u.origin===value&&!u.username&&!u.password&&!u.search&&!u.hash&&u.pathname==="/",
        "Use exact HTTPS origin without path, credentials, query or trailing slash");
    require(!isIP(u.hostname)&&u.hostname.length<=253&&
        u.hostname.split(".").every(v=>/^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/.test(v))&&
        (u.hostname==="localhost"||u.hostname.includes(".")),"Use a reviewed DNS hostname");
    return u;
}
function hash(bytes){return createHash("sha256").update(bytes).digest("hex");}
function headerValues(response,name){
    const key=name.toLowerCase(),values=[];
    for(let i=0;i<response.rawHeaders.length;i+=2){
        if(response.rawHeaders[i].toLowerCase()===key)values.push(response.rawHeaders[i+1]);
    }
    return values;
}
function equalHeader(response,name,value,route){
    const values=headerValues(response,name);
    require(values.length===1&&values[0]===value,"Unexpected "+name+" at "+route);
}
function noHeader(response,name,route){
    require(headerValues(response,name).length===0,"Unexpected "+name+" at "+route);
}
function request(origin,route,method,expected,headers,signal,ca){
    return new Promise((resolve,reject)=>{
        const deadline=AbortSignal.any([signal,AbortSignal.timeout(8000)]);
        // Node's ordinary chain and hostname verification remain mandatory.
        // An optional explicit private CA is only passed by the test harness;
        // the CLI uses the Node trust store (including NODE_EXTRA_CA_CERTS).
        const req=https.request(origin,{
            method,path:route,agent:false,rejectUnauthorized:true,minVersion:"TLSv1.2",
            ...(ca===undefined?{}:{ca}),
            signal:deadline,maxHeaderSize:16384,
            headers:{"Accept-Encoding":"identity","Accept":"*/*","Cache-Control":"no-cache"}
        },response=>{
            const socket=response.socket;
            let count=0,done=false;
            const digest=createHash("sha256");
            const limit=expected?(method==="HEAD"?0:expected.body.length):MAX_ERROR_BODY;
            function fail(error){
                if(done)return;
                done=true;response.destroy();req.destroy();
                reject(error instanceof ProbeError?error:new ProbeError("Invalid HTTPS response at "+route));
            }
            try{
                require(socket?.authorized===true,"Untrusted TLS certificate at "+route);
                const version=socket.getProtocol();
                require(version==="TLSv1.2"||version==="TLSv1.3","Unexpected TLS version at "+route);
                if(expected){
                    require(response.statusCode===200,"Expected HTTP 200 at "+route);
                    equalHeader(response,"Content-Type",expected.type,route);
                    equalHeader(response,"Content-Length",String(expected.body.length),route);
                }else require(response.statusCode===403||response.statusCode===404,"Unreviewed path was not denied: "+route);
                // Require the canonical reviewed security policy, not merely
                // presence of a CSP or a reverse proxy's merged header.
                for(const [name,value] of Object.entries(headers))equalHeader(response,name,value,route);
                noHeader(response,"Set-Cookie",route);noHeader(response,"Location",route);
                const encoding=headerValues(response,"Content-Encoding");
                require(encoding.length===0||encoding.length===1&&encoding[0]==="identity","Compressed or ambiguous response at "+route);
                response.on("data",chunk=>{
                    if(done)return;
                    count+=chunk.length;
                    if(count>limit){fail(new ProbeError("Response exceeds reviewed bound at "+route));return;}
                    digest.update(chunk);
                });
                response.on("aborted",()=>fail(new ProbeError("Incomplete HTTPS response at "+route)));
                response.on("error",()=>fail(new ProbeError("HTTPS response error at "+route)));
                response.on("end",()=>{
                    if(done)return;
                    try{
                        if(expected&&method==="GET"){
                            require(count===expected.body.length&&digest.digest("hex")===hash(expected.body),"Asset digest mismatch at "+route);
                        }else if(method==="HEAD")require(count===0,"Unexpected HEAD body at "+route);
                        require(response.complete,"Incomplete HTTPS response at "+route);
                        done=true;resolve({bytes:count,version});
                    }catch(e){fail(e);}
                });
            }catch(e){fail(e);}
        });
        req.on("error",()=>reject(new ProbeError("HTTPS trust, connection or deadline failure at "+route)));
        req.end();
    });
}
export async function verifyLiveSite(root,originValue,expectedDigest,{ca}={}){
    const origin=exactOrigin(originValue);
    require(typeof expectedDigest==="string"&&/^[0-9a-f]{64}$/.test(expectedDigest),"Expected reviewed SHA-256 tree digest required");
    let expected;
    try{expected=await snapshot(root,{publicOptIn:false});}
    catch{throw new ProbeError("Local reviewed asset tree failed verification");}
    require(expected.verified.tree_sha256===expectedDigest,"Local tree differs from independently reviewed digest");
    // All paths come from the reviewed fixed table. No crawling, redirects,
    // operator-provided extra paths, WSS upgrade or ICE/STUN/TURN traffic.
    const deadline=AbortSignal.timeout(60000),versions=new Set();
    let requests=0,servedBytes=0;
    async function check(route,method,item){
        const result=await request(origin,route,method,item,expected.headers,deadline,ca);
        versions.add(result.version);requests++;
        if(item&&method==="GET")servedBytes+=result.bytes;
    }
    for(const [route,item] of expected.table)await check(route,"GET",item);
    for(const route of ["/","/pkg/noise_bg.wasm"])await check(route,"HEAD",expected.table.get(route));
    for(const route of deniedPaths)await check(route,"GET",null);
    return {schema:1,valid:true,tree_sha256:expectedDigest,mode:expected.verified.mode,
        control:expected.verified.control,extended:expected.verified.extended,asset_routes:expected.table.size,
        verified_bytes:servedBytes,denied_routes:deniedPaths.length,requests,tls_versions:[...versions].sort(),
        exact_security_headers:true,signaling_connected:false,service_changed:false};
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
    try{
        require(process.argv.length===5,"Usage: node scripts/verify-controller-site.mjs STAGED_DIRECTORY EXACT_HTTPS_ORIGIN REVIEWED_TREE_SHA256");
        console.log(JSON.stringify(await verifyLiveSite(path.resolve(process.argv[2]),process.argv[3],process.argv[4]),null,2));
    }catch(e){
        console.error(e instanceof ProbeError?e.message:"Controller site verification failed");
        process.exitCode=1;
    }
}
