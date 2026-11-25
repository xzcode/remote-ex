<script setup lang="ts">
import {computed,ref,watch,onMounted,onBeforeUnmount,nextTick} from "vue";
import {invoke,isTauri} from "@tauri-apps/api/core";
import type {TextKey} from "./i18n";
import {getCurrentWebview} from "@tauri-apps/api/webview";
type Stage="idle"|"starting"|"awaiting_approval"|"connecting"|"listening"|"connected"|"stopping"|"passed"|"failed"|"canceled";
type CatalogEntry={parts:string[];directory:boolean;size:number};
type Change={op:'create_folder';parent:number|null;name:string}|{op:'create_file';parent:number|null;name:string}|{op:'rename';index:number;name:string}|{op:'recycle';index:number};
type FileReview={items:{operation:string;source:string[]|null;destination:string[]}[];recovery:string|null};
type Snapshot={stage:Stage;active:boolean;local_key:string|null;device_id:string|null;detail:string|null;profile:string;input_released:boolean;decoded_fps?:number|null;decoded_bps?:number|null;transfer_done?:number|null;transfer_total?:number|null;transfer_verified?:boolean;transfer_paused?:boolean;checkpoint_saved?:number|null;directory_entries?:number|null;clipboard_sent?:number|null;clipboard_received?:number|null;catalog?:CatalogEntry[];catalog_ready?:boolean;catalog_selected?:number|null;manage_review?:FileReview|null;manage_token?:string|null;manage_submitted?:boolean;manage_approved?:boolean;manage_applied?:number|null;resume_offset?:number|null;audio_volume?:number|null;network_bps?:number|null;rtt_ms?:number|null;loss_pct?:number|null;route_type?:string|null};
type Device={device_id:string;public_key:string;label:string};
type Display={id:number;name:string;width:number;height:number;primary:boolean};
const props=defineProps<{t:(key:TextKey)=>string;slot:number;visible:boolean}>();
const emit=defineEmits<{active:[value:boolean]}>();
const native=isTauri(),role=ref<"host"|"controller">("controller"),mode=ref<"test"|"view"|"control"|"file"|"text"|"png"|"audio"|"directory"|"clipboard_sync"|"catalog"|"av_view"|"av_control"|"manage">("test");
const sender=ref<"host"|"controller">("controller"),transferPath=ref(""),volume=ref(75);
const extraDisplays=ref<number[]>([]),secondaryControl=ref(false);
const canSecondary=computed(()=>role.value==="host"&&(mode.value==="control"||mode.value==="av_control"));
const catalogQuery=ref(""),catalogChoice=ref<number|null>(null);
const changes=ref<Change[]>([]),operation=ref<"create_folder"|"create_file"|"rename"|"recycle">("create_folder"),newName=ref(""),parentIndex=ref<number|null>(null),fileIndex=ref<number|null>(null),reviewConfirmed=ref(false),planConfirmed=ref(false);
const managedEditable=computed(()=>mode.value==="manage"&&role.value==="controller"&&snapshot.value.catalog_ready&&snapshot.value.stage==="connected"&&!snapshot.value.manage_submitted);
const manageFile=computed(()=>operation.value==="rename"||operation.value==="recycle");
const actionNames:Record<string,TextKey>={create_folder:"manageCreateFolder",create_file:"manageCreateFile",rename:"manageRename",recycle:"manageRecycle"};
const actionName=(op:string):TextKey=>actionNames[op]??"manageUnknown";
function appendChange(){
 if(!managedEditable.value||changes.value.length>=4)return;
 if(manageFile.value){if(fileIndex.value===null)return;changes.value.push(operation.value==="rename"?{op:"rename",index:fileIndex.value,name:newName.value.trim()}:{op:"recycle",index:fileIndex.value});}
 else if(operation.value==="create_folder"){changes.value.push({op:"create_folder",parent:parentIndex.value,name:newName.value.trim()});}
 else if(operation.value==="create_file"){changes.value.push({op:"create_file",parent:parentIndex.value,name:newName.value.trim()});}
 newName.value="";planConfirmed.value=false;
}
function describeChange(c:Change){const row=snapshot.value.catalog??[];if(c.op==="rename"||c.op==="recycle"){const src=row[c.index]?.parts.join("/")??"?";return c.op==="rename"?`${src} → ${c.name}`:src;}return `${c.parent===null?"":(row[c.parent]?.parts.join("/")??"?")+"/"}${c.name}`;}

