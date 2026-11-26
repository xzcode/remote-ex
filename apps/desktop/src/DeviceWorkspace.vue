<script setup lang="ts">
import {computed,onMounted,onBeforeUnmount,ref} from "vue";
import {invoke,isTauri} from "@tauri-apps/api/core";
import type {TextKey} from "./i18n";
import TrustPanel from "./TrustPanel.vue";
type Profile={label:string;url:string;server_id:string};
const props=defineProps<{t:(key:TextKey)=>string;section:"devices"|"history"}>();
const emit=defineEmits<{active:[value:boolean];server:[]}>();
const native=isTauri(),profiles=ref<Profile[]>([]),selection=ref(""),scope=ref(""),draft=ref(""),busy=ref(false),editing=ref(false),error=ref("");
const locked=computed(()=>busy.value||editing.value);
let disposed=false;
function valid(s:string){return /^[a-fA-F0-9]{64}$/.test(s)&&!/^0+$/.test(s);}
function active(value:boolean){editing.value=value;emit("active",locked.value);}
async function refresh(){
 if(!native||locked.value)return;
 busy.value=true;emit("active",true);error.value="";
 try{const rows=await invoke<Profile[]>("server_profiles_list");if(!disposed)profiles.value=rows;}
 catch(e){if(!disposed)error.value=String(e);}
 finally{busy.value=false;if(!disposed)emit("active",locked.value);}
}
function choose(){
 if(locked.value)return;
 const p=profiles.value.find(p=>p.server_id===selection.value);
 if(p){draft.value=p.server_id;scope.value=p.server_id;}
}
function apply(){if(!locked.value&&valid(draft.value.trim())){scope.value=draft.value.trim().toLowerCase();selection.value=profiles.value.some(p=>p.server_id===scope.value)?scope.value:"";}}
onMounted(()=>void refresh());onBeforeUnmount(()=>{disposed=true;});
</script>
<template>
 <section class="panel lan-panel scope-panel">
  <div class="preview-head"><h2>{{ t("serverProfilesChoose") }}</h2><button :disabled="!native||locked" @click="refresh">{{ t("refresh") }}</button></div>
  <div class="actions"><select v-model="selection" :disabled="locked||!native" @change="choose"><option value="">{{ t("serverProfilesNone") }}</option><option v-for="p in profiles" :key="p.server_id" :value="p.server_id">{{ p.label }}</option></select></div>
  <details class="server-profiles" :open="!profiles.length||undefined">
   <summary>{{ t("presenceServerId") }}</summary>
   <div class="field"><label for="workspace-server-id">{{ t("presenceServerId") }}</label><div class="actions"><input id="workspace-server-id" v-model="draft" maxlength="64" class="key-input" :disabled="locked||!native"/><button :disabled="locked||!native||!valid(draft.trim())" @click="apply">{{ t("serverProfilesApply") }}</button></div></div>
  </details>
  <p v-if="scope" class="muted profile-id">{{ scope }}</p>
  <p v-else class="muted">{{ t("workspaceScopeNotice") }}</p>
  <p v-if="error" class="error" role="alert">{{ error }}</p>
  <button v-if="!scope" :disabled="locked" @click="emit('server')">{{ t("server") }}</button>
 </section>
 <TrustPanel v-if="scope" :key="scope+section" :t="t" :server-id="scope" :disabled="busy" :section="section" @active="active"/>
</template>
