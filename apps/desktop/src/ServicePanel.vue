<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref, watch } from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";
import type { TextKey } from "./i18n";
const props=defineProps<{t:(key:TextKey)=>string}>();
const emit=defineEmits<{active:[boolean]}>();
type Policy={server_id:string;device_id:string;peer_key:string;host_key:string;url:string;display_id:number;display_name:string;display_width:number;display_height:number;mode:"view"|"control";quality:string;relay_only:boolean;extended:boolean;expires_at:number};
type Device={device_id:string;public_key:string;label:string};
type Display={id:number;name:string;width:number;height:number;primary:boolean};
type Session={stage:string;active:boolean;privacy_available?:boolean;privacy_paused?:boolean;privacy_pending?:boolean|null;saved_policy?:boolean;service_running?:boolean;armed_wait?:number;service_limit?:number;service_completed?:number;service_expires_at?:number|null;local_key:string|null;device_id:string|null;detail:string|null;input_released:boolean};
const policies=ref<Policy[]>([]),devices=ref<Device[]>([]),displays=ref<Display[]>([]);
const policyServer=ref(""),policyUrl=ref(""),policyDevice=ref(""),policyDisplay=ref<number|null>(null),policyMode=ref<"view"|"control">("view"),policyQuality=ref("compat"),policyRelay=ref(false),policyExtended=ref(false),policyDays=ref(7),policyConfirm=ref(false);
const sessionLimit=ref(1);
const selectedPolicy=ref<Policy|null>(null),activateConfirm=ref(false),revokeConfirm=ref(false),session=ref<Session|null>(null);
const ownedActive=computed(()=>!!session.value?.saved_policy&&(session.value.active||!!session.value.service_running));
const expired=(p:Policy)=>p.expires_at<=Math.floor(Date.now()/1000);
const dateLabel=(n:number)=>new Date(n*1000).toLocaleString();
let pollTimer:number|undefined,polling=false,disposed=false;
watch(ownedActive,v=>emit("active",v),{immediate:true});
watch([policyServer,policyUrl,policyDevice,policyDisplay,policyMode,policyQuality,policyRelay,policyExtended,policyDays],()=>{policyConfirm.value=false;});
watch(policyServer,()=>{devices.value=[];policyDevice.value="";});
watch(policyMode,()=>{policyDisplay.value=null;});
watch(selectedPolicy,()=>{activateConfirm.value=false;revokeConfirm.value=false;});
watch(sessionLimit,()=>{activateConfirm.value=false;});
async function pollSession(){if(!isTauri()||disposed||polling||busy.value)return;polling=true;try{const s=await invoke<Session>("routed_status",{slot:0});if(!disposed)session.value=s;}catch(e){if(!disposed)error.value=String(e);}finally{polling=false;}}
async function reloadPolicies(){policies.value=await invoke<Policy[]>("host_policy_list");const selected=selectedPolicy.value;selectedPolicy.value=selected?policies.value.find(p=>p.server_id===selected.server_id&&p.device_id===selected.device_id)??null:null;}
function refreshPolicies(){return task(reloadPolicies);}
function loadPolicyDevices(){return task(async()=>{devices.value=await invoke<Device[]>("trusted_list",{serverId:policyServer.value.trim()});policyDevice.value="";});}
function loadPolicyDisplays(){return task(async()=>{displays.value=await invoke<Display[]>("list_displays");policyDisplay.value=null;});}
const allowedDisplays=computed(()=>displays.value.filter(d=>policyMode.value==="view"||d.primary));
const canSave=computed(()=>!busy.value&&!ownedActive.value&&!!caps.value?.armed_candidate&&policyConfirm.value&&/^wss:\/\//.test(policyUrl.value)&&/^[a-fA-F0-9]{64}$/.test(policyServer.value.trim())&&devices.value.some(d=>d.device_id===policyDevice.value)&&allowedDisplays.value.some(d=>d.id===policyDisplay.value));
function savePolicy(){if(!canSave.value)return;return task(async()=>{policies.value=await invoke<Policy[]>("host_policy_save",{draft:{server_id:policyServer.value.trim(),device_id:policyDevice.value,url:policyUrl.value.trim(),display_id:policyDisplay.value,mode:policyMode.value,quality:policyQuality.value,relay_only:policyRelay.value,extended:policyExtended.value,days:policyDays.value},confirmed:true});policyConfirm.value=false;result.value=props.t("policySaved");});}
function activatePolicy(){const p=selectedPolicy.value;if(!p||expired(p)||!activateConfirm.value||ownedActive.value)return;return task(async()=>{session.value=await invoke<Session>("host_policy_activate",{serverId:p.server_id,deviceId:p.device_id,confirmed:true,sessions:sessionLimit.value});activateConfirm.value=false;});}
function revokePolicy(){const p=selectedPolicy.value;if(!p||!revokeConfirm.value)return;return task(async()=>{policies.value=await invoke<Policy[]>("host_policy_revoke",{serverId:p.server_id,deviceId:p.device_id,confirmed:true});selectedPolicy.value=null;result.value=props.t("policyRevoked");});}

type Caps={os:string;desktop_session:string;view_candidate:boolean;input_candidate:boolean;armed_candidate:boolean;sidecar_present:boolean;login_launch:boolean;tray_ready:boolean;wayland_portal:boolean;remote_power:boolean;virtual_display:boolean;privacy_display:boolean;portal_sources:string[];portal_version:number|null;portal_detail:string|null;portal_control:boolean};
const caps=ref<Caps|null>(null),startup=ref<boolean|null>(null),busy=ref(false),error=ref(""),result=ref(""),mac=ref(""),startConfirm=ref(false),wakeConfirm=ref(false);
const items=computed<{key:TextKey,value:boolean}[]>(()=>{const c=caps.value;if(!c)return [];return [{key:"serviceView",value:c.view_candidate},{key:"serviceInput",value:c.input_candidate},{key:"armedHost",value:c.armed_candidate},{key:"servicePortal",value:c.wayland_portal},{key:"servicePower",value:c.remote_power},{key:"serviceVirtual",value:c.virtual_display},{key:"servicePrivacy",value:c.privacy_display}];});
async function task(fn:()=>Promise<void>){if(busy.value||!isTauri())return;busy.value=true;error.value="";result.value="";try{await fn();}catch(e){error.value=String(e);}finally{busy.value=false;}}
function refresh(){return task(async()=>{caps.value=await invoke<Caps>("platform_capabilities");startup.value=await invoke<boolean>("startup_status");});}
function changeStartup(){if(startup.value===null||!startConfirm.value)return;return task(async()=>{startup.value=await invoke<boolean>("startup_change",{enabled:!startup.value,confirmed:true});startConfirm.value=false;result.value=props.t("serviceUpdated");});}
function privacyChange(){if(!session.value?.privacy_available||session.value.privacy_pending!=null)return;return task(async()=>{session.value=await invoke<Session>("routed_privacy_change",{slot:0,paused:!session.value?.privacy_paused,confirmed:true});});}
function stop(){return task(async()=>{await invoke("stop_all");result.value=props.t("serviceStopRequested");});}
function wake(){if(!wakeConfirm.value)return;return task(async()=>{await invoke("wake_lan",{mac:mac.value,confirmed:true});wakeConfirm.value=false;result.value=props.t("wakeSent");});}
onMounted(()=>{if(isTauri()){void task(async()=>{caps.value=await invoke<Caps>("platform_capabilities");await reloadPolicies();startup.value=await invoke<boolean>("startup_status");});pollTimer=window.setInterval(pollSession,400);}});
onBeforeUnmount(()=>{disposed=true;if(pollTimer!==undefined)window.clearInterval(pollTimer);if(ownedActive.value)void invoke("routed_disconnect",{slot:0}).catch(()=>{});});
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
      <p v-if="caps.desktop_session==='wayland'" class="muted">{{ t("portalSource") }}: {{ caps.portal_sources.map(s=>t(s==="monitor"?"portalMonitor":s==="window"?"portalWindow":"portalVirtual")).join(" / ")||t("serviceUnsupported") }} <span v-if="caps.portal_version!==null"> · Portal v{{ caps.portal_version }}</span></p>
      <p v-if="caps.desktop_session==='wayland'" class="muted">{{ t("portalInputCapability") }}: {{ caps.portal_control?t("serviceCandidate"):t("serviceUnsupported") }}</p>
      <p v-if="caps.portal_detail" class="muted">{{ caps.portal_detail }}</p>
      <p class="muted">{{ t("sidecarTitle") }}: {{ caps.sidecar_present?t("serviceAvailable"):t("serviceUnsupported") }}</p>
    </template>
    <p class="muted">{{ t("platformNotice") }}</p>
  </section>
  <section class="panel">
    <header class="preview-head"><h2>{{ t("policyTitle") }}</h2><button :disabled="busy||!isTauri()" @click="refreshPolicies">{{ t("refresh") }}</button></header>
    <p>{{ t("policyNotice") }}</p>
    <p v-if="!caps?.armed_candidate" class="muted">{{ t("policyUnsupported") }}</p>
    <template v-if="caps?.armed_candidate">
      <div class="field">
        <label>{{ t("policyUrl") }}</label><input v-model="policyUrl" placeholder="wss://…/v1/ws" maxlength="512" :disabled="busy||ownedActive"/>
        <label>{{ t("presenceServerId") }}</label><input v-model="policyServer" maxlength="64" class="key-input" :disabled="busy||ownedActive"/>
        <label>{{ t("policyPeer") }}</label><div class="actions"><select v-model="policyDevice" :disabled="busy||ownedActive"><option value="">{{ t("select") }}</option><option v-for="d in devices" :key="d.device_id" :value="d.device_id">{{ d.label }} · {{ d.device_id.slice(0,16) }}</option></select><button :disabled="busy||ownedActive||policyServer.trim().length!==64" @click="loadPolicyDevices">{{ t("refresh") }}</button></div>
        <label>{{ t("policyMode") }}</label><select v-model="policyMode" :disabled="busy||ownedActive"><option value="view">{{ t("serviceView") }}</option><option value="control">{{ t("serviceInput") }}</option></select>
        <label>{{ t("display") }}</label><div class="actions"><select v-model="policyDisplay" :disabled="busy||ownedActive"><option :value="null">{{ t("select") }}</option><option v-for="d in allowedDisplays" :key="d.id" :value="d.id">{{ d.name }} · {{ d.width }} × {{ d.height }}</option></select><button :disabled="busy||ownedActive" @click="loadPolicyDisplays">{{ t("refresh") }}</button></div>
        <label>{{ t("qualityPreset") }}</label><select v-model="policyQuality" :disabled="busy||ownedActive"><option value="compat">540p · 10fps</option><option value="low">480p · 15fps</option><option value="balanced">720p</option><option value="high">1080p</option><option value="qhd">1440p</option><option value="uhd">2160p</option></select>
        <label>{{ t("policyDays") }}</label><select v-model.number="policyDays" :disabled="busy||ownedActive"><option :value="1">1</option><option :value="7">7</option><option :value="30">30</option></select>
        <label class="check-label"><input type="checkbox" v-model="policyRelay" :disabled="busy||ownedActive"/>{{ t("routedRelay") }}</label>
        <label class="check-label"><input type="checkbox" v-model="policyExtended" :disabled="busy||ownedActive"/>{{ t("extendedSession") }}</label>
        <label class="check-label"><input type="checkbox" v-model="policyConfirm" :disabled="busy||ownedActive"/>{{ t("policyConfirm") }}</label>
        <button :disabled="!canSave" @click="savePolicy">{{ t("policySave") }}</button>
      </div>
    </template>
    <div v-for="p in policies" :key="p.server_id+p.device_id" class="field">
      <label class="check-label"><input type="radio" name="saved-policy" :checked="selectedPolicy?.server_id===p.server_id&&selectedPolicy?.device_id===p.device_id" @change="selectedPolicy=p"/>{{ p.device_id.slice(0,16) }}… · {{ t(p.mode==="view"?"serviceView":"serviceInput") }}</label>
      <p class="muted">{{ p.url }} · {{ p.display_name }} {{ p.display_width }} × {{ p.display_height }} · {{ p.quality }} · {{ t("policyExpires") }}: {{ dateLabel(p.expires_at) }} {{ expired(p)?t("policyExpired"):"" }}</p>
      <p class="muted">{{ t("lanLocalKey") }}: {{ p.host_key }} · {{ t("lanPeerKey") }}: {{ p.peer_key }}</p>
    </div>
    <template v-if="selectedPolicy">
      <label>{{ t("serviceRunLimit") }}</label><select v-model.number="sessionLimit" :disabled="busy||ownedActive"><option :value="1">{{ t("serviceOneSession") }}</option><option :value="3">{{ t("serviceThreeSessions") }}</option><option :value="12">{{ t("serviceTwelveSessions") }}</option></select>
      <p v-if="sessionLimit>1" class="muted">{{ t("serviceRunNotice") }}</p><p v-if="sessionLimit===12" class="muted">{{ t("serviceLongNotice") }}</p>
      <label class="check-label"><input type="checkbox" v-model="activateConfirm" :disabled="busy||ownedActive||expired(selectedPolicy)"/>{{ t(sessionLimit===12?"serviceLongConfirm":sessionLimit===3?"serviceRunConfirm":"policyActivateConfirm") }}</label>
      <button :disabled="busy||ownedActive||!activateConfirm||expired(selectedPolicy)||!caps?.armed_candidate" @click="activatePolicy">{{ t("policyActivate") }}</button>
      <label class="check-label"><input type="checkbox" v-model="revokeConfirm" :disabled="busy"/>{{ t("policyRevokeConfirm") }}</label>
      <button :disabled="busy||!revokeConfirm" @click="revokePolicy">{{ t("policyRevoke") }}</button>
    </template>
    <div v-if="session?.saved_policy" class="field">
      <p>{{ t("policyState") }}: {{ session.stage==="listening"?t("armedWaiting"):session.stage }}</p>
      <p v-if="(session.service_limit??0)>1" class="muted">{{ t("serviceCompleted") }}: {{ session.service_completed??0 }}/{{ session.service_limit }} · {{ t("policyExpires") }}: {{ session.service_expires_at?dateLabel(session.service_expires_at):"—" }} · {{ session.service_running?t("serviceRunning"):t("serviceEnded") }}</p>
      <p v-if="session.device_id">{{ t("presenceDeviceId") }}: {{ session.device_id }}</p>
      <p v-if="session.detail" role="alert" class="error">{{ session.detail }}</p>
      <p v-if="session.input_released" class="muted">{{ t("policyReleased") }}</p>
      <template v-if="session.stage==='connected'&&session.privacy_available"><p class="muted">{{ t(session.privacy_pending!=null?"privacyPending":session.privacy_paused?"privacyPaused":"privacyPauseNotice") }}</p><button :disabled="busy||session.privacy_pending!=null" @click="privacyChange">{{ t(session.privacy_paused?"privacyResume":"privacyPause") }}</button></template>
      <button :disabled="busy||!ownedActive" @click="stop">{{ t("serviceStopAll") }}</button>
    </div>
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
