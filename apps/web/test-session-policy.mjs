import assert from "node:assert/strict";
import {sessionPolicy,leaseConfirmed,qualityStatus,qualityRequest,supportedFrame,mayEnableInput,remainingSeconds} from "./session-policy.js";
for(const [control,extended,profile,grant,seconds] of [[false,false,1,1,30],[true,false,2,3,30],[false,true,22,1,300],[true,true,23,3,300]]){
    assert.deepEqual(sessionPolicy(control,extended),{profile,grant,seconds,extended});
}
assert.throws(()=>sessionPolicy(true,1));
const lease={type:"lease_granted",session:"01".repeat(16),seconds:420};
assert(leaseConfirmed(lease,lease.session));assert(!leaseConfirmed({...lease,seconds:300},lease.session));assert(!leaseConfirmed({...lease,extra:1},lease.session));assert(!leaseConfirmed(lease,"02".repeat(16)));
assert.deepEqual(qualityStatus(Uint8Array.of(0xd3,0,2)),{mode:0,tier:2,width:720,height:404});
assert.deepEqual(qualityStatus(Uint8Array.of(0xd3,3,3)),{mode:3,tier:3,width:480,height:270});
for(const x of [[0xd3,3,1],[0xd3,0,0],[0xd3,4,1],[0xd2,1,1]])assert.throws(()=>qualityStatus(Uint8Array.from(x)));
assert.deepEqual([...qualityRequest(1)],[0xd2,1]);assert.throws(()=>qualityRequest(4));
assert(supportedFrame(720,404,true));assert(!supportedFrame(720,404,false));assert(!supportedFrame(1280,720,true));
const full=qualityStatus(Uint8Array.of(0xd3,1,1)),low=qualityStatus(Uint8Array.of(0xd3,3,3));
assert(mayEnableInput(true,"viewing",false,1,960,540,full));assert(!mayEnableInput(false,"viewing",false,1,960,540,full));assert(!mayEnableInput(true,"viewing",false,1,960,540,low));assert(!mayEnableInput(true,"viewing",false,1,480,270,full));
assert.equal(remainingSeconds(300,61000),239);assert.equal(remainingSeconds(30,30001),0);
console.log("PASS signed session policy, exact lease, runtime tier bounds, full-resolution input gate and session deadline");
