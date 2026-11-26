// Real WSS integration using the repository's existing rendezvous executable,
// Chromium, and a new private CA in an isolated disposable browser HOME.
// No public host, no production secrets, no certificate bypass, no OS capture.
import {execFileSync} from "node:child_process";
import {mkdirSync,writeFileSync,chmodSync} from "node:fs";
import path from "node:path";
import net from "node:net";
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

export {freePort,certificates,installRoot,ready,stopChild};
