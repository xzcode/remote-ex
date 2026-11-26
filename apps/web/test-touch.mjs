import assert from "node:assert/strict";
import {TouchGesture} from "./touch.js";
const p={x:100,y:200},q={x:500,y:600};
function fixture(mode){const events=[];return {events,t:new TouchGesture(mode,(kind,value)=>{events.push([kind,value]);return true;})};}
let {events,t}=fixture("tap");
assert(t.start(1,p,100,100));assert(t.end(1,p,105,105));
assert.deepEqual(events,[["move",p],["button",{button:1,down:true}],["button",{button:1,down:false}]]);
({events,t}=fixture("tap"));assert(t.start(1,p,100,100));assert(!t.start(2,q,110,110));assert(!t.move(2,q,110,110,40));assert(t.move(1,q,150,150,50));assert(t.end(1,q,150,150));assert.equal(events.length,0);
({events,t}=fixture("right"));t.start(1,p,100,100);t.end(1,p,100,100);assert.equal(events[1][1].button,3);
({events,t}=fixture("drag"));t.start(1,p,100,100);t.move(1,q,130,130,50);t.end(1,q,130,130);assert.deepEqual(events,[["move",p],["button",{button:1,down:true}],["move",q],["move",q],["button",{button:1,down:false}]]);
({events,t}=fixture("scroll"));t.start(1,p,100,100);t.move(1,q,100,76,50);t.end(1,q,100,76);assert.deepEqual(events,[["move",q],["scroll",{x:0,y:240}]]);
({events,t}=fixture("drag"));t.start(1,p,100,100);assert(!t.move(1,null,150,150,50));t.cancel();assert(!t.current&&!t.held);assert(!t.end(1,p,100,100));
assert.throws(()=>new TouchGesture("unknown",()=>true));
console.log("PASS touch click/right/drag/scroll, motion cancellation, multi-pointer rejection and cleanup");
