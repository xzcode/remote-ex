<script setup lang="ts">
import {ref,watch} from "vue";
import {invoke,isTauri} from "@tauri-apps/api/core";
import type {TextKey} from "./i18n";
type Device={device_id:string;public_key:string;label:string};
const props=defineProps<{t:(key:TextKey)=>string;serverId:string;disabled:boolean}>();
const records=ref<Device[]>([]);const label=ref("");const id=ref("");const key=ref("");
const verified=ref(false);const removeId=ref("");const busy=ref(false);const error=ref("");
let revision=0;
async function load(){
 const stamp=++revision;records.value=[];error.value="";if(!isTauri())return;
 try{const data=await invoke<Device[]>("trusted_list",{serverId:props.serverId});if(stamp===revision)records.value=data;}
 catch(e){if(stamp===revision)error.value=String(e);}
}
watch(()=>props.serverId,()=>{verified.value=false;removeId.value="";void load();},{immediate:true});
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
 <div v-for="d in records" :key="d.device_id" class="field">
  <div class="preview-head"><strong>{{ d.label }}</strong><button :disabled="disabled||busy" @click="removeId=d.device_id">{{ t("trustRemove") }}</button></div>
  <input readonly :value="d.device_id" class="key-input"/><input readonly :value="d.public_key" class="key-input"/>
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
 <p v-if="error" class="error" role="alert">{{ error }}</p>
</section>
</template>
