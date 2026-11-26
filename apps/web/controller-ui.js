// Local appearance and one-shot connection-book selection only. Neither
// preference authorizes a session, registers a device or stores secrets.
export const APPEARANCE_KEY="remote-ex.controller.appearance.v1";
export const SELECTION_KEY="remote-ex.controller.selection.v1";
export const THEMES=Object.freeze(["system","light","gray","dark","black"]);
export function applyAppearance(){
    let theme="system";
    try{const v=localStorage.getItem(APPEARANCE_KEY);if(THEMES.includes(v))theme=v;}catch{}
    document.documentElement.dataset.theme=theme;
    return theme;
}
export function setAppearance(theme){
    if(!THEMES.includes(theme))return false;
    document.documentElement.dataset.theme=theme;
    try{localStorage.setItem(APPEARANCE_KEY,theme);return true;}catch{return false;}
}
export function selectForReview(key){
    if(typeof key!=="string"||key.length<1||key.length>2048)return false;
    try{sessionStorage.setItem(SELECTION_KEY,key);return true;}catch{return false;}
}
export function takeSelection(){
    try{const key=sessionStorage.getItem(SELECTION_KEY);sessionStorage.removeItem(SELECTION_KEY);
        return typeof key==="string"&&key.length<=2048?key:"";}catch{return "";}
}
