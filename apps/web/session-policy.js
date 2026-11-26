// Existing native signed profiles: short view/control (1/2), extended (22/23).
// The lease is a routing limit, never a permission grant or automatic renewal.
export const EXTENDED_LEASE_SECONDS=420;
export const QUALITY_MODES=["Auto","Full","Balanced","Low"];
const SIZES=[[960,540],[720,404],[480,270]];
export function sessionPolicy(control,extended){
    if(typeof control!=="boolean"||typeof extended!=="boolean")throw Error("Invalid session policy");
    return Object.freeze({profile:extended?(control?23:22):(control?2:1),grant:control?3:1,seconds:extended?300:30,extended});
}
export function leaseConfirmed(value,session){
    return !!value&&typeof value==="object"&&!Array.isArray(value)&&Object.keys(value).sort().join(",")==="seconds,session,type"&&value.type==="lease_granted"&&value.session===session&&value.seconds===EXTENDED_LEASE_SECONDS;
}
export function qualityStatus(v){
    if(!(v instanceof Uint8Array)||v.length!==3||v[0]!==0xd3||v[1]>3||v[2]<1||v[2]>3||(v[1]!==0&&v[1]!==v[2]))throw Error("Invalid runtime quality status");
    return {mode:v[1],tier:v[2],width:SIZES[v[2]-1][0],height:SIZES[v[2]-1][1]};
}
export function qualityRequest(mode){
    if(!Number.isInteger(mode)||mode<0||mode>3)throw Error("Invalid runtime quality request");
    return Uint8Array.of(0xd2,mode);
}
export function supportedFrame(width,height,extended){
    return (extended?SIZES:[SIZES[0]]).some(([w,h])=>w===width&&h===height);
}
export function mayEnableInput(control,phase,stopping,frames,width,height,quality){
    return control===true&&phase==="viewing"&&!stopping&&Number.isFinite(frames)&&frames>0&&width===960&&height===540&&(!quality||quality.tier===1);
}
export function remainingSeconds(limit,elapsed){
    if(![30,300].includes(limit)||!Number.isFinite(elapsed)||elapsed<0)throw Error("Invalid session clock");
    return Math.max(0,Math.ceil((limit*1000-elapsed)/1000));
}
