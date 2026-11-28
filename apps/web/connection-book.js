// Optional local connection details, never authority or a session token.
// No consent, private keys, ICE credentials or reconnect state is persisted.
export const BOOK_KEY="remote-ex.connection-book.v1";
const LIMIT=24,MAX_BYTES=32768;
function exactKey(v){return typeof v==="string"&&/^[0-9a-f]{64}$/.test(v)&&!/^0+$/.test(v);}
function checkedEndpoint(v){
    if(typeof v!=="string"||v.length>512)throw Error("Invalid saved endpoint");
    const u=new URL(v);
    if(u.protocol!=="wss:"||u.pathname!=="/v1/ws"||!u.hostname||u.username||u.password||u.search||u.hash||u.href!==v)throw Error("Invalid saved endpoint");
    return v;
}
function entry(v){
    if(!v||typeof v!=="object"||Array.isArray(v)||Object.keys(v).sort().join(",")!=="endpoint,hostKey,label,serverId")throw Error("Invalid saved connection fields");
    if(typeof v.label!=="string"||v.label.length<1||v.label.length>64||v.label.trim()!==v.label||/[\u0000-\u001f\u007f-\u009f]/.test(v.label)||!exactKey(v.serverId)||!exactKey(v.hostKey))throw Error("Invalid saved connection");
    return {endpoint:checkedEndpoint(v.endpoint),serverId:v.serverId,hostKey:v.hostKey,label:v.label};
}
export function connectionKey(v){return JSON.stringify([v.endpoint,v.serverId,v.hostKey]);}
export function readBook(raw){
    if(raw===null)return [];
    if(typeof raw!=="string"||raw.length>MAX_BYTES)throw Error("Connection book exceeds bounds");
    const v=JSON.parse(raw);
    if(!v||typeof v!=="object"||Array.isArray(v)||Object.keys(v).sort().join(",")!=="entries,schema"||v.schema!==1||!Array.isArray(v.entries)||v.entries.length>LIMIT)throw Error("Invalid connection book");
    const entries=v.entries.map(entry);
    if(new Set(entries.map(connectionKey)).size!==entries.length)throw Error("Duplicate saved connection");
    return entries;
}
export function writeBook(entries){
    const value=JSON.stringify({schema:1,entries});
    readBook(value);
    return value;
}
export function saveEntry(entries,value){
    const current=readBook(writeBook(entries)),v=entry(value),k=connectionKey(v),i=current.findIndex(x=>connectionKey(x)===k);
    if(i>=0)current[i]=v;
    else {if(current.length>=LIMIT)throw Error("Connection book is full (24 entries)");current.push(v);}
    return current;
}
export function removeEntry(entries,key){
    const current=readBook(writeBook(entries));
    if(!current.some(x=>connectionKey(x)===key))throw Error("Saved connection not found");
    return current.filter(x=>connectionKey(x)!==key);
}
export function forEndpoint(entries,endpoint){
    const exact=checkedEndpoint(endpoint);
    return readBook(writeBook(entries)).filter(x=>x.endpoint===exact);
}


// Opt-in local history is not the trusted connection book.
export const HISTORY_KEY="remote-ex.connection-history.v1";
const HISTORY_LIMIT=64,HISTORY_BYTES=65536;
function historyEntry(v){
    if(!v||typeof v!=="object"||Array.isArray(v)||Object.keys(v).sort().join(",")!=="duration,endedAt,endpoint,hostKey,mode,outcome,route,serverId")throw Error("Invalid history fields");
    if(!exactKey(v.serverId)||!exactKey(v.hostKey)||
        !Number.isSafeInteger(v.endedAt)||v.endedAt<=0||v.endedAt>8640000000000000||
        !Number.isInteger(v.duration)||v.duration<0||v.duration>300||
        !["view","control"].includes(v.mode)||!["ended","interrupted"].includes(v.outcome)||
        !["direct","relay","unknown"].includes(v.route))throw Error("Invalid history record");
    return {endpoint:checkedEndpoint(v.endpoint),serverId:v.serverId,hostKey:v.hostKey,endedAt:v.endedAt,duration:v.duration,mode:v.mode,outcome:v.outcome,route:v.route};
}
export function readHistory(raw){
    if(raw===null)return [];
    if(typeof raw!=="string"||raw.length>HISTORY_BYTES)throw Error("History exceeds bounds");
    const v=JSON.parse(raw);
    if(!v||typeof v!=="object"||Array.isArray(v)||Object.keys(v).sort().join(",")!=="entries,schema"||v.schema!==1||!Array.isArray(v.entries)||v.entries.length>HISTORY_LIMIT)throw Error("Invalid history");
    return v.entries.map(historyEntry).sort((a,b)=>b.endedAt-a.endedAt);
}
export function writeHistory(entries){
    const value=JSON.stringify({schema:1,entries});readHistory(value);return value;
}
export function appendHistory(entries,value){
    const current=readHistory(writeHistory(entries));
    return [historyEntry(value),...current].sort((a,b)=>b.endedAt-a.endedAt).slice(0,HISTORY_LIMIT);
}
