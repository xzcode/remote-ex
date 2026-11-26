// Real native no-bind preflight against a disposable configuration. No live
// services, real device IDs, production certificates or TURN credentials.
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import {execFileSync} from "node:child_process";
import {names,config,policyFor} from "./stage-mobile-client.mjs";
import {verify} from "./verify-mobile-stage.mjs";
import {verifyDeployment,relayPolicy,plan,checkProtocol} from "./verify-controller-deployment.mjs";
const dir=await fs.mkdtemp(path.join(os.tmpdir(),"remote-ex-deployment-"));
try{
    const root=path.join(dir,"assets");await fs.mkdir(root);await fs.mkdir(path.join(root,"pkg"));
    const origin="https://controller.example.net",endpoint="wss://signal.example.net/v1/ws";
    const relayA="turns://relay-a.example.net:5349",relayB="turns://relay-b.example.net:5349";
    const v=config({endpoint,ice:{stun:"stun://stun.example.net:3478",turn:[relayA,relayB],relayOnly:true},control:true,extended:true});
    for(const n of names)await fs.writeFile(path.join(root,n),n==="pkg/noise_bg.wasm"?Buffer.from([0,97,115,109,1,0,0,0]):"fixture\n");
    await fs.writeFile(path.join(root,"index.html"),"fixture\n");
    await fs.writeFile(path.join(root,"view-config.json"),JSON.stringify(v));
    await fs.writeFile(path.join(root,"headers.json"),JSON.stringify(policyFor(v)));
    const digest=(await verify(root)).tree_sha256;
    const review={schema:1,tree_sha256:digest,site_origin:origin,signaling_endpoint:endpoint,server_id:"12".repeat(32),android:true};
    const cert=path.join(dir,"cert.pem"),key=path.join(dir,"key.pem"),allow=path.join(dir,"allow.txt"),pool=path.join(dir,"pool.json");
    execFileSync("openssl",["req","-x509","-newkey","rsa:2048","-nodes","-days","1","-subj","/CN=signal.example.net","-addext","subjectAltName=DNS:signal.example.net","-keyout",key,"-out",cert],{stdio:"ignore",timeout:10000});
    await fs.writeFile(allow,"01".repeat(32)+"\n"+"02".repeat(32)+"\n");
    const a=path.join(dir,"a.secret"),b=path.join(dir,"b.secret");
    await fs.writeFile(a,"a".repeat(40),{mode:0o600});await fs.writeFile(b,"b".repeat(40),{mode:0o600});
    const rows=[
        {id:"a",region:"asia",priority:1,url:relayA,secret_file:a},
        {id:"b",region:"asia",priority:1,url:relayB,secret_file:b},
        {id:"backup",region:"asia",priority:2,url:"turns://backup.example.net:5349",secret_file:a},
        {id:"eu",region:"eu",priority:1,url:"turns://eu.example.net:5349",secret_file:b}
    ];
    const save=()=>fs.writeFile(pool,JSON.stringify({schema:1,relays:rows}));
    await save();
    const env={...Object.fromEntries(Object.entries(process.env).filter(([k])=>!k.startsWith("REMOTE_EX_"))),
        REMOTE_EX_SIGNAL_BIND:"127.0.0.1:47822",REMOTE_EX_SERVER_ID:review.server_id,REMOTE_EX_TLS_CERT:cert,REMOTE_EX_TLS_KEY:key,
        REMOTE_EX_ALLOWED_DEVICES_FILE:allow,REMOTE_EX_WEB_ORIGINS:origin+",https://appassets.androidplatform.net",
        REMOTE_EX_STUN_URL:v.ice.stun,REMOTE_EX_RELAY_POOL_FILE:pool,REMOTE_EX_RELAY_REGION:"asia"};
    assert.deepEqual(await relayPolicy(env),{stun:v.ice.stun,eligible:[relayA,relayB],count:4});
    assert.throws(()=>plan({...review,site_origin:origin+"/"}));
    await assert.rejects(relayPolicy({...env,REMOTE_EX_TURN_URL:relayA,REMOTE_EX_TURN_SECRET_FILE:a}),/Conflicting/);
    const binary=process.argv[2]?path.resolve(process.argv[2]):null;
    if(!binary){
        // Local environments without Rust can exercise the same policy parser;
        // native integration remains required by CI.
        await assert.rejects(relayPolicy({...env,REMOTE_EX_RELAY_REGION:"unknown"}),/No eligible/);
        assert.throws(()=>checkProtocol({schema:1},v),/Incompatible/);
        console.log("PASS offline plan and relay policy parser only; native executable not supplied");
    }else{
        const verifyOne=(r=review,e=env)=>verifyDeployment(root,r,binary,e);
        const report=await verifyOne();
        assert.equal(report.valid,true);assert.deepEqual(report.required_profiles,[1,2,22,23]);
        assert.equal(report.eligible_relay_count,2);assert.equal(report.relay_count,4);
        assert.equal(report.network_started,false);assert.equal(report.service_changed,false);
        assert(!JSON.stringify(report).includes(endpoint)&&!JSON.stringify(report).includes(review.server_id)&&!JSON.stringify(report).includes(a));
        await assert.rejects(verifyOne({...review,tree_sha256:"ab".repeat(32)}),/tree digest/);
        await assert.rejects(verifyOne({...review,server_id:"34".repeat(32)}),/server ID/);
        await assert.rejects(verifyOne({...review,signaling_endpoint:"wss://other.example.net/v1/ws"}),/WSS endpoint/);
        await assert.rejects(verifyOne(review,{...env,REMOTE_EX_WEB_ORIGINS:origin}),/Android bundled origin/);
        await assert.rejects(verifyOne(review,{...env,REMOTE_EX_WEB_ORIGINS:"https://other.example.net,https://appassets.androidplatform.net"}),/site origin/);
        await assert.rejects(verifyOne(review,{...env,REMOTE_EX_STUN_URL:"stun://other.example.net:3478"}),/STUN policy/);
        await assert.rejects(verifyOne(review,{...env,REMOTE_EX_RELAY_REGION:"eu"}),/TURN allowlist/);
        await fs.writeFile(allow,"01".repeat(32)+"\n");
        await assert.rejects(verifyOne(),/lacks two enrolled devices/);
        console.log("PASS real native no-bind deployment consistency: reviewed asset digest, WSS pin/certificate, exact site/Android origins, two enrolled devices, eligible pool, signed profile/lease contract");
    }
}finally{await fs.rm(dir,{recursive:true,force:true});}
