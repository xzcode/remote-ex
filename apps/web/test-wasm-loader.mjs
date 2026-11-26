import assert from "node:assert/strict";
import {loadControllerWasm,WasmLoadError} from "./wasm-loader.js";
const minimal=new Uint8Array([0,97,115,109,1,0,0,0]);
const old=globalThis.fetch;
try{
    let calls=0;
    const response=(body=minimal,headers={"content-type":"application/wasm"})=>new Response(body,{headers});
    globalThis.fetch=async(url,opts)=>{
        assert(url.href.endsWith("/pkg/noise_bg.wasm"));
        assert.equal(opts.redirect,"error");assert.equal(opts.credentials,"same-origin");
        calls++;return response();
    };
    const init=async({module_or_path})=>{assert.deepEqual(module_or_path,minimal);return WebAssembly.instantiate(module_or_path);};
    const a=await loadControllerWasm(init),b=await loadControllerWasm(init);
    assert.equal(a,b);assert.equal(calls,1);
    const bad=async(code,make)=>{
        globalThis.fetch=make;
        await assert.rejects(loadControllerWasm(async()=>{}),e=>e instanceof WasmLoadError&&e.code===code);
    };
    await bad("wasm_fetch",async()=>{throw Error("fixture");});
    await bad("wasm_http",async()=>new Response("x",{status:404}));
    await bad("wasm_mime",async()=>response(minimal,{"content-type":"text/plain"}));
    await bad("wasm_size",async()=>response(minimal,{"content-type":"application/wasm","content-length":"16777217"}));
    await bad("wasm_validate",async()=>response(new Uint8Array(8)));
    globalThis.fetch=async()=>response();
    await assert.rejects(loadControllerWasm(async()=>{throw Error("fixture");}),e=>e.code==="wasm_init");
    assert(await loadControllerWasm(init));
    console.log("PASS bounded same-origin WASM bytes, cache, content-type, invalid module, fetch and initialization rejection");
}finally{globalThis.fetch=old;}
