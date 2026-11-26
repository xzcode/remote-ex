// Local-only static fixture server. Fixed files, no proxy, API, traversal,
// credentials, registration, or remote connections. Never bind publicly.
import http from "node:http";
import {readFile} from "node:fs/promises";
import {existsSync} from "node:fs";
import {fileURLToPath,pathToFileURL} from "node:url";
import path from "node:path";
// Source-only optional policy module is loaded only when viewer enabled.
const root=path.dirname(fileURLToPath(import.meta.url));
const routes=new Map([
 ["/",["index.html","text/html; charset=utf-8"]],
 ["/index.html",["index.html","text/html; charset=utf-8"]],
 ["/style.css",["style.css","text/css; charset=utf-8"]],
 ["/app.js",["app.js","text/javascript; charset=utf-8"]],
 ["/crypto.js",["crypto.js","text/javascript; charset=utf-8"]],
 ["/wasm-loader.js",["wasm-loader.js","text/javascript; charset=utf-8"]],
 ["/capabilities.js",["capabilities.js","text/javascript; charset=utf-8"]],
 ["/controller-ui.js",["controller-ui.js","text/javascript; charset=utf-8"]],
 ["/identity.html",["identity.html","text/html; charset=utf-8"]],
 ["/identity.js",["identity.js","text/javascript; charset=utf-8"]],
 ["/identity-page.js",["identity-page.js","text/javascript; charset=utf-8"]],
 ["/pkg/noise.js",["pkg/noise.js","text/javascript; charset=utf-8"]],
 ["/pkg/noise_bg.wasm",["pkg/noise_bg.wasm","application/wasm"]],
 ["/v1-fixture.json",[existsSync(path.join(root,"v1-fixture.json"))?"v1-fixture.json":"../../docs/protocol/v1-fixture.json","application/json; charset=utf-8"]]
]);
export const headers={
 "Content-Security-Policy":"default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; connect-src 'self'; media-src 'self' blob:; img-src 'self'; base-uri 'none'; frame-ancestors 'none'; object-src 'none'; form-action 'none'",
 "Permissions-Policy":"camera=(), microphone=(), display-capture=(), clipboard-read=(), clipboard-write=()",
 "Cross-Origin-Resource-Policy":"same-origin",
 "X-Content-Type-Options":"nosniff",
 "Referrer-Policy":"no-referrer",
 "Cache-Control":"no-store"
};
// fixtureEndpoint is used only by the disposable integration harness. The
// normal command and distributed archive have neither this route nor WSS CSP.
// Viewer is separately opted in by the local operator. The configured
// endpoint is fixed for the life of this process, not a user-input proxy.
export function viewEndpoint(value){
 const u=new URL(value);
 if(typeof value!=="string"||value.length>512||u.protocol!=="wss:"||u.pathname!=="/v1/ws"||!u.hostname||u.username||u.password||u.search||u.hash||u.href!==value||!/^[a-zA-Z0-9.:\[\]-]+$/.test(u.host))throw Error("Use exact WSS endpoint /v1/ws, no credentials or parameters");
 return u;
}
export async function serve(port=0,fixtureEndpoint=null,viewer=null,options={stun:null,turn:[],relayOnly:false,control:false,extended:false}){
 if(!Number.isInteger(port)||port<0||port>65535)throw Error("Invalid local port");
 const table=new Map(routes);let viewConfig=null;
 const policy={...headers};
 if(fixtureEndpoint!==null){
    const endpoint=new URL(fixtureEndpoint);
    if(endpoint.protocol!=="wss:"||endpoint.hostname!=="127.0.0.1"||!endpoint.port||endpoint.pathname!=="/v1/ws"||endpoint.username||endpoint.password||endpoint.search||endpoint.hash||endpoint.href!==fixtureEndpoint)throw Error("Only the exact local WSS fixture is allowed");
    table.set("/wss-fixture.js",["wss-fixture.js","text/javascript; charset=utf-8"]);
    table.set("/native-wss-fixture.js",["native-wss-fixture.js","text/javascript; charset=utf-8"]);
    policy["Content-Security-Policy"]=policy["Content-Security-Policy"].replace("connect-src 'self'","connect-src 'self' "+endpoint.origin);
 }
 if(viewer!==null){
    const ep=viewEndpoint(viewer);
    if(!["view.html","view-page.js","view-session.js","ice-policy.js","input.js","touch.js","mobile.html","mobile.js","session-policy.js","connection-book.js","wasm-loader.js","capabilities.js"].every(file=>existsSync(path.join(root,file))))throw Error("Source-only viewer assets unavailable; use the repository checkout");
    if(fixtureEndpoint!==null&&fixtureEndpoint!==viewer)throw Error("Only one WSS endpoint allowed");
    const {normalizePolicy}=await import("./ice-policy.js");
    const {stun,turn,relayOnly,control,extended=false}=options;
    if(typeof control!=="boolean"||typeof extended!=="boolean")throw Error("Invalid control or duration policy");
    viewConfig={schema:1,mode:"direct-read-only",endpoint:viewer,ice:normalizePolicy({stun,turn,relayOnly}),control,extended};
    table.set("/ice-policy.js",["ice-policy.js","text/javascript; charset=utf-8"]);
    table.set("/connection-book.js",["connection-book.js","text/javascript; charset=utf-8"]);
    table.set("/session-policy.js",["session-policy.js","text/javascript; charset=utf-8"]);
    table.set("/input.js",["input.js","text/javascript; charset=utf-8"]);
    table.set("/touch.js",["touch.js","text/javascript; charset=utf-8"]);
    table.set("/mobile.html",["mobile.html","text/html; charset=utf-8"]);
    table.set("/mobile.js",["mobile.js","text/javascript; charset=utf-8"]);
    table.set("/view.html",["view.html","text/html; charset=utf-8"]);
    table.set("/view-page.js",["view-page.js","text/javascript; charset=utf-8"]);
    table.set("/view-session.js",["view-session.js","text/javascript; charset=utf-8"]);
    table.set("/view-config.json",[null,"application/json; charset=utf-8"]);
    if(fixtureEndpoint===null)policy["Content-Security-Policy"]=policy["Content-Security-Policy"].replace("connect-src 'self'","connect-src 'self' "+ep.origin);
 }
 const server=http.createServer(async(req,res)=>{
    const validHost=req.headers.host===`127.0.0.1:${server.address()?.port}`;
    if(!validHost||req.method!=="GET"||!table.has(req.url)){
      res.writeHead(404,{"Content-Type":"text/plain",...policy});res.end("Not found");return;
    }
    const [file,type]=table.get(req.url);
    try{
      const data=file===null?Buffer.from(JSON.stringify(viewConfig)):await readFile(path.resolve(root,file));
      if(data.length>16*1024*1024)throw Error("Fixture asset exceeds bound");
      res.writeHead(200,{"Content-Type":type,"Content-Length":data.length,...policy});res.end(data);
    }catch{res.writeHead(503,{"Content-Type":"text/plain",...policy});res.end("Build the local WASM fixture first.");}
 });
 server.requestTimeout=5000;server.headersTimeout=5000;server.keepAliveTimeout=1000;
 await new Promise((resolve,reject)=>{server.once("error",reject);server.listen(port,"127.0.0.1",resolve);});
 return {server,url:`http://127.0.0.1:${server.address().port}`};
}
if(process.argv[1]&&import.meta.url===pathToFileURL(path.resolve(process.argv[1])).href){
 const port=Number(process.argv[2]??8080);
 const args=process.argv.slice(3);
 let viewer=null;const options={stun:null,turn:[],relayOnly:false,control:false,extended:false};
 if(args.length){
    if(args.shift()!=="--view"||!args.length)throw Error("Usage: node serve.mjs [port] [--view wss://host/v1/ws [--stun URI] [--turn URI] [--relay-only] [--allow-control] [--allow-extended]]");
    viewer=args.shift();
    while(args.length){
        const flag=args.shift();
        if(flag==="--stun"&&options.stun===null&&args.length)options.stun=args.shift();
        else if(flag==="--turn"&&args.length&&options.turn.length<4)options.turn.push(args.shift());
        else if(flag==="--relay-only"&&!options.relayOnly)options.relayOnly=true;
        else if(flag==="--allow-control"&&!options.control)options.control=true;
        else if(flag==="--allow-extended"&&!options.extended)options.extended=true;
        else throw Error("Invalid viewer option");
    }
 }
 const {server,url}=await serve(port,null,viewer,options);console.log(viewer?"Local read-only viewer: "+url+"/view.html":"Local synthetic-only lab: "+url);
 for(const sig of ["SIGINT","SIGTERM"])process.once(sig,()=>server.close(()=>process.exit(0)));
}
