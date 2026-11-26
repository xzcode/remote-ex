// Local-only static fixture server. Fixed files, no proxy, API, traversal,
// credentials, registration, or remote connections. Never bind publicly.
import http from "node:http";
import {readFile} from "node:fs/promises";
import {existsSync} from "node:fs";
import {fileURLToPath,pathToFileURL} from "node:url";
import path from "node:path";
const root=path.dirname(fileURLToPath(import.meta.url));
const routes=new Map([
 ["/",["index.html","text/html; charset=utf-8"]],
 ["/index.html",["index.html","text/html; charset=utf-8"]],
 ["/style.css",["style.css","text/css; charset=utf-8"]],
 ["/app.js",["app.js","text/javascript; charset=utf-8"]],
 ["/crypto.js",["crypto.js","text/javascript; charset=utf-8"]],
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
export async function serve(port=0,fixtureEndpoint=null){
 if(!Number.isInteger(port)||port<0||port>65535)throw Error("Invalid local port");
 const table=new Map(routes);
 const policy={...headers};
 if(fixtureEndpoint!==null){
    const endpoint=new URL(fixtureEndpoint);
    if(endpoint.protocol!=="wss:"||endpoint.hostname!=="127.0.0.1"||!endpoint.port||endpoint.pathname!=="/v1/ws"||endpoint.username||endpoint.password||endpoint.search||endpoint.hash||endpoint.href!==fixtureEndpoint)throw Error("Only the exact local WSS fixture is allowed");
    table.set("/wss-fixture.js",["wss-fixture.js","text/javascript; charset=utf-8"]);
    table.set("/native-wss-fixture.js",["native-wss-fixture.js","text/javascript; charset=utf-8"]);
    policy["Content-Security-Policy"]=policy["Content-Security-Policy"].replace("connect-src 'self'","connect-src 'self' "+endpoint.origin);
 }
 const server=http.createServer(async(req,res)=>{
    const validHost=req.headers.host===`127.0.0.1:${server.address()?.port}`;
    if(!validHost||req.method!=="GET"||!table.has(req.url)){
      res.writeHead(404,{"Content-Type":"text/plain",...policy});res.end("Not found");return;
    }
    const [file,type]=table.get(req.url);
    try{
      const data=await readFile(path.resolve(root,file));
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
 const {server,url}=await serve(port);console.log("Local synthetic-only lab:",url);
 for(const sig of ["SIGINT","SIGTERM"])process.once(sig,()=>server.close(()=>process.exit(0)));
}
