import assert from "node:assert/strict";
import {TouchGesture,TwoFingerScroll} from "./touch.js";
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

({events,t}=fixture("smart"));assert(t.start(1,p,100,100,10));assert(t.end(1,p,100,100,100));assert.deepEqual(events.map(x=>x[0]),["move","move","button","button"]);assert.equal(events[2][1].button,1);
({events,t}=fixture("smart"));t.start(1,p,100,100,10);t.end(1,p,100,100,560);assert.equal(events[2][1].button,3);
({events,t}=fixture("smart"));t.start(1,p,100,100,10);t.move(1,q,150,150,50);t.end(1,q,150,150,700);assert(events.every(x=>x[0]==="move"));
({events,t}=fixture("smart"));t.start(1,p,100,100,10);t.cancel();assert(!t.end(1,p,100,100,700));assert.deepEqual(events,[["move",p]]);
console.log("PASS smart pointer: click, release-time long press, motion without button, canceled touch");

const wheelEvents=[],two=new TwoFingerScroll((kind,value)=>{wheelEvents.push([kind,value]);return true;});
assert(two.start(1,p,100,100,0));assert(two.move(1,p,100,60,50));assert.equal(wheelEvents.length,0);
assert(two.start(2,q,160,60,60));assert(two.move(1,p,100,36,110));assert.equal(wheelEvents.length,0);
assert(two.move(2,q,160,36,111));assert.deepEqual(wheelEvents,[["move",{x:300,y:400}],["scroll",{x:0,y:240}]]);
assert(!two.start(3,q,150,20,120));assert(two.end(1));assert(two.move(2,q,160,0,150));assert(!two.start(3,q,140,40,160));assert.equal(wheelEvents.length,2);assert(two.end(2));
two.cancel();wheelEvents.length=0;
assert(two.start(1,p,100,100,0));assert(two.start(2,q,160,100,1));assert(two.move(1,p,70,100,40));assert(two.move(2,q,190,100,41));assert.deepEqual(wheelEvents,[]);
two.cancel();assert.equal(two.points.size,0);
console.log("PASS explicit two-finger scroll: paired translation, pinch refusal, no lone-finger or trailing scroll");
