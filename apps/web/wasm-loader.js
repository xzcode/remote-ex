// Exact same-origin, bounded WASM loader for Chromium/WebView/Safari.
// No fallback cipher, downloaded code from another origin, or permission change.
export class WasmLoadError extends Error{
    constructor(code){super("Controller WASM: "+code);this.name="WasmLoadError";this.code=code;}
}
const limit=16*1024*1024;
const url=new URL("./pkg/noise_bg.wasm",import.meta.url);
let initialized=null;
export async function loadControllerWasm(init){
    if(typeof init!=="function")throw new WasmLoadError("wasm_module");
    if(initialized?.init===init)return initialized.promise;
    const promise=(async()=>{
        if(typeof WebAssembly!=="object"||typeof WebAssembly.validate!=="function")throw new WasmLoadError("wasm_api");
        let response;
        try{response=await fetch(url,{cache:"no-store",credentials:"same-origin",redirect:"error"});}
        catch{throw new WasmLoadError("wasm_fetch");}
        if(!response.ok||response.redirected)throw new WasmLoadError("wasm_http");
        if(response.headers.get("content-type")?.split(";")[0].trim().toLowerCase()!=="application/wasm")throw new WasmLoadError("wasm_mime");
        const length=response.headers.get("content-length");
        if(length!==null&&(!/^[0-9]+$/.test(length)||Number(length)>limit||Number(length)<8))throw new WasmLoadError("wasm_size");
        let bytes;
        try{
            if(response.body?.getReader){
                const reader=response.body.getReader(),parts=[];let size=0;
                try{
                    while(true){
                        const {done,value}=await reader.read();
                        if(done)break;
                        size+=value.byteLength;
                        if(size>limit){await reader.cancel();throw new WasmLoadError("wasm_size");}
                        parts.push(value);
                    }
                }finally{reader.releaseLock();}
                bytes=new Uint8Array(size);let at=0;
                for(const p of parts){bytes.set(p,at);at+=p.length;}
            }else bytes=new Uint8Array(await response.arrayBuffer());
        }catch(e){if(e instanceof WasmLoadError)throw e;throw new WasmLoadError("wasm_read");}
        if(bytes.length<8||bytes.length>limit||length!==null&&bytes.length!==Number(length))throw new WasmLoadError("wasm_size");
        if(![0,97,115,109,1,0,0,0].every((v,i)=>bytes[i]===v)||!WebAssembly.validate(bytes))throw new WasmLoadError("wasm_validate");
        // Pass the exact verified bytes to wasm-bindgen. This avoids relying
        // on instantiateStreaming support in embedded browser engines.
        try{return await init({module_or_path:bytes});}
        catch{throw new WasmLoadError("wasm_init");}
    })();
    initialized={init,promise};
    try{return await promise;}
    catch(e){if(initialized?.promise===promise)initialized=null;throw e;}
}
