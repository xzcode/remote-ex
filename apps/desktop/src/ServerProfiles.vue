<script setup lang="ts">
import {ref,computed,watch,onMounted,onBeforeUnmount} from "vue";
import {invoke,isTauri} from "@tauri-apps/api/core";
import type {TextKey} from "./i18n";
type Profile={label:string;url:string;server_id:string};
const props=defineProps<{t:(key:TextKey)=>string;disabled:boolean;url:string;serverId:string}>();
const emit=defineEmits<{apply:[profile:Profile]}>();
const native=isTauri(),rows=ref<Profile[]>([]),selected=ref(""),label=ref(""),confirmSave=ref(false),confirmRemove=ref(false),busy=ref(false),error=ref("");
let disposed=false;
const chosen=computed(()=>rows.value.find(p=>p.server_id===selected.value));
const canSave=computed(()=>native&&!props.disabled&&!busy.value&&confirmSave.value&&label.value.trim().length>0&&props.url.trim().startsWith("wss://")&&/^[a-fA-F0-9]{64}$/.test(props.serverId.trim()));
watch([()=>props.url,()=>props.serverId,label],()=>{confirmSave.value=false;});
watch(selected,()=>{confirmRemove.value=false;});
async function task(fn:()=>Promise<Profile[]>){
 if(!native||busy.value||props.disabled)return;busy.value=true;error.value="";
 try{const result=await fn();if(!disposed){rows.value=result;if(!result.some(p=>p.server_id===selected.value))selected.value="";}}
 catch(e){if(!disposed)error.value=String(e);}
 finally{if(!disposed)busy.value=false;}
}
function refresh(){return task(()=>invoke<Profile[]>("server_profiles_list"));}
function save(){
 if(!canSave.value)return;
 const p={label:label.value.trim(),url:props.url.trim(),server_id:props.serverId.trim().toLowerCase()};
 return task(async()=>{const result=await invoke<Profile[]>("server_profile_save",{profile:p,confirmed:true});if(!disposed){label.value="";confirmSave.value=false;selected.value=p.server_id;}return result;});
}
function apply(){if(!props.disabled&&!busy.value&&chosen.value)emit("apply",chosen.value);}
function remove(){const p=chosen.value;if(!p||!confirmRemove.value||props.disabled||busy.value)return;return task(()=>invoke<Profile[]>("server_profile_remove",{serverId:p.server_id,confirmed:true}));}
onMounted(()=>void refresh());onBeforeUnmount(()=>{disposed=true;});
</script>
<template>
 <details class="server-profiles">
  <summary>{{ t("serverProfilesTitle") }}</summary>
  <div class="field">
   <label>{{ t("serverProfilesChoose") }}</label>
   <div class="actions"><select v-model="selected" :disabled="disabled||busy||!native"><option value="">{{ t("serverProfilesNone") }}</option><option v-for="p in rows" :key="p.server_id" :value="p.server_id">{{ p.label }} · {{ p.url }}</option></select><button :disabled="disabled||busy||!native" @click="refresh">{{ t("serverProfilesRefresh") }}</button><button :disabled="disabled||busy||!chosen" @click="apply">{{ t("serverProfilesApply") }}</button></div>
   <p v-if="chosen" class="muted profile-id">{{ chosen.server_id }}</p>
   <label v-if="chosen" class="check-label"><input type="checkbox" v-model="confirmRemove" :disabled="disabled||busy"/>{{ t("serverProfilesRemoveConfirm") }}</label>
   <button v-if="chosen" :disabled="disabled||busy||!confirmRemove" @click="remove">{{ t("serverProfilesRemove") }}</button>
   <label>{{ t("serverProfilesName") }}</label><input v-model="label" maxlength="64" :disabled="disabled||busy" />
   <p class="muted">{{ t("serverProfilesNotice") }}</p>
   <label class="check-label"><input type="checkbox" v-model="confirmSave" :disabled="disabled||busy"/>{{ t("serverProfilesConfirm") }}</label>
   <button :disabled="!canSave" @click="save">{{ t("serverProfilesSave") }}</button>
   <p v-if="error" class="error" role="alert">{{ error }}</p>
  </div>
 </details>
</template>