const catalogRows=computed(()=> (snapshot.value.catalog??[]).map((entry,index)=>({entry,index})).filter(({entry})=>entry.parts.join("/").toLowerCase().includes(catalogQuery.value.toLowerCase())));
const resumeFrom=ref(""),checkpointPath=ref("");
const fileReceiver=computed(()=>mode.value==="file"&&role.value!==sender.value);
watch([mode,role,sender],()=>{resumeFrom.value="";checkpointPath.value="";});
const isExchange=computed(()=>["file","text","png","directory"].includes(mode.value));
const isView=computed(()=>mode.value==="view"||mode.value==="av_view");
const isAv=computed(()=>mode.value==="av_view"||mode.value==="av_control");
const isDisplay=computed(()=>isView.value||mode.value==="control"||mode.value==="av_control");
const hasAudio=computed(()=>isAv.value||mode.value==="audio");
const url=ref(""),serverId=ref(""),ca=ref(""),relay=ref(false),persistent=ref(false),sharedIdentity=ref(false);
const shareAvailable=computed(()=>persistent.value&&role.value==="controller"&&["test","view","control","av_view","av_control"].includes(mode.value));
const enrollment=ref<{device_id:string;public_key:string}|null>(null);
watch(persistent,value=>{if(!value){enrollment.value=null;sharedIdentity.value=false;}});
watch(shareAvailable,value=>{if(!value)sharedIdentity.value=false;});
watch(sharedIdentity,()=>{enrollment.value=null;});
const displays=ref<Display[]>([]),displayId=ref<number|null>(null),trusted=ref<Device[]>([]),chosen=ref(""),peerId=ref(""),peerKey=ref("");
const approved=ref(false),submitted=ref(false),busy=ref(false),error=ref("");
const saveHistory=ref(false),invitation=ref(""),quality=ref("compat");
type Invitation={url:string;server_id:string;device_id:string;public_key:string;mode:"test"|"view"|"control"|"av_view"|"av_control";expires_at:number};
const snapshot=ref<Snapshot>({stage:"idle",active:false,local_key:null,device_id:null,detail:null,profile:"routed_test",input_released:false});
watch(()=>snapshot.value.manage_token,()=>{reviewConfirmed.value=false;});
let timer:number|undefined,disposed=false,polling=false,revision=0;
const statuses:Record<Stage,TextKey>={idle:"lanIdle",starting:"lanStarting",awaiting_approval:"lanAwaiting",connecting:"lanConnecting",listening:"lanListening",connected:"routedConnected",stopping:"controlStopping",passed:"controlEnded",failed:"lanFailed",canceled:"lanCanceled"};
const status=computed(()=>props.t(statuses[snapshot.value.stage]));
const validHex=(s:string)=>/^[a-fA-F0-9]{64}$/.test(s.trim());
const canPrepare=computed(()=>native&&!busy.value&&!snapshot.value.active&&url.value.trim().startsWith("wss://")&&validHex(serverId.value)&&(!isDisplay.value||role.value==="controller"||displayId.value!==null)&&(!(["file","directory","catalog"].includes(mode.value)||mode.value==="manage"&&role.value==="host")||!!transferPath.value.trim()));
const canConfirm=computed(()=>!busy.value&&snapshot.value.stage==="awaiting_approval"&&!!snapshot.value.device_id&&validHex(peerId.value)&&validHex(peerKey.value)&&approved.value&&!submitted.value&&peerKey.value.trim().toLowerCase()!==snapshot.value.local_key);
watch([role,mode],()=>{secondaryControl.value=false;displayId.value=null;extraDisplays.value=[];displays.value=[];approved.value=false;});
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
 try{enrollment.value=await invoke("routed_identity_info",{slot:props.slot,confirmed:true,sharedIdentity:shareAvailable.value&&sharedIdentity.value});}
 catch(e){error.value=String(e);}finally{busy.value=false;}
}
async function chooseTransferFile(){
 if(!native||busy.value||snapshot.value.active||!["file","directory","catalog","manage"].includes(mode.value))return;
 busy.value=true;error.value="";
 try{const p=await invoke<string|null>("select_transfer_file",{receive:mode.value==="manage"?false:mode.value==="catalog"?role.value==="controller":role.value!==sender.value,directory:mode.value==="manage"?true:mode.value==="catalog"?role.value==="host":mode.value==="directory"&&role.value===sender.value});if(p)transferPath.value=p;}
 catch(e){error.value=String(e);}finally{busy.value=false;}
}
async function chooseCheckpoint(save:boolean){
 if(!native||busy.value||snapshot.value.active||!fileReceiver.value)return;
 busy.value=true;error.value="";
 try{const p=await invoke<string|null>("select_transfer_file",{receive:save});if(p){if(save)checkpointPath.value=p;else resumeFrom.value=p;}}
 catch(e){error.value=String(e);}finally{busy.value=false;}
}
async function loadDisplays(){
 if(busy.value||snapshot.value.active)return;busy.value=true;error.value="";
 try{displays.value=(await invoke<Display[]>("list_displays")).filter(d=>isView.value||secondaryControl.value||d.primary);displayId.value=null;extraDisplays.value=[];if(!displays.value.length)error.value=props.t("noDisplays");}
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
 approved.value=false;submitted.value=false;catalogChoice.value=null;catalogQuery.value="";changes.value=[];parentIndex.value=null;fileIndex.value=null;newName.value="";reviewConfirmed.value=false;planConfirmed.value=false;
 return command("routed_prepare",{role:role.value,mode:mode.value,url:url.value.trim(),serverId:serverId.value.trim(),
 displayId:role.value==="host"&&isDisplay.value?displayId.value:null,relayOnly:relay.value,persistent:persistent.value,storageConsent:persistent.value,caPath:ca.value.trim()||null,saveHistory:saveHistory.value,quality:quality.value,sender:isExchange.value?sender.value:null,transferPath:(["file","directory","catalog"].includes(mode.value)||mode.value==="manage"&&role.value==="host")?transferPath.value.trim():null,volume:volume.value,extraDisplays:role.value==="host"&&isView.value?extraDisplays.value:[],resumeFrom:fileReceiver.value?resumeFrom.value.trim()||null:null,checkpointPath:fileReceiver.value?checkpointPath.value.trim()||null:null,sharedIdentity:shareAvailable.value&&sharedIdentity.value,secondaryControl:canSecondary.value&&secondaryControl.value});
}
function confirm(){if(!canConfirm.value)return;submitted.value=true;return command("routed_confirm",{deviceId:peerId.value.trim(),peerKey:peerKey.value.trim(),approved:true});}
let stopDrop:(()=>void)|undefined;
onMounted(()=>{void poll();timer=window.setInterval(poll,400);if(native)void getCurrentWebview().onDragDropEvent(event=>{
 if(event.payload.type!=="drop"||!props.visible||busy.value||snapshot.value.active||role.value!==sender.value||!["file","directory"].includes(mode.value))return;
 if(event.payload.paths.length!==1){error.value=props.t("dropSingle");return;}
 transferPath.value=event.payload.paths[0]??"";approved.value=false;
}).then(unlisten=>{if(disposed)unlisten();else stopDrop=unlisten;}).catch(()=>{});});
onBeforeUnmount(()=>{disposed=true;stopDrop?.();if(timer!==undefined)window.clearInterval(timer);if(native&&snapshot.value.active)void invoke("routed_disconnect",{slot:props.slot}).catch(()=>{});});
</script>
<template>
<section class="panel lan-panel">
 <header class="preview-head"><h2>{{ t("routedTitle") }}</h2><span class="status" role="status">{{ status }}</span></header>
 <p>{{ t("routedDescription") }}</p>
 <div v-if="snapshot.active&&isDisplay" class="view-notice">{{ t(role==="host"?"routedHostNotice":"controlControllerNotice") }}</div>
 <div class="field"><label>{{ t("invitationCode") }}</label><textarea v-model="invitation" maxlength="2048" :readonly="snapshot.active" rows="2"/><div class="actions"><button :disabled="busy||snapshot.active||!invitation.trim()" @click="readInvitation">{{ t("invitationImport") }}</button><button :disabled="busy||role!=='host'||!snapshot.device_id||isExchange||mode==='audio'||mode==='clipboard_sync'||mode==='catalog'||mode==='manage'" @click="makeInvitation">{{ t("invitationCreate") }}</button></div><p class="muted">{{ t("invitationNotice") }}</p></div>
 <div class="field">
  <label>{{ t("presenceUrl") }}</label><input v-model="url" placeholder="wss://…/v1/ws" maxlength="512" :disabled="busy||snapshot.active"/>
  <label>{{ t("presenceServerId") }}</label><input v-model="serverId" maxlength="64" class="key-input" :disabled="busy||snapshot.active"/>
  <label>{{ t("routedCa") }}</label><input v-model="ca" maxlength="1024" :disabled="busy||snapshot.active"/>
  <label>{{ t("sessionProfile") }}</label><select v-model="mode" :disabled="busy||snapshot.active"><option value="test">{{ t("secureTest") }}</option><option value="view">{{ t("screenView") }}</option><option value="control">{{ t("controlMode") }}</option><option value="av_view">{{ t("avView") }}</option><option value="av_control">{{ t("avControl") }}</option><option value="file">{{ t("exchangeFile") }}</option><option value="directory">{{ t("exchangeDirectory") }}</option><option value="catalog">{{ t("catalogMode") }}</option><option value="manage">{{ t("manageMode") }}</option><option value="text">{{ t("exchangeText") }}</option><option value="clipboard_sync">{{ t("clipboardSync") }}</option><option value="png">{{ t("exchangeImage") }}</option><option value="audio">{{ t("audioMode") }}</option></select>
  <label>{{ t("lanRole") }}</label><select v-model="role" :disabled="busy||snapshot.active"><option value="controller">{{ t("lanController") }}</option><option value="host">{{ t("lanHost") }}</option></select>
  <template v-if="role==='host'&&isDisplay">
   <label v-if="canSecondary" class="check-label"><input type="checkbox" v-model="secondaryControl" :disabled="busy||snapshot.active" @change="displayId=null;displays=[]"/>{{ t("secondaryControl") }}</label>
   <p v-if="canSecondary&&secondaryControl" class="muted">{{ t("secondaryNotice") }}</p>
   <label>{{ t("display") }}</label><div class="actions"><select v-model="displayId" :disabled="busy||snapshot.active"><option :value="null" disabled>{{ t("select") }}</option><option v-for="d in displays" :key="d.id" :value="d.id">{{ d.name }} · {{ d.width }} × {{ d.height }}</option></select><button :disabled="busy||snapshot.active" @click="loadDisplays">{{ t("refresh") }}</button></div>
   <template v-if="isView&&displayId!==null"><label>{{ t("extraDisplays") }}</label><label v-for="d in displays.filter(x=>x.id!==displayId)" :key="d.id" class="check-label"><input type="checkbox" v-model="extraDisplays" :value="d.id" :disabled="busy||snapshot.active||(extraDisplays.length>=3&&!extraDisplays.includes(d.id))"/>{{ d.name }} · {{ d.width }} × {{ d.height }}</label><p class="muted">{{ t("extraDisplaysNotice") }}</p></template>
  </template>
  <template v-if="isExchange"><label>{{ t("exchangeSender") }}</label><select v-model="sender" :disabled="busy||snapshot.active"><option value="controller">{{ t("lanController") }}</option><option value="host">{{ t("lanHost") }}</option></select><template v-if="mode==='file'||mode==='directory'"><label>{{ t(mode==="directory"?(role===sender?"directorySource":"directoryDestination"):(role===sender?"exchangeSource":"exchangeDestination")) }}</label><div class="actions"><input v-model="transferPath" maxlength="2048" :disabled="busy||snapshot.active"/><button :disabled="busy||snapshot.active||!native" @click="chooseTransferFile">{{ t(mode==="directory"&&role===sender?"directoryChoose":"exchangeChooseFile") }}</button></div><p v-if="role===sender" class="muted">{{ t("dropSingle") }}</p><p v-if="mode==='directory'" class="muted">{{ t("directoryNotice") }}</p></template><p class="muted">{{ t("exchangeNotice") }}</p></template>
  <div v-if="mode==='catalog'" class="field">
    <label>{{ t(role==="host"?"catalogRoot":"exchangeDestination") }}</label>
    <div class="actions"><input v-model="transferPath" maxlength="2048" :disabled="busy||snapshot.active"/><button :disabled="busy||snapshot.active||!native" @click="chooseTransferFile">{{ t(role==="host"?"directoryChoose":"exchangeChooseFile") }}</button></div>
    <p class="muted">{{ t("catalogNotice") }}</p>
  </div>
  <div v-if="mode==='manage'" class="field">
    <template v-if="role==='host'"><label>{{ t("manageRoot") }}</label><div class="actions"><input v-model="transferPath" maxlength="2048" :disabled="busy||snapshot.active"/><button :disabled="busy||snapshot.active||!native" @click="chooseTransferFile">{{ t("directoryChoose") }}</button></div></template>
    <p class="muted">{{ t("manageNotice") }}</p>
  </div>
  <template v-if="fileReceiver">
    <label>{{ t("resumeExisting") }}</label><div class="actions"><input v-model="resumeFrom" maxlength="2048" :disabled="busy||snapshot.active"/><button :disabled="busy||snapshot.active||!native" @click="chooseCheckpoint(false)">{{ t("exchangeChooseFile") }}</button></div>
    <label>{{ t("resumeSave") }}</label><div class="actions"><input v-model="checkpointPath" maxlength="2048" :disabled="busy||snapshot.active"/><button :disabled="busy||snapshot.active||!native" @click="chooseCheckpoint(true)">{{ t("exchangeChooseFile") }}</button></div>
    <p class="muted">{{ t("resumeNotice") }}</p>
  </template>
  <p v-if="mode==='clipboard_sync'" class="view-notice">{{ t("clipboardSyncNotice") }}</p>
  <template v-if="hasAudio"><label>{{ t("audioVolume") }}: {{ volume }}%</label><input type="range" v-model.number="volume" min="0" max="100" :disabled="busy||(snapshot.active&&(role==='host'||snapshot.stage!=='connected'))" @change="snapshot.stage==='connected'&&role==='controller'&&command('routed_audio_volume',{volume})"/><p class="muted">{{ t("audioNotice") }}</p></template>
  <label>{{ t("qualityPreset") }}</label><select v-model="quality" :disabled="busy||snapshot.active"><option value="compat">540p · 10 fps</option><option value="low">480p · 15 fps</option><option value="balanced">720p · 30 fps</option><option value="high">1080p · 30 fps</option><option value="qhd">1440p · 30 fps</option><option value="uhd">2160p · 30 fps</option></select><p class="muted">{{ t("qualityNotice") }}</p>
  <label class="check-label"><input type="checkbox" v-model="relay" :disabled="busy||snapshot.active"/>{{ t("routedRelay") }}</label>
  <label class="check-label"><input type="checkbox" v-model="persistent" :disabled="busy||snapshot.active"/>{{ t("presenceStorageConsent") }}</label>
  <label v-if="shareAvailable" class="check-label"><input type="checkbox" v-model="sharedIdentity" :disabled="busy||snapshot.active"/>{{ t("sharedIdentity") }}</label>
  <p v-if="shareAvailable&&sharedIdentity" class="muted">{{ t("sharedIdentityNotice") }}</p>
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
  <label class="check-label"><input type="checkbox" v-model="approved" :disabled="busy||submitted"/>{{ t(mode==="manage"?(role==="host"?"manageHostConsent":"manageControllerConsent"):canSecondary&&secondaryControl?(isAv?"secondaryAvConsent":"secondaryConsent"):isAv?(isView?(role==="host"?"avViewHostConsent":"avViewControllerConsent"):(role==="host"?"avControlHostConsent":"avControlControllerConsent")):mode==="catalog"?(role==="host"?"catalogHostConsent":"catalogControllerConsent"):mode==="clipboard_sync"?"clipboardSyncConsent":mode==="audio"?(role==="host"?"audioSendConsent":"audioPlayConsent"):isExchange?(role===sender?"exchangeSendConsent":"exchangeReceiveConsent"):mode==="test"?"routedTestConsent":mode==="view"?(role==="host"?(extraDisplays.length?"multiDisplayConsent":"viewHostConsent"):"viewControllerConsent"):(role==="host"?"controlHostConsent":"controlControllerConsent")) }}</label>
  <button class="primary" :disabled="!canConfirm" @click="confirm">{{ t("lanApprove") }}</button>
 </div>
 <div v-if="snapshot.stage==='connected'&&isDisplay" class="field"><label>{{ t("networkStats") }}</label><p class="muted">{{ t("networkBitrate") }}: {{ snapshot.network_bps==null?"—":(snapshot.network_bps/1000000).toFixed(2)+" Mb/s" }} · RTT: {{ snapshot.rtt_ms==null?"—":snapshot.rtt_ms+" ms" }} · {{ t("networkLoss") }}: {{ snapshot.loss_pct==null?"—":snapshot.loss_pct.toFixed(2)+"%" }} · {{ t("networkRoute") }}: {{ snapshot.route_type==="relay"?"TURN":snapshot.route_type==="direct"?"P2P":"—" }}</p></div>
 <p v-if="snapshot.decoded_fps!=null" class="muted">{{ t("decodedFps") }}: {{ snapshot.decoded_fps.toFixed(1) }} · {{ t("decodedThroughput") }}: {{ ((snapshot.decoded_bps??0)/1000000).toFixed(1) }} Mb/s</p>
 <div v-if="snapshot.transfer_total!=null" class="field"><label>{{ t("exchangeProgress") }}: {{ snapshot.transfer_done??0 }} / {{ snapshot.transfer_total }} B</label><progress :max="Math.max(1,snapshot.transfer_total)" :value="snapshot.transfer_done??0"/></div><div v-if="mode==='file'&&snapshot.stage==='connected'&&!snapshot.transfer_verified" class="actions"><span v-if="snapshot.transfer_paused" class="muted">{{ t("exchangePaused") }}</span><button v-if="role===sender" :disabled="busy" @click="command('routed_transfer_pause',{paused:!snapshot.transfer_paused})">{{ t(snapshot.transfer_paused?"exchangeResume":"exchangePause") }}</button></div><p v-if="snapshot.transfer_verified" class="muted">{{ t("exchangeVerified") }}</p>
 <div v-if="mode==='manage'&&role==='controller'&&snapshot.catalog_ready" class="field">
  <label>{{ t("catalogTitle") }} ({{ snapshot.catalog?.length??0 }})</label>
  <input v-model="catalogQuery" :placeholder="t('deviceSearch')"/>
  <div style="max-height:200px;overflow:auto;border:1px solid var(--border);border-radius:6px"><div v-for="{entry,index} in catalogRows" :key="index" style="padding:6px 8px;overflow-wrap:anywhere">{{ entry.directory?"▸":"·" }} {{ entry.parts.join("/") }} <span v-if="!entry.directory" class="muted">{{ entry.size }} B</span></div></div>
  <label>{{ t("manageAction") }}</label>
  <select v-model="operation" :disabled="busy||!managedEditable"><option value="create_folder">{{ t("manageCreateFolder") }}</option><option value="create_file">{{ t("manageCreateFile") }}</option><option value="rename">{{ t("manageRename") }}</option><option value="recycle">{{ t("manageRecycle") }}</option></select>
  <select v-if="manageFile" v-model="fileIndex" :disabled="busy||!managedEditable"><option :value="null">{{ t("select") }}</option><option v-for="{entry,index} in catalogRows.filter(x=>!x.entry.directory)" :key="index" :value="index">{{ entry.parts.join("/") }}</option></select>
  <select v-else v-model="parentIndex" :disabled="busy||!managedEditable"><option :value="null">{{ t("manageRootLevel") }}</option><option v-for="{entry,index} in catalogRows.filter(x=>x.entry.directory)" :key="index" :value="index">{{ entry.parts.join("/") }}</option></select>
  <input v-if="operation!=='recycle'" v-model="newName" maxlength="96" :placeholder="t('manageNewName')" :disabled="busy||!managedEditable"/>
  <button :disabled="busy||!managedEditable||changes.length>=4||(manageFile&&fileIndex===null)||(operation!=='recycle'&&!newName.trim())" @click="appendChange">{{ t("manageAdd") }}</button>
  <label>{{ t("manageProposed") }} ({{ changes.length }}/4)</label>
  <div v-for="(c,i) in changes" :key="i" class="actions" style="overflow-wrap:anywhere"><span>{{ t(actionName(c.op)) }}: {{ describeChange(c) }}</span><button :disabled="busy||!managedEditable" @click="changes.splice(i,1);planConfirmed=false">{{ t("trustRemove") }}</button></div>
  <label class="check-label"><input type="checkbox" v-model="planConfirmed" :disabled="!managedEditable||busy"/>{{ t("manageSendConfirm") }}</label>
  <button class="primary" :disabled="busy||!managedEditable||!changes.length||!planConfirmed" @click="command('routed_manage_submit',{plan:{changes},confirmed:true})">{{ t("manageSend") }}</button>
  <p v-if="snapshot.manage_submitted" class="muted">{{ t("manageWaiting") }}</p>
 </div>
 <div v-if="mode==='manage'&&role==='host'&&snapshot.manage_review" class="field">
  <label>{{ t("manageReview") }}</label><p>{{ t("manageReviewNotice") }}</p>
  <div v-for="(r,i) in snapshot.manage_review.items" :key="i" style="overflow-wrap:anywhere;padding:6px 0"><strong>{{ t(actionName(r.operation)) }}</strong><div v-if="r.source">{{ r.source.join("/") }} →</div><div>{{ r.destination.join("/") }}</div></div>
  <p v-if="snapshot.manage_review.recovery" class="muted">{{ t("manageRecovery") }}: {{ snapshot.manage_review.recovery }}</p>
  <label class="check-label"><input type="checkbox" v-model="reviewConfirmed" :disabled="busy||snapshot.manage_approved||!snapshot.active"/>{{ t("manageExactConfirm") }}</label>
  <button class="primary" :disabled="busy||!snapshot.active||snapshot.manage_approved||!reviewConfirmed" @click="command('routed_manage_approve',{token:snapshot.manage_token,confirmed:true})">{{ t("manageApprove") }}</button>
 </div>
 <p v-if="snapshot.manage_applied!=null" class="muted">{{ t("manageApplied") }}: {{ snapshot.manage_applied }}</p>
 <div v-if="mode==='catalog'&&role==='controller'&&snapshot.catalog_ready" class="field">
   <label>{{ t("catalogTitle") }} ({{ snapshot.catalog?.length??0 }})</label>
   <input v-model="catalogQuery" :placeholder="t('deviceSearch')"/>
   <div class="catalog-list" style="max-height:280px;overflow:auto;border:1px solid var(--border);border-radius:6px">
     <div v-for="{entry,index} in catalogRows" :key="index" class="actions" style="padding:6px 8px;overflow-wrap:anywhere">
       <input v-if="!entry.directory" type="radio" name="catalog-file" :value="index" v-model="catalogChoice" :disabled="busy||snapshot.catalog_selected!=null||!snapshot.active"/>
       <span>{{ entry.directory?"▸":"·" }} {{ entry.parts.join("/") }}</span>
       <span v-if="!entry.directory" class="muted">{{ entry.size }} B</span>
     </div>
   </div>
   <button class="primary" :disabled="busy||!snapshot.active||snapshot.stage!=='connected'||catalogChoice===null||snapshot.catalog_selected!=null" @click="command('routed_catalog_select',{index:catalogChoice,confirmed:true})">{{ t("catalogDownload") }}</button>
 </div>
 <p v-if="snapshot.clipboard_sent!=null" class="muted">{{ t("clipboardSyncCounts") }}: {{ snapshot.clipboard_sent }} / {{ snapshot.clipboard_received??0 }}</p>
 <p v-if="snapshot.directory_entries!=null" class="muted">{{ t("directoryEntries") }}: {{ snapshot.directory_entries }}</p>
 <p v-if="snapshot.resume_offset!=null" class="muted">{{ t("resumeOffset") }}: {{ snapshot.resume_offset }} B</p>
 <p v-if="snapshot.checkpoint_saved!=null" class="muted">{{ t("resumeSaved") }}: {{ snapshot.checkpoint_saved }} B</p>
 <p v-if="snapshot.input_released" class="muted">{{ t("controlReleased") }}</p>
 <p v-if="error||snapshot.detail" class="error" role="alert">{{ error||snapshot.detail }}</p>
 <p v-if="!native" class="muted">{{ t("browser") }}</p>
</section>
</template>
