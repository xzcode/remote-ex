<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";
import type { TextKey } from "./i18n";
const props=defineProps<{t:(key:TextKey)=>string}>();
type Caps={os:string;desktop_session:string;view_candidate:boolean;input_candidate:boolean;armed_candidate:boolean;sidecar_present:boolean;login_launch:boolean;tray_ready:boolean;wayland_portal:boolean;remote_power:boolean;virtual_display:boolean;privacy_display:boolean};
const caps=ref<Caps|null>(null),startup=ref<boolean|null>(null),busy=ref(false),error=ref(""),result=ref(""),mac=ref(""),startConfirm=ref(false),wakeConfirm=ref(false);
const items=computed<{key:TextKey,value:boolean}[]>(()=>{const c=caps.value;if(!c)return [];return [{key:"serviceView",value:c.view_candidate},{key:"serviceInput",value:c.input_candidate},{key:"armedHost",value:c.armed_candidate},{key:"servicePortal",value:c.wayland_portal},{key:"servicePower",value:c.remote_power},{key:"serviceVirtual",value:c.virtual_display},{key:"servicePrivacy",value:c.privacy_display}];});
async function task(fn:()=>Promise<void>){if(busy.value||!isTauri())return;busy.value=true;error.value="";result.value="";try{await fn();}catch(e){error.value=String(e);}finally{busy.value=false;}}
function refresh(){return task(async()=>{caps.value=await invoke<Caps>("platform_capabilities");startup.value=await invoke<boolean>("startup_status");});}
function changeStartup(){if(startup.value===null||!startConfirm.value)return;return task(async()=>{startup.value=await invoke<boolean>("startup_change",{enabled:!startup.value,confirmed:true});startConfirm.value=false;result.value=props.t("serviceUpdated");});}
function stop(){return task(async()=>{await invoke("stop_all");result.value=props.t("serviceStopRequested");});}
function wake(){if(!wakeConfirm.value)return;return task(async()=>{await invoke("wake_lan",{mac:mac.value,confirmed:true});wakeConfirm.value=false;result.value=props.t("wakeSent");});}
onMounted(()=>{if(isTauri())void refresh();});
</script>
<template>
  <section class="panel">
    <header class="preview-head"><h2>{{ t("serviceTitle") }}</h2><button :disabled="busy||!isTauri()" @click="refresh">{{ t("refresh") }}</button></header>
    <p>{{ t("serviceNotice") }}</p>
    <div class="field">
      <label>{{ t("startupTitle") }}</label>
      <span class="muted">{{ startup===null?t("permissionUnknown"):startup?t("startupEnabled"):t("startupDisabled") }}</span>
      <p class="muted">{{ t("startupNotice") }}</p>
      <label class="check-label"><input type="checkbox" v-model="startConfirm" :disabled="busy||startup===null"/>{{ t("startupConfirm") }}</label>
      <button :disabled="busy||startup===null||!startConfirm" @click="changeStartup">{{ t(startup?"startupDisable":"startupEnable") }}</button>
    </div>
    <div class="field"><label>{{ t("trayTitle") }}</label><span>{{ caps?.tray_ready?t("serviceAvailable"):t("serviceUnsupported") }}</span><p class="muted">{{ t("trayNotice") }}</p>
      <button :disabled="busy||!isTauri()" @click="stop">{{ t("serviceStopAll") }}</button></div>
  </section>
  <section class="panel">
    <h2>{{ t("platformTitle") }}</h2>
    <template v-if="caps">
      <p>{{ caps.os }} / {{ caps.desktop_session }}</p>
      <div v-for="item in items" :key="item.key" class="preference-row"><label>{{ t(item.key) }}</label><span class="muted">{{ t(item.value?"serviceCandidate":"serviceUnsupported") }}</span></div>
      <p class="muted">{{ t("sidecarTitle") }}: {{ caps.sidecar_present?t("serviceAvailable"):t("serviceUnsupported") }}</p>
    </template>
    <p class="muted">{{ t("platformNotice") }}</p>
  </section>
  <section class="panel">
    <h2>{{ t("wakeTitle") }}</h2><p>{{ t("wakeNotice") }}</p>
    <div class="field"><label for="wake-mac">MAC</label><input id="wake-mac" v-model="mac" maxlength="17" placeholder="02:12:34:56:78:9A" :disabled="busy"/>
      <label class="check-label"><input type="checkbox" v-model="wakeConfirm" :disabled="busy"/>{{ t("wakeConfirm") }}</label>
      <button :disabled="busy||!wakeConfirm||!mac.trim()||!isTauri()" @click="wake">{{ t("wakeSend") }}</button>
    </div>
    <p v-if="error" role="alert" class="error">{{ error }}</p><p v-if="result" role="status" class="muted">{{ result }}</p>
  </section>
</template>
