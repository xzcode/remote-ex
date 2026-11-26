<script setup lang="ts">
import {computed,ref,watch,onBeforeUnmount} from "vue";
import {invoke,isTauri} from "@tauri-apps/api/core";
import type {TextKey} from "./i18n";
const props=defineProps<{t:(key:TextKey)=>string;url:string;serverId:string;caPath?:string;disabled:boolean}>();
const emit=defineEmits<{active:[boolean]}>();
const native=isTauri(),running=ref(false),report=ref<{verified:boolean;elapsed_ms:number;custom_ca:boolean}|null>(null),error=ref("");
let disposed=false,revision=0;
const ready=computed(()=>native&&!props.disabled&&!running.value&&props.url.trim().startsWith("wss://")&&/^[a-fA-F0-9]{64}$/.test(props.serverId.trim()));
watch([()=>props.url,()=>props.serverId,()=>props.caPath],()=>{revision++;report.value=null;error.value="";});
onBeforeUnmount(()=>{disposed=true;revision++;if(running.value)emit("active",false);});
async function check(){
 if(!ready.value)return;running.value=true;emit("active",true);report.value=null;error.value="";
 const r=revision;
 try{const value=await invoke<{verified:boolean;elapsed_ms:number;custom_ca:boolean}>("server_preflight",{url:props.url.trim(),serverId:props.serverId.trim(),caPath:props.caPath?.trim()||null,confirmed:true});if(!disposed&&r===revision)report.value=value;}
 catch(e){if(!disposed&&r===revision)error.value=String(e);}
 finally{if(!disposed){running.value=false;emit("active",false);}}
}
</script>
<template>
 <div class="field">
  <button :disabled="!ready" @click="check">{{ t(running?"busy":"serverCheck") }}</button>
  <p class="muted">{{ t("serverCheckNotice") }}</p>
  <p v-if="report?.verified" role="status">{{ t("serverCheckPassed") }} · {{ report.elapsed_ms }} ms</p>
  <p v-if="error" class="error" role="alert">{{ error }}</p>
 </div>
</template>
