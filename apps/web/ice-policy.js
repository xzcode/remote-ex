// Operator-selected ICE services only. Convert native URI notation into
// RTCIceServer notation without persisting or displaying temporary credentials.
const need=(v,m)=>{if(!v)throw Error(m);};
function fields(v,keys){return v&&typeof v==="object"&&!Array.isArray(v)&&Object.keys(v).sort().join(",")===keys.slice().sort().join(",");}
export function canonicalIce(value,schemes){
    need(typeof value==="string"&&value.length<=512&&!/[\x00-\x20\x7f]/.test(value),"Invalid configured ICE URI");
    const match=/^(stun|turn|turns):\/\/(\[[0-9a-fA-F:.]+\]|[a-zA-Z0-9.-]+):([1-9][0-9]{0,4})\/?$/.exec(value);
    need(match&&schemes.includes(match[1])&&Number(match[3])<=65535,"ICE URI must have explicit scheme, host and port");
    const u=new URL("https://"+match[2]+":"+match[3]+"/");
    need(u.hostname&&u.hostname.toLowerCase()===match[2].toLowerCase(),"Non-canonical ICE host");
    return match[1]+"://"+u.hostname.toLowerCase()+":"+match[3];
}
export function normalizePolicy(value={}){
    need(fields(value,["stun","turn","relayOnly"]),"Invalid operator ICE policy");
    need(Array.isArray(value.turn)&&value.turn.length<=4&&typeof value.relayOnly==="boolean","Invalid relay policy");
    const stun=value.stun===null?null:canonicalIce(value.stun,["stun"]);
    const turn=value.turn.map(x=>canonicalIce(x,["turn","turns"]));
    need(new Set(turn).size===turn.length&&(!value.relayOnly||turn.length>0),"Duplicate or missing relay");
    return {stun,turn,relayOnly:value.relayOnly};
}
export const directPolicy=()=>({stun:null,turn:[],relayOnly:false});
export function iceMode(policy){const p=normalizePolicy(policy);return p.relayOnly?2:(p.stun||p.turn.length?1:0);}
export function rtcIce(config,policy,now=Math.floor(Date.now()/1000)){
    const p=normalizePolicy(policy);
    need(fields(config,["stun","turn"]),"Unexpected ICE configuration");
    const servers=[];
    if(config.stun!==null){
        const uri=canonicalIce(config.stun,["stun"]);need(uri===p.stun,"Server supplied an unapproved STUN endpoint");
        if(!p.relayOnly)servers.push({urls:uri.replace("://",":")});
    }
    if(config.turn!==null){
        const t=config.turn;
        need(fields(t,["url","username","password","expires_at"]),"Invalid temporary relay credentials");
        const uri=canonicalIce(t.url,["turn","turns"]);
        need(p.turn.includes(uri),"Server supplied an unapproved TURN endpoint");
        need(typeof t.username==="string"&&/^[a-zA-Z0-9:_-]{1,160}$/.test(t.username)&&typeof t.password==="string"&&/^[A-Za-z0-9+/=]{28}$/.test(t.password),"Invalid temporary relay credentials");
        need(Number.isSafeInteger(t.expires_at)&&t.expires_at>now+10&&t.expires_at<=now+600,"Temporary relay credentials expired or outside lifetime");
        servers.push({urls:uri.replace("://",":"),username:t.username,credential:t.password});
    }
    need(!p.relayOnly||config.turn!==null,"Forced relay requires an approved TURN credential");
    return {iceServers:servers,iceTransportPolicy:p.relayOnly?"relay":"all",bundlePolicy:"max-bundle"};
}
export function candidateRoute(stats){
    const transports=[...stats.values()].filter(x=>x.type==="transport"&&x.dtlsState==="connected");
    if(transports.length!==1)return "unknown";
    const pair=stats.get(transports[0].selectedCandidatePairId);
    if(pair?.type!=="candidate-pair"||pair.state!=="succeeded")return "unknown";
    const local=stats.get(pair.localCandidateId)?.candidateType,remote=stats.get(pair.remoteCandidateId)?.candidateType;
    if(!["host","srflx","prflx","relay"].includes(local)||!["host","srflx","prflx","relay"].includes(remote))return "unknown";
    return local==="relay"&&remote==="relay"?"relay-both":local==="relay"||remote==="relay"?"relay":"direct";
}
