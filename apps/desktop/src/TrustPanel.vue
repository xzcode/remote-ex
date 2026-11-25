<script setup lang="ts">
import {computed,ref,watch} from "vue";
import {invoke,isTauri} from "@tauri-apps/api/core";
import type {TextKey} from "./i18n";
type Device={device_id:string;public_key:string;label:string;group_name:string;note:string;favorite:boolean};
type History={device_id:string;mode:string;ended_at:number;outcome:string;duration_sec:number};
const props=defineProps<{t:(key:TextKey)=>string;serverId:string;disabled:boolean}>();
const records=ref<Device[]>([]);const label=ref("");const id=ref("");const key=ref("");
const verified=ref(false);const removeId=ref("");const busy=ref(false);const error=ref("");
const query=ref(""),group=ref(""),favorites=ref(false),history=ref<History[]>([]),confirmClear=ref(false);
const filtered=computed(()=>records.value.filter(d=>(!favorites.value||d.favorite)&&(!group.value||d.group_name===group.value)&&[d.label,d.device_id,d.group_name,d.note].some(s=>s.toLowerCase().includes(query.value.toLowerCase()))));
const groups=computed(()=>[...new Set(records.value.map(d=>d.group_name).filter(Boolean))].sort());
let revision=0;
async function loadHistory(){
 const scope=props.serverId;
 try{const result=await invoke<History[]>("connection_history",{serverId:scope});if(scope===props.serverId)history.value=result;}catch(e){error.value=String(e);}
}
async function saveMetadata(d:Device){
 if(busy.value||props.disabled)return;busy.value=true;
 try{records.value=await invoke("trusted_metadata",{serverId:props.serverId,deviceId:d.device_id,groupName:d.group_name,note:d.note,favorite:d.favorite});}catch(e){error.value=String(e);}finally{busy.value=false;}
}
async function clearHistory(){
 if(!confirmClear.value||busy.value||props.disabled)return;busy.value=true;
 try{await invoke("clear_connection_history",{serverId:props.serverId,confirmed:true});history.value=[];confirmClear.value=false;}catch(e){error.value=String(e);}finally{busy.value=false;}
}
async function load(){
 const stamp=++revision;records.value=[];error.value="";if(!isTauri())return;
 try{const data=await invoke<Device[]>("trusted_list",{serverId:props.serverId});if(stamp===revision)records.value=data;}
 catch(e){if(stamp===revision)error.value=String(e);}
}
watch(()=>props.serverId,()=>{verified.value=false;removeId.value="";void load();history.value=[];if(isTauri())void loadHistory();},{immediate:true});
watch([id,key,label],()=>{verified.value=false;});
async function add(){
 if(busy.value||props.disabled||!verified.value)return;busy.value=true;error.value="";
 try{records.value=await invoke("trusted_add",{serverId:props.serverId,deviceId:id.value.trim(),publicKey:key.value.trim(),label:label.value.trim(),confirmed:verified.value});label.value="";id.value="";key.value="";verified.value=false;}
 catch(e){error.value=String(e);}finally{busy.value=false;}
}
async function remove(){
 if(busy.value||props.disabled||!removeId.value)return;busy.value=true;error.value="";
 try{records.value=await invoke("trusted_remove",{serverId:props.serverId,deviceId:removeId.value,confirmed:true});removeId.value="";}
 catch(e){error.value=String(e);}finally{busy.value=false;}
}
</script>
<template>
<section class="panel lan-panel">
 <h2>{{ t("trustTitle") }}</h2><p>{{ t("trustDescription") }}</p>
 <div class="field"><input v-model="query" :placeholder="t('deviceSearch')"/><select v-model="group"><option value="">{{ t("deviceAllGroups") }}</option><option v-for="g in groups" :key="g">{{ g }}</option></select><label class="check-label"><input type="checkbox" v-model="favorites"/>{{ t("deviceFavorites") }}</label></div>
 <div v-for="d in filtered" :key="d.device_id" class="field">
  <div class="preview-head"><strong>{{ d.label }}</strong><button :disabled="disabled||busy" @click="removeId=d.device_id">{{ t("trustRemove") }}</button></div>
  <input readonly :value="d.device_id" class="key-input"/><input readonly :value="d.public_key" class="key-input"/>
  <label>{{ t("deviceGroup") }}</label><input v-model="d.group_name" maxlength="64" :disabled="disabled||busy"/><label>{{ t("deviceNote") }}</label><input v-model="d.note" maxlength="256" :disabled="disabled||busy"/><label class="check-label"><input type="checkbox" v-model="d.favorite" :disabled="disabled||busy"/>{{ t("deviceFavorites") }}</label><button :disabled="disabled||busy" @click="saveMetadata(d)">{{ t("deviceSaveMetadata") }}</button>
  <div v-if="removeId===d.device_id" class="actions"><button :disabled="disabled||busy" @click="remove">{{ t("trustConfirmRemove") }}</button><button @click="removeId=''">{{ t("trustCancel") }}</button></div>
 </div>
 <p v-if="!records.length" class="muted">{{ t("trustEmpty") }}</p>
 <div class="field">
  <label>{{ t("trustLabel") }}</label><input v-model="label" maxlength="64" :disabled="disabled||busy"/>
  <label>{{ t("presenceDeviceId") }}</label><input v-model="id" maxlength="64" class="key-input" :disabled="disabled||busy"/>
  <label>{{ t("trustPublicKey") }}</label><input v-model="key" maxlength="64" class="key-input" :disabled="disabled||busy"/>
  <label class="check-label"><input type="checkbox" v-model="verified" :disabled="disabled||busy"/>{{ t("trustConsent") }}</label>
  <button class="primary" :disabled="disabled||busy||!verified||!label.trim()||id.trim().length!==64||key.trim().length!==64" @click="add">{{ t("trustSave") }}</button>
 </div>
 <div class="field"><div class="preview-head"><strong>{{ t("deviceHistory") }}</strong><button :disabled="busy" @click="loadHistory">{{ t("refresh") }}</button></div><div v-for="(h,i) in history" :key="i" class="muted">{{ new Date(h.ended_at*1000).toLocaleString() }} · {{ h.device_id.slice(0,12) }}… · {{ h.mode }} · {{ h.outcome }} · {{ h.duration_sec }}s</div><label class="check-label"><input type="checkbox" v-model="confirmClear" :disabled="disabled||busy"/>{{ t("deviceClearHistory") }}</label><button :disabled="disabled||busy||!confirmClear" @click="clearHistory">{{ t("clear") }}</button></div>
 <p v-if="error" class="error" role="alert">{{ error }}</p>
</section>
</template>
