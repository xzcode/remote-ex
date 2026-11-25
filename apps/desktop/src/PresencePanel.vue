<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref, watch } from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";
import type { TextKey } from "./i18n";
import TrustPanel from "./TrustPanel.vue";
type Stage = "offline" | "connecting" | "online" | "reconnecting" | "stopping" | "failed";
type DeviceStatus={device_id:string;online:boolean;busy:boolean};
type Snapshot = {devices?:DeviceStatus[];checked_at?:number|null;stage:Stage; active:boolean; device_id:string|null; public_key:string|null; retries:number; persistent:boolean; heartbeats:number; declined_requests:number; detail:string|null};
const props = defineProps<{t:(key:TextKey)=>string}>();
const emit = defineEmits<{active:[value:boolean]}>();
const development = ref(false);
const url = ref("");
const serverId = ref("");
const persistent = ref(false);
const reconnect = ref(false);
const enrollment=ref<{device_id:string;public_key:string}|null>(null);
const snapshot = ref<Snapshot>({stage:"offline",active:false,device_id:null,public_key:null,retries:0,persistent:false,heartbeats:0,declined_requests:0,detail:null});
const busy = ref(false);
const error = ref("");
const native = isTauri();
let revision=0;
let disposed=false;
let polling=false;
let timer:number|undefined;
const stages:Record<Stage,TextKey>={offline:"presenceOffline",connecting:"presenceConnecting",online:"presenceOnline",reconnecting:"presenceReconnecting",stopping:"presenceStopping",failed:"presenceFailed"};
const status=computed(()=>props.t(stages[snapshot.value.stage]));
const canStart=computed(()=>native&&!busy.value&&!snapshot.value.active&&url.value.trim().length>0
  &&(development.value||/^[a-fA-F0-9]{64}$/.test(serverId.value.trim())));
watch(development, value=>{
  persistent.value=false;
  serverId.value="";
  url.value=value?"ws://127.0.0.1:47822/v1/ws":"";
});
watch(()=>busy.value||snapshot.value.active,value=>emit("active",value),{immediate:true});
async function poll(){
  if(!native||disposed||polling||busy.value)return;
  polling=true;
  const current=revision;
  try{
    const data=await invoke<Snapshot>("presence_status");
    if(!disposed&&current===revision)snapshot.value=data;
  }catch(e){if(!disposed)error.value=String(e);}
  finally{polling=false;}
}
async function action(name:string,args:Record<string,unknown>={}){
  if(!native||busy.value)return;
  busy.value=true;revision++;error.value="";
  try{snapshot.value=await invoke<Snapshot>(name,args);}
  catch(e){error.value=String(e);}
  finally{busy.value=false;}
}
function start(){
  return action("presence_start",{
    url:url.value.trim(),serverId:development.value?"":serverId.value.trim(),
    development:development.value,persistent:persistent.value,storageConsent:persistent.value,reconnect:reconnect.value
  });
}
async function prepareIdentity(){
  if(!native||busy.value||snapshot.value.active||!persistent.value)return;
  busy.value=true;error.value="";
  try{enrollment.value=await invoke("persistent_identity_info",{confirmed:true});}
  catch(e){error.value=String(e);}finally{busy.value=false;}
}
onMounted(()=>{void poll();timer=window.setInterval(poll,400);});
onBeforeUnmount(()=>{
  disposed=true;if(timer!==undefined)window.clearInterval(timer);
  if(native&&snapshot.value.active)void invoke("presence_stop").catch(()=>{});
});
</script>

<template>
  <section class="panel lan-panel">
    <header class="preview-head"><h2>{{ t("presenceTitle") }}</h2><span class="status" role="status">{{ status }}</span></header>
    <p>{{ t("presenceDescription") }}</p>
    <div class="field">
      <label for="presence-mode">{{ t("presenceMode") }}</label>
      <select id="presence-mode" v-model="development" :disabled="busy||snapshot.active">
        <option :value="false">{{ t("presenceSecure") }}</option>
        <option :value="true">{{ t("presenceDevelopment") }}</option>
      </select>
      <label for="presence-url">{{ t("presenceUrl") }}</label>
      <input id="presence-url" v-model="url" maxlength="512" spellcheck="false" autocomplete="off" placeholder="wss://…/v1/ws" :disabled="busy||snapshot.active"/>
      <template v-if="!development">
        <label for="presence-pin">{{ t("presenceServerId") }}</label>
        <input id="presence-pin" v-model="serverId" maxlength="64" spellcheck="false" autocomplete="off" class="key-input" :disabled="busy||snapshot.active"/>
        <p class="muted">{{ t("presencePinHelp") }}</p>
        <label class="check-label">
          <input type="checkbox" v-model="persistent" :disabled="busy||snapshot.active"/>
          {{ t("presenceStorageConsent") }}
        </label>
        <button v-if="persistent" :disabled="busy||snapshot.active" @click="prepareIdentity">{{ t("identityPrepare") }}</button>
      </template>
      <p v-else class="muted">{{ t("presenceLoopbackNotice") }}</p>
      <label class="check-label"><input type="checkbox" v-model="reconnect" :disabled="busy||snapshot.active"/>{{ t("presenceRetry") }}</label>
      <div class="actions">
        <button class="primary" :disabled="!canStart" @click="start">{{ t("presenceRegister") }}</button>
        <button :disabled="busy||!snapshot.active||snapshot.stage==='stopping'" @click="action('presence_stop')">{{ t("presenceDisconnect") }}</button>
      </div>
    </div>
    <div v-if="enrollment" class="field">
      <label>{{ t("identityEnrollment") }}</label><input readonly :value="enrollment.device_id" class="key-input"/>
      <label>{{ t("trustPublicKey") }}</label><input readonly :value="enrollment.public_key" class="key-input"/>
    </div>
    <div v-if="snapshot.device_id" class="field">
      <label for="presence-device-id">{{ t("presenceDeviceId") }}</label>
      <input id="presence-device-id" readonly :value="snapshot.device_id" class="key-input"/>
      <label v-if="snapshot.public_key">{{ t("trustPublicKey") }}</label><input v-if="snapshot.public_key" readonly :value="snapshot.public_key" class="key-input"/>
      <p v-if="snapshot.retries" class="muted">{{ t("presenceRetries") }}: {{ snapshot.retries }}</p>
      <p class="muted">{{ t(snapshot.persistent?"presencePersistent":"presenceTemporary") }}</p>
      <div class="presence-metrics">
        <div><span class="muted">{{ t("presenceHeartbeats") }}</span><strong>{{ snapshot.heartbeats }}</strong></div>
        <div><span class="muted">{{ t("presenceDeclined") }}</span><strong>{{ snapshot.declined_requests }}</strong></div>
      </div>
    </div>
    <p v-if="error||snapshot.detail" class="error" role="alert">{{ t("error") }}: {{ error||snapshot.detail }}</p>
    <p v-if="!native" class="muted">{{ t("browser") }}</p>
  </section>
  <TrustPanel v-if="!development && /^[a-fA-F0-9]{64}$/.test(serverId.trim())" :t="t" :server-id="serverId.trim()" :disabled="busy||snapshot.active" :online="snapshot.stage==='online'" :statuses="snapshot.devices??[]" :checked-at="snapshot.checked_at??null" />
</template>
