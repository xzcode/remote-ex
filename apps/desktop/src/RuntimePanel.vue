<script setup lang="ts">
import {computed,ref,onBeforeUnmount} from "vue";
import {invoke,isTauri} from "@tauri-apps/api/core";
import type {TextKey} from "./i18n";
const props=defineProps<{t:(key:TextKey)=>string;blocked:boolean}>();
const emit=defineEmits<{active:[boolean]}>();
type Component={name:string;present:boolean;integrity:string};
type Probe={schema:number;initialized:boolean;version:number[]|null;missing:string[];synthetic_video:boolean};
type Report={schema:number;app_version:string;platform:string;arch:string;bundle_commit:string|null;manifest:string;components:Component[];probe:Probe|null;probe_status:string;core_ready:boolean;audio_ready:boolean;portal_ready:boolean};
const running=ref(false),report=ref<Report|null>(null),error=ref(""),showReport=ref(false);
let disposed=false;
const native=isTauri();
const statusKey=(s:string):TextKey=>({verified:"runtimeVerified",unverified:"runtimeUnverified",missing:"runtimeMissing",mismatch:"runtimeMismatch",invalid:"runtimeInvalid",absent:"runtimeAbsent",complete:"runtimeComplete",launch_failed:"runtimeLaunchFailed",timeout:"runtimeTimeout",invalid_output:"runtimeInvalidOutput",component_unavailable:"runtimeComponentUnavailable"} as Record<string,TextKey>)[s]??"permissionUnknown";
const exportText=computed(()=>report.value?JSON.stringify(report.value,null,2):"");
async function check(){
 if(running.value||props.blocked||!native)return;running.value=true;emit("active",true);error.value="";report.value=null;showReport.value=false;
 try{const r=await invoke<Report>("runtime_check");if(!disposed)report.value=r;}
 catch(e){if(!disposed)error.value=String(e);}
 finally{if(!disposed){running.value=false;emit("active",false);}}
}
onBeforeUnmount(()=>{disposed=true;if(running.value)emit("active",false);});
</script>
<template>
 <section class="panel">
   <header class="preview-head"><h2>{{ t("runtimeTitle") }}</h2><button class="primary" :disabled="running||blocked||!native" @click="check">{{ t(running?"busy":"runtimeCheck") }}</button></header>
   <p>{{ t("runtimeNotice") }}</p>
   <p v-if="!native" class="muted">{{ t("browser") }}</p>
   <p v-if="error" class="error" role="alert">{{ error }}</p>
   <template v-if="report">
     <p class="muted">Remote EX {{ report.app_version }} · {{ report.platform }} / {{ report.arch }}<span v-if="report.bundle_commit"> · {{ report.bundle_commit.slice(0,12) }}</span></p>
     <div class="preference-row"><label>{{ t("runtimeManifest") }}</label><span class="muted">{{ t(statusKey(report.manifest)) }}</span></div>
     <div v-for="c in report.components" :key="c.name" class="preference-row"><label>{{ c.name }}</label><span class="muted">{{ t(statusKey(c.integrity)) }}</span></div>
     <div class="preference-row"><label>GStreamer</label><span>{{ report.probe?.version?report.probe.version.slice(0,3).join("."):t(statusKey(report.probe_status)) }}</span></div>
     <div class="preference-row"><label>{{ t("runtimeCodec") }}</label><span>{{ t(report.probe?.synthetic_video?"runtimePassed":"runtimeNotReady") }}</span></div>
     <div class="preference-row"><label>{{ t("runtimeCore") }}</label><span>{{ t(report.core_ready?"runtimePassed":"runtimeNotReady") }}</span></div>
     <div class="preference-row"><label>{{ t("runtimeAudio") }}</label><span>{{ t(report.audio_ready?"runtimePassed":"runtimeNotReady") }}</span></div>
     <div v-if="report.platform==='linux'" class="preference-row"><label>{{ t("runtimePipewire") }}</label><span>{{ t(report.portal_ready?"runtimePassed":"runtimeNotReady") }}</span></div>
     <template v-if="report.probe?.missing.length"><p class="muted">{{ t("runtimeMissingElements") }}</p><div class="runtime-elements"><code v-for="name in report.probe.missing" :key="name">{{ name }}</code></div></template>
     <p v-if="!report.core_ready" class="error">{{ t("runtimeRepair") }}</p>
     <p class="muted">{{ t("runtimeNotAcceptance") }}</p>
     <button :disabled="running" @click="showReport=!showReport">{{ t(showReport?"runtimeHideReport":"runtimeShowReport") }}</button>
     <textarea v-if="showReport" class="runtime-report" :value="exportText" readonly rows="8" :aria-label="t('runtimeShowReport')" @focus="($event.target as HTMLTextAreaElement).select()"></textarea>
   </template>
 </section>
</template>
