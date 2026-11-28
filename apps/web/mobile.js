import {probeCapabilities,compatibilityGuidance} from "./capabilities.js";
import {readIdentity} from "./identity.js";
import {BOOK_KEY,readBook,forEndpoint,connectionKey,HISTORY_KEY,readHistory} from "./connection-book.js";
import {applyAppearance,setAppearance,selectForReview,applyLanguage,setLanguage,takeSettingsPage,uiText as t} from "./controller-ui.js";
const $=id=>document.getElementById(id);
const status=$("mobile-status"),checks=$("mobile-checks"),open=$("mobile-connect"),policy=$("mobile-policy"),blocker=$("mobile-blocker");
const search=$("mobile-device-search"),theme=$("mobile-theme"),recheck=$("mobile-recheck");
let configuredEndpoint=null,ready=false,running=false,rows=[];
theme.value=applyAppearance();
$("mobile-language").value=applyLanguage();
$("mobile-language").addEventListener("change",()=>{if(setLanguage($("mobile-language").value))location.reload();else $("language-status").textContent=t("Language preference could not be saved.");});
theme.addEventListener("change",()=>{$("appearance-status").textContent=setAppearance(theme.value)?"":t("Theme applies to this page; local preferences unavailable.");});
const panels=[...document.querySelectorAll("[data-home-panel]")];
function showPanel(id){
    if(!panels.some(p=>p.id===id))return;
    for(const p of panels)p.hidden=p.id!==id;
    $("history-confirm").hidden=true;
    if(id==="home-history")renderHistory();
    for(const b of document.querySelectorAll(".mobile-nav [data-home-target]")){
        if(b.dataset.homeTarget===id)b.setAttribute("aria-current","page");
        else b.removeAttribute("aria-current");
    }
}
for(const b of document.querySelectorAll("[data-home-target]"))b.addEventListener("click",()=>showPanel(b.dataset.homeTarget));
if(takeSettingsPage())showPanel("home-settings");
function review(entry){
    if(!ready||!configuredEndpoint||entry.endpoint!==configuredEndpoint)return;
    // Only a single-use reference into the validated local book; no URL
    // parameters, secret, profile choice, consent or automatic connection.
    if(!selectForReview(connectionKey(entry))){blocker.textContent=t("Local selection unavailable. Open the controller and choose the saved connection there.");blocker.hidden=false;return;}
    location.assign("/view.html");
}
function renderList(container,entries){
    container.replaceChildren();
    if(!entries.length){const empty=document.createElement("p");empty.className="empty-state muted";empty.textContent=t("No saved devices.");container.append(empty);return;}
    for(const entry of entries){
        const button=document.createElement("button");button.type="button";button.className="saved-row";button.disabled=!ready;
        const mark=document.createElement("span");mark.className="saved-mark";mark.textContent="PC";mark.setAttribute("aria-hidden","true");
        const content=document.createElement("span");content.className="saved-name";
        const title=document.createElement("strong");title.textContent=entry.label;const sub=document.createElement("small");sub.textContent=t("Saved connection");
        content.append(title,sub);const arrow=document.createElement("span");arrow.className="saved-arrow";arrow.textContent="›";
        button.append(mark,content,arrow);button.addEventListener("click",()=>review(entry));container.append(button);
    }
}
function renderHistory(){
    const list=$("mobile-history"),notice=$("mobile-history-status"),clear=$("history-clear");
    list.replaceChildren();notice.textContent="";
    let history;
    try{history=readHistory(localStorage.getItem(HISTORY_KEY));}
    catch{notice.textContent=t("Local history is unavailable or invalid. You can clear it.");clear.disabled=false;return;}
    clear.disabled=!history.length;
    if(!history.length){const p=document.createElement("p");p.className="empty-state muted";p.textContent=t("No saved sessions.");list.append(p);return;}
    const known=new Map(rows.map(v=>[connectionKey(v),v]));
    for(const record of history){
        const entry=known.get(connectionKey(record));
        const row=document.createElement("div");row.className="history-row";
        const details=document.createElement("div");details.className="saved-name";
        const title=document.createElement("strong");title.textContent=entry?.label??t("Unsaved device")+" · "+record.hostKey.slice(0,8);
        const date=document.createElement("small");date.textContent=new Date(record.endedAt).toLocaleString(document.documentElement.lang);
        const info=document.createElement("small");info.textContent=[t(record.mode==="control"?"Control session":"View session"),record.duration+" s",t(record.route==="direct"?"Direct":record.route==="relay"?"TURN relay":"Unknown route")].join(" · ");
        const outcome=document.createElement("span");outcome.className="history-outcome";outcome.textContent=t(record.outcome==="ended"?"Ended":"Interrupted");outcome.dataset.outcome=record.outcome;
        details.append(title,date,info,outcome);row.append(details);
        if(entry&&ready&&entry.endpoint===configuredEndpoint){
            const button=document.createElement("button");button.type="button";button.className="history-review";button.textContent=t("Review connection");button.addEventListener("click",()=>review(entry));row.append(button);
        }
        list.append(row);
    }
}
$("history-clear").addEventListener("click",e=>{if(e.isTrusted)$("history-confirm").hidden=false;});
$("history-clear-cancel").addEventListener("click",()=>{$("history-confirm").hidden=true;});
$("history-clear-confirm").addEventListener("click",e=>{
    if(!e.isTrusted||$("history-confirm").hidden)return;
    try{localStorage.removeItem(HISTORY_KEY);$("history-confirm").hidden=true;renderHistory();$("mobile-history-status").textContent=t("Local history cleared.");}
    catch{$("mobile-history-status").textContent=t("Local history could not be cleared.");}
});
function render(){
    const q=search.value.trim().toLowerCase();
    renderList($("mobile-recent"),rows.slice(0,3));
    renderList($("mobile-devices"),rows.filter(e=>e.label.toLowerCase().includes(q)));
    if(!$("home-history").hidden)renderHistory();
}
function loadBook(){
    try{rows=configuredEndpoint?forEndpoint(readBook(localStorage.getItem(BOOK_KEY)),configuredEndpoint):[];}
    catch{rows=[];}
    render();
}
search.addEventListener("input",render);
window.addEventListener("storage",e=>{if(e.key===BOOK_KEY||e.key===null)loadBook();if(e.key===HISTORY_KEY&&!$("home-history").hidden){$("history-confirm").hidden=true;renderHistory();}});
async function identityStatus(){
    try{const i=await readIdentity();$("mobile-identity").textContent=i?t("Local identity verified: ")+i.id.slice(0,12)+"…":t("No local identity. Create one explicitly before connecting.");}
    catch{$("mobile-identity").textContent=t("Local identity unavailable. Review compatibility or manage identity.");}
}
async function run(){
    if(running)return;running=true;ready=false;configuredEndpoint=null;open.hidden=true;recheck.disabled=true;blocker.hidden=true;loadBook();
    status.dataset.state="checking";status.textContent=t("Checking this device…");
    try{
        const results=await probeCapabilities();
        checks.replaceChildren();
        for(const result of results){
            const item=document.createElement("li");
            item.dataset.capability=result.id;item.dataset.code=result.code;item.dataset.pass=String(result.ok);
            const label=document.createElement("span");label.textContent=t(result.label);
            const detail=document.createElement("small");detail.textContent=result.ok?t("Available"):result.code;
            item.append(label,detail);checks.append(item);
        }
        const all=results.length===6&&results.every(v=>v.ok);
        status.dataset.state=all?"ready":"unsupported";
        status.textContent=all?t("Required browser APIs detected; actual connection still needs verification."):t(compatibilityGuidance(results));
        if(!all){blocker.textContent=status.textContent;blocker.hidden=false;}
        try{
            const r=await fetch("/view-config.json",{cache:"no-store"}),v=await r.json();
            if(r.ok&&v.schema===1&&v.mode==="direct-read-only"&&typeof v.endpoint==="string"){
                // forEndpoint validates the exact same endpoint shape even with an empty book.
                forEndpoint([],v.endpoint);configuredEndpoint=v.endpoint;
            }
            ready=!!configuredEndpoint&&all;
            if(ready){open.hidden=false;policy.textContent=t("Operator endpoint configured. Independent identity verification and host approval required.");}
            else policy.textContent=t("No enabled compatible connection configuration. Identity management is local only.");
        }catch{policy.textContent=t("No connection configuration. Identity management is local only.");}
        loadBook();void identityStatus();
    }catch{
        status.dataset.state="error";status.textContent=t("Compatibility probe failed; no connection attempted.");
        blocker.textContent=status.textContent;blocker.hidden=false;policy.textContent=t("No connection configuration. Identity management is local only.");
    }finally{running=false;recheck.disabled=false;}
}
recheck.addEventListener("click",()=>void run());
void run();
