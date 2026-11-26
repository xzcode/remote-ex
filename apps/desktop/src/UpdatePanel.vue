<script setup lang="ts">
import {ref,onMounted,onBeforeUnmount} from "vue";
import {invoke,isTauri} from "@tauri-apps/api/core";
import type {TextKey} from "./i18n";
const props=defineProps<{t:(key:TextKey)=>string;blocked:boolean}>();
const emit=defineEmits<{active:[boolean]}>();
type Build={version:string;tag:string|null;commit:string|null;platform:string;arch:string};
type Candidate={tag:string;prerelease:boolean;assets:{name:string;size:number}[]};
type Report={installed_tag:string|null;build_commit:string|null;status:string;candidate:Candidate|null;include_previews:boolean};
const build=ref<Build|null>(null),report=ref<Report|null>(null),previews=ref(true),running=ref(false),error=ref("");
const native=isTauri();let disposed=false;
const key=(status:string):TextKey=>({development:"updatesDevReport",update_available:"updatesAvailable",up_to_date:"updatesCurrent",installed_newer:"updatesNewer",no_compatible_release:"updatesNoRelease"} as Record<string,TextKey>)[status]??"updatesNoRelease";
onMounted(()=>{if(native)void invoke<Build>("build_info").then(b=>{if(!disposed){build.value=b;previews.value=!b.tag||b.tag.includes("-");}}).catch(e=>{if(!disposed)error.value=String(e);});});
onBeforeUnmount(()=>{disposed=true;if(running.value)emit("active",false);});
async function check(){
 if(!native||props.blocked||running.value)return;
 running.value=true;emit("active",true);report.value=null;error.value="";
 try{const r=await invoke<Report>("check_updates",{includePreviews:previews.value,confirmed:true});if(!disposed)report.value=r;}
 catch(e){if(!disposed)error.value=String(e);}
 finally{if(!disposed){running.value=false;emit("active",false);}}
}
async function open(){if(!native||props.blocked)return;try{await invoke("open_help",{target:"releases",confirmed:true});}catch(e){if(!disposed)error.value=String(e);}}
</script>
<template>
<section class="panel">
 <header class="preview-head"><h2>{{ t("updatesTitle") }}</h2><button class="primary" :disabled="!native||blocked||running" @click="check">{{ t(running?"busy":"updatesCheck") }}</button></header>
 <p>{{ t("updatesNotice") }}</p>
 <div v-if="build" class="preference-row"><label>{{ t("updatesInstalled") }}</label><span>{{ build.tag??t("updatesDevelopment") }}<small v-if="build.commit" class="muted"> · {{ build.commit.slice(0,12) }}</small></span></div>
 <label class="check-label"><input type="checkbox" v-model="previews" :disabled="blocked||running" @change="report=null"/>{{ t("updatesIncludePreviews") }}</label>
 <p v-if="error" role="alert" class="error">{{ error }}</p>
 <div v-if="report" class="field">
  <p role="status"><strong>{{ t(key(report.status)) }}</strong></p>
  <template v-if="report.candidate">
   <p>{{ report.candidate.tag }} · {{ t(report.candidate.prerelease?"updatesPreview":"updatesStable") }}</p>
   <div v-for="asset in report.candidate.assets" :key="asset.name" class="preference-row"><label class="asset-name">{{ asset.name }}</label><span>{{ (asset.size/1048576).toFixed(1) }} MiB</span></div>
   <p class="muted">{{ t("updatesManual") }}</p>
  </template>
 </div>
 <button :disabled="!native||blocked||running" @click="open">{{ t("updatesOpen") }}</button>
</section>
</template>
