// Pure event-routing regression. Deliberately fake DOM to test state gates;
// real trusted browser input is independently exercised in test-touch-browser.
import assert from "node:assert/strict";
import {InputEncoder,SurfaceInput} from "./input.js";
class Target {
    constructor(){this.listeners=new Map();}
    addEventListener(name,fn){if(!this.listeners.has(name))this.listeners.set(name,new Set());this.listeners.get(name).add(fn);}
    removeEventListener(name,fn){this.listeners.get(name)?.delete(fn);}
    dispatch(name,event={}){for(const fn of [...(this.listeners.get(name)??[])])fn(event);}
}
const doc=new Target(),win=new Target(),video=new Target(),classes=new Set(),captures=new Set();
globalThis.document=doc;globalThis.window=win;doc.hidden=false;doc.activeElement=null;
Object.assign(video,{videoWidth:960,videoHeight:540,
    classList:{add:v=>classes.add(v),remove:v=>classes.delete(v)},
    getBoundingClientRect:()=>({left:0,top:0,width:960,height:540}),
    focus:()=>{doc.activeElement=video;},removeAttribute:()=>{},
    setPointerCapture:id=>captures.add(id),hasPointerCapture:id=>captures.has(id),releasePointerCapture:id=>captures.delete(id)});
let packets=[],input;
function activate(mode){
    input?.dispose();packets=[];doc.hidden=false;
    input=new SurfaceInput({video,geometry:{x:120,y:0,width:720,height:540},encoder:new InputEncoder(new Uint8Array(16).fill(1)),
        send:p=>packets.push([...p]),onFailure:()=>{throw Error("Unexpected fixture send failure");},onDisabled:()=>{},touchMode:mode});
    input.enable();assert(input.active);
}
function pointer(type,id=1,primary=true,extra={}){
    return {isTrusted:true,pointerType:type,pointerId:id,isPrimary:primary,button:0,clientX:480,clientY:270,preventDefault(){},...extra};
}
function down(event){win.dispatch("pointerdown",event);video.dispatch("pointerdown",event);}
function tags(){return packets.map(p=>p[22]);}
function released(){assert(!input.active);assert.equal(tags().at(-1),114);assert.equal(captures.size,0);assert(!classes.has("touch-active"));}
activate("drag");down(pointer("touch"));assert.deepEqual(tags(),[82,90]);assert(captures.has(1));
down(pointer("mouse",2));released();assert.deepEqual(tags(),[82,90,114]);
video.dispatch("pointerup",pointer("touch"));win.dispatch("pointerup",pointer("mouse",2));assert.deepEqual(tags(),[82,90,114]);
activate("mouse");down(pointer("mouse"));assert.deepEqual(tags(),[82,90]);down(pointer("touch",2));released();assert.deepEqual(tags(),[82,90,114]);
activate("tap");down(pointer("touch",9,false));released();assert.deepEqual(tags(),[114]);
activate("drag");down(pointer("touch"));down(pointer("touch",2,false));released();assert.deepEqual(tags(),[82,90,114]);
activate("drag");down(pointer("touch"));doc.hidden=true;doc.dispatch("visibilitychange");released();assert.deepEqual(tags(),[82,90,114]);
activate("drag");down(pointer("touch"));doc.activeElement=null;video.dispatch("pointerup",pointer("touch"));released();assert.deepEqual(tags(),[82,90,114]);
activate("tap");down(pointer("touch"));video.dispatch("pointerup",pointer("touch"));captures.delete(1);assert.deepEqual(tags(),[82,90,90]);assert(input.active);
input.disable();assert.equal(tags().at(-1),114);assert.equal(input.emit("move",{x:0,y:0}),false);
activate("mouse");down(pointer("mouse"));win.dispatch("pointerup",pointer("touch",3));assert.deepEqual(tags(),[82,90]);win.dispatch("pointerup",pointer("mouse"));assert.deepEqual(tags(),[82,90,90]);
input.dispose();
console.log("PASS pure pointer-source arbitration: mixed mouse/touch, secondary finger, hidden/focus loss and post-disable rejection");
