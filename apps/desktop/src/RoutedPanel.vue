<script setup lang="ts">
import {computed,ref,watch,onMounted,onBeforeUnmount,nextTick} from "vue";
import {invoke,isTauri} from "@tauri-apps/api/core";
import type {TextKey} from "./i18n";
type Stage="idle"|"starting"|"awaiting_approval"|"connecting"|"listening"|"connected"|"stopping"|"passed"|"failed"|"canceled";
type Snapshot={stage:Stage;active:boolean;local_key:string|null;device_id:string|null;detail:string|null;profile:string;input_released:boolean;decoded_fps?:number|null;decoded_bps?:number|null;transfer_done?:number|null;transfer_total?:number|null;transfer_verified?:boolean;transfer_paused?:boolean;audio_volume?:number|null;network_bps?:number|null;rtt_ms?:number|null;loss_pct?:number|null;route_type?:string|null};
type Device={device_id:string;public_key:string;label:string};
type Display={id:number;name:string;width:number;height:number;primary:boolean};
const props=defineProps<{t:(key:TextKey)=>string;slot:number}>();
const emit=defineEmits<{active:[value:boolean]}>();
const native=isTauri(),role=ref<"host"|"controller">("controller"),mode=ref<"test"|"view"|"control"|"file"|"text"|"png"|"audio">("test");
const sender=ref<"host"|"controller">("controller"),transferPath=ref(""),volume=ref(75);
const extraDisplays=ref<number[]>([]);
const isExchange=computed(()=>["file","text","png"].includes(mode.value));
const isDisplay=computed(()=>mode.value==="view"||mode.value==="control");
const url=ref(""),serverId=ref(""),ca=ref(""),relay=ref(false),persistent=ref(false);
const enrollment=ref<{device_id:string;public_key:string}|null>(null);
watch(persistent,value=>{if(!value)enrollment.value=null;});
const displays=ref<Display[]>([]),displayId=ref<number|null>(null),trusted=ref<Device[]>([]),chosen=ref(""),peerId=ref(""),peerKey=ref("");
const approved=ref(false),submitted=ref(false),busy=ref(false),error=ref("");
const saveHistory=ref(false),invitation=ref(""),quality=ref("compat");
type Invitation={url:string;server_id:string;device_id:string;public_key:string;mode:"test"|"view"|"control";expires_at:number};
const snapshot=ref<Snapshot>({stage:"idle",active:false,local_key:null,device_id:null,detail:null,profile:"routed_test",input_released:false});
let timer:number|undefined,disposed=false,polling=false,revision=0;
const statuses:Record<Stage,TextKey>={idle:"lanIdle",starting:"lanStarting",awaiting_approval:"lanAwaiting",connecting:"lanConnecting",listening:"lanListening",connected:"routedConnected",stopping:"controlStopping",passed:"controlEnded",failed:"lanFailed",canceled:"lanCanceled"};
const status=computed(()=>props.t(statuses[snapshot.value.stage]));
const validHex=(s:string)=>/^[a-fA-F0-9]{64}$/.test(s.trim());
const canPrepare=computed(()=>native&&!busy.value&&!snapshot.value.active&&url.value.trim().startsWith("wss://")&&validHex(serverId.value)&&(!isDisplay.value||role.value==="controller"||displayId.value!==null)&&(mode.value!=="file"||!!transferPath.value.trim()));
const canConfirm=computed(()=>!busy.value&&snapshot.value.stage==="awaiting_approval"&&!!snapshot.value.device_id&&validHex(peerId.value)&&validHex(peerKey.value)&&approved.value&&!submitted.value&&peerKey.value.trim().toLowerCase()!==snapshot.value.local_key);
watch([role,mode],()=>{displayId.value=null;extraDisplays.value=[];displays.value=[];approved.value=false;});
watch(displayId,()=>{extraDisplays.value=[];});
watch(serverId,()=>{trusted.value=[];chosen.value="";peerId.value="";peerKey.value="";approved.value=false;});
watch(chosen,id=>{if(id){peerId.value=id;peerKey.value=trusted.value.find(x=>x.device_id===id)?.public_key??"";}approved.value=false;});
watch([peerId,peerKey],()=>{approved.value=false;});
watch(()=>snapshot.value.stage,s=>{if(s!=="awaiting_approval"){approved.value=false;submitted.value=false;}});
watch(()=>busy.value||snapshot.value.active,v=>emit("active",v),{immediate:true});
async function poll(){
 if(!native||disposed||busy.value||polling)return;polling=true;const stamp=revision;
 try{const s=await invoke<Snapshot>("routed_status",{slot:props.slot});if(!disposed&&revision===stamp&&s.profile.startsWith("routed_"))snapshot.value=s;}
 catch(e){if(!disposed)error.value=String(e);}finally{polling=false;}
}
async function command(name:string,args:Record<string,unknown>={}){
 if(!native||busy.value)return;busy.value=true;revision++;error.value="";
 try{snapshot.value=await invoke<Snapshot>(name,{...args,slot:props.slot});}
 catch(e){error.value=String(e);submitted.value=false;}finally{busy.value=false;}
}
async function prepareSlotIdentity(){
 if(!native||busy.value||snapshot.value.active||!persistent.value)return;
 busy.value=true;error.value="";
 try{enrollment.value=await invoke("routed_identity_info",{slot:props.slot,confirmed:true});}
 catch(e){error.value=String(e);}finally{busy.value=false;}
}
async function chooseTransferFile(){
 if(!native||busy.value||snapshot.value.active||mode.value!=="file")return;
 busy.value=true;error.value="";
 try{const p=await invoke<string|null>("select_transfer_file",{receive:role.value!==sender.value});if(p)transferPath.value=p;}
 catch(e){error.value=String(e);}finally{busy.value=false;}
}
async function loadDisplays(){
 if(busy.value||snapshot.value.active)return;busy.value=true;error.value="";
 try{displays.value=(await invoke<Display[]>("list_displays")).filter(d=>mode.value==="view"||d.primary);displayId.value=null;extraDisplays.value=[];if(!displays.value.length)error.value=props.t("noDisplays");}
 catch(e){error.value=String(e);}finally{busy.value=false;}
}
async function loadTrusted(){
 if(!native||busy.value||!validHex(serverId.value))return;busy.value=true;error.value="";
 try{trusted.value=await invoke<Device[]>("trusted_list",{serverId:serverId.value.trim()});}
 catch(e){error.value=String(e);}finally{busy.value=false;}
}
async function readInvitation(){
 if(busy.value||snapshot.value.active)return;busy.value=true;error.value="";
 try{const i=await invoke<Invitation>("invitation_parse",{code:invitation.value.trim()});role.value="controller";url.value=i.url;serverId.value=i.server_id;mode.value=i.mode;ca.value="";approved.value=false;
 // Apply target after reactive server/role watchers clear the old pin.
 await nextTick();peerId.value=i.device_id;peerKey.value=i.public_key;approved.value=false;
 }catch(e){error.value=String(e);}finally{busy.value=false;}
}
async function makeInvitation(){
 if(!snapshot.value.local_key||!snapshot.value.device_id||busy.value)return;busy.value=true;error.value="";
 try{invitation.value=await invoke<string>("invitation_create",{url:url.value.trim(),serverId:serverId.value.trim(),publicKey:snapshot.value.local_key,mode:mode.value});}catch(e){error.value=String(e);}finally{busy.value=false;}
}
function prepare(){
 approved.value=false;submitted.value=false;
 return command("routed_prepare",{role:role.value,mode:mode.value,url:url.value.trim(),serverId:serverId.value.trim(),
 displayId:role.value==="host"&&isDisplay.value?displayId.value:null,relayOnly:relay.value,persistent:persistent.value,storageConsent:persistent.value,caPath:ca.value.trim()||null,saveHistory:saveHistory.value,quality:quality.value,sender:isExchange.value?sender.value:null,transferPath:mode.value==="file"?transferPath.value.trim():null,volume:volume.value,extraDisplays:role.value==="host"&&mode.value==="view"?extraDisplays.value:[]});
}
function confirm(){if(!canConfirm.value)return;submitted.value=true;return command("routed_confirm",{deviceId:peerId.value.trim(),peerKey:peerKey.value.trim(),approved:true});}
onMounted(()=>{void poll();timer=window.setInterval(poll,400);});
onBeforeUnmount(()=>{disposed=true;if(timer!==undefined)window.clearInterval(timer);if(native&&snapshot.value.active)void invoke("routed_disconnect",{slot:props.slot}).catch(()=>{});});
</script>
<template>
<section class="panel lan-panel">
 <header class="preview-head"><h2>{{ t("routedTitle") }}</h2><span class="status" role="status">{{ status }}</span></header>
 <p>{{ t("routedDescription") }}</p>
 <div v-if="snapshot.active&&isDisplay" class="view-notice">{{ t(role==="host"?"routedHostNotice":"controlControllerNotice") }}</div>
 <div class="field"><label>{{ t("invitationCode") }}</label><textarea v-model="invitation" maxlength="2048" :readonly="snapshot.active" rows="2"/><div class="actions"><button :disabled="busy||snapshot.active||!invitation.trim()" @click="readInvitation">{{ t("invitationImport") }}</button><button :disabled="busy||role!=='host'||!snapshot.device_id||isExchange||mode==='audio'" @click="makeInvitation">{{ t("invitationCreate") }}</button></div><p class="muted">{{ t("invitationNotice") }}</p></div>
 <div class="field">
  <label>{{ t("presenceUrl") }}</label><input v-model="url" placeholder="wss://…/v1/ws" maxlength="512" :disabled="busy||snapshot.active"/>
  <label>{{ t("presenceServerId") }}</label><input v-model="serverId" maxlength="64" class="key-input" :disabled="busy||snapshot.active"/>
  <label>{{ t("routedCa") }}</label><input v-model="ca" maxlength="1024" :disabled="busy||snapshot.active"/>
  <label>{{ t("sessionProfile") }}</label><select v-model="mode" :disabled="busy||snapshot.active"><option value="test">{{ t("secureTest") }}</option><option value="view">{{ t("screenView") }}</option><option value="control">{{ t("controlMode") }}</option><option value="file">{{ t("exchangeFile") }}</option><option value="text">{{ t("exchangeText") }}</option><option value="png">{{ t("exchangeImage") }}</option><option value="audio">{{ t("audioMode") }}</option></select>
  <label>{{ t("lanRole") }}</label><select v-model="role" :disabled="busy||snapshot.active"><option value="controller">{{ t("lanController") }}</option><option value="host">{{ t("lanHost") }}</option></select>
  <template v-if="role==='host'&&isDisplay">
   <label>{{ t("display") }}</label><div class="actions"><select v-model="displayId" :disabled="busy||snapshot.active"><option :value="null" disabled>{{ t("select") }}</option><option v-for="d in displays" :key="d.id" :value="d.id">{{ d.name }} · {{ d.width }} × {{ d.height }}</option></select><button :disabled="busy||snapshot.active" @click="loadDisplays">{{ t("refresh") }}</button></div>
   <template v-if="mode==='view'&&displayId!==null"><label>{{ t("extraDisplays") }}</label><label v-for="d in displays.filter(x=>x.id!==displayId)" :key="d.id" class="check-label"><input type="checkbox" v-model="extraDisplays" :value="d.id" :disabled="busy||snapshot.active||(extraDisplays.length>=3&&!extraDisplays.includes(d.id))"/>{{ d.name }} · {{ d.width }} × {{ d.height }}</label><p class="muted">{{ t("extraDisplaysNotice") }}</p></template>
  </template>
  <template v-if="isExchange"><label>{{ t("exchangeSender") }}</label><select v-model="sender" :disabled="busy||snapshot.active"><option value="controller">{{ t("lanController") }}</option><option value="host">{{ t("lanHost") }}</option></select><template v-if="mode==='file'"><label>{{ t(role===sender?"exchangeSource":"exchangeDestination") }}</label><div class="actions"><input v-model="transferPath" maxlength="2048" :disabled="busy||snapshot.active"/><button :disabled="busy||snapshot.active||!native" @click="chooseTransferFile">{{ t("exchangeChooseFile") }}</button></div></template><p class="muted">{{ t("exchangeNotice") }}</p></template>
  <template v-if="mode==='audio'"><label>{{ t("audioVolume") }}: {{ volume }}%</label><input type="range" v-model.number="volume" min="0" max="100" :disabled="busy||(snapshot.active&&(role==='host'||snapshot.stage!=='connected'))" @change="snapshot.stage==='connected'&&role==='controller'&&command('routed_audio_volume',{volume})"/><p class="muted">{{ t("audioNotice") }}</p></template>
  <label>{{ t("qualityPreset") }}</label><select v-model="quality" :disabled="busy||snapshot.active"><option value="compat">540p · 10 fps</option><option value="low">480p · 15 fps</option><option value="balanced">720p · 30 fps</option><option value="high">1080p · 30 fps</option><option value="qhd">1440p · 30 fps</option><option value="uhd">2160p · 30 fps</option></select><p class="muted">{{ t("qualityNotice") }}</p>
  <label class="check-label"><input type="checkbox" v-model="relay" :disabled="busy||snapshot.active"/>{{ t("routedRelay") }}</label>
  <label class="check-label"><input type="checkbox" v-model="persistent" :disabled="busy||snapshot.active"/>{{ t("presenceStorageConsent") }}</label>
  <button v-if="persistent" :disabled="busy||snapshot.active||!native" @click="prepareSlotIdentity">{{ t("identityPrepare") }}</button>
  <template v-if="enrollment&&persistent"><label>{{ t("identityEnrollment") }}</label><input readonly :value="enrollment.device_id" class="key-input"/><label>{{ t("trustPublicKey") }}</label><input readonly :value="enrollment.public_key" class="key-input"/></template>
  <label class="check-label"><input type="checkbox" v-model="saveHistory" :disabled="busy||snapshot.active"/>{{ t("deviceSaveHistory") }}</label>
  <div class="actions"><button class="primary" :disabled="!canPrepare" @click="prepare">{{ t("routedPrepare") }}</button><button :disabled="busy||!snapshot.active||snapshot.stage==='stopping'" @click="command('routed_disconnect')">{{ t("lanDisconnect") }}</button></div>
 </div>
 <div v-if="snapshot.local_key" class="field"><label>{{ t("lanLocalKey") }}</label><input readonly :value="snapshot.local_key" class="key-input"/><label v-if="snapshot.device_id">{{ t("presenceDeviceId") }}</label><input v-if="snapshot.device_id" readonly :value="snapshot.device_id" class="key-input"/></div>
 <div v-if="snapshot.stage==='awaiting_approval'&&snapshot.device_id" class="field">
  <label>{{ t("trustTitle") }}</label><div class="actions"><select v-model="chosen" :disabled="busy||submitted"><option value="">{{ t("routedManual") }}</option><option v-for="d in trusted" :key="d.device_id" :value="d.device_id">{{ d.label }} · {{ d.device_id.slice(0,12) }}…</option></select><button :disabled="busy||submitted" @click="loadTrusted">{{ t("refresh") }}</button></div>
  <label>{{ t("routedPeerId") }}</label><input v-model="peerId" maxlength="64" class="key-input" :disabled="busy||submitted||!!chosen"/>
  <label>{{ t("lanPeerKey") }}</label><input v-model="peerKey" maxlength="64" class="key-input" :disabled="busy||submitted||!!chosen"/>
  <p class="muted">{{ t("routedVerify") }}</p>
  <label class="check-label"><input type="checkbox" v-model="approved" :disabled="busy||submitted"/>{{ t(mode==="audio"?(role==="host"?"audioSendConsent":"audioPlayConsent"):isExchange?(role===sender?"exchangeSendConsent":"exchangeReceiveConsent"):mode==="test"?"routedTestConsent":mode==="view"?(role==="host"?(extraDisplays.length?"multiDisplayConsent":"viewHostConsent"):"viewControllerConsent"):(role==="host"?"controlHostConsent":"controlControllerConsent")) }}</label>
  <button class="primary" :disabled="!canConfirm" @click="confirm">{{ t("lanApprove") }}</button>
 </div>
 <div v-if="snapshot.stage==='connected'&&isDisplay" class="field"><label>{{ t("networkStats") }}</label><p class="muted">{{ t("networkBitrate") }}: {{ snapshot.network_bps==null?"—":(snapshot.network_bps/1000000).toFixed(2)+" Mb/s" }} · RTT: {{ snapshot.rtt_ms==null?"—":snapshot.rtt_ms+" ms" }} · {{ t("networkLoss") }}: {{ snapshot.loss_pct==null?"—":snapshot.loss_pct.toFixed(2)+"%" }} · {{ t("networkRoute") }}: {{ snapshot.route_type==="relay"?"TURN":snapshot.route_type==="direct"?"P2P":"—" }}</p></div>
 <p v-if="snapshot.decoded_fps!=null" class="muted">{{ t("decodedFps") }}: {{ snapshot.decoded_fps.toFixed(1) }} · {{ t("decodedThroughput") }}: {{ ((snapshot.decoded_bps??0)/1000000).toFixed(1) }} Mb/s</p>
 <div v-if="snapshot.transfer_total!=null" class="field"><label>{{ t("exchangeProgress") }}: {{ snapshot.transfer_done??0 }} / {{ snapshot.transfer_total }} B</label><progress :max="Math.max(1,snapshot.transfer_total)" :value="snapshot.transfer_done??0"/></div><div v-if="mode==='file'&&snapshot.stage==='connected'&&!snapshot.transfer_verified" class="actions"><span v-if="snapshot.transfer_paused" class="muted">{{ t("exchangePaused") }}</span><button v-if="role===sender" :disabled="busy" @click="command('routed_transfer_pause',{paused:!snapshot.transfer_paused})">{{ t(snapshot.transfer_paused?"exchangeResume":"exchangePause") }}</button></div><p v-if="snapshot.transfer_verified" class="muted">{{ t("exchangeVerified") }}</p>
 <p v-if="snapshot.input_released" class="muted">{{ t("controlReleased") }}</p>
 <p v-if="error||snapshot.detail" class="error" role="alert">{{ error||snapshot.detail }}</p>
 <p v-if="!native" class="muted">{{ t("browser") }}</p>
</section>
</template>
