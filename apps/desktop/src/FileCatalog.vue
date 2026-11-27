<script setup lang="ts">
import {computed,ref,watch} from "vue";
import type {TextKey} from "./i18n";
import {catalogRows,selectedRows,toggleCatalogFile,validFolder,type CatalogEntry,type CatalogSort} from "./catalog-view";
import WorkspaceIcon from "./WorkspaceIcon.vue";
const props=defineProps<{t:(key:TextKey)=>string;entries:CatalogEntry[];selected:number[];limit:1|4;disabled:boolean;inspectOnly?:boolean}>();
const emit=defineEmits<{change:[indexes:number[]]}>();
const folder=ref<string[]>([]),query=ref(""),sort=ref<CatalogSort>("name");
const rows=computed(()=>catalogRows(props.entries,folder.value,query.value,sort.value));
const selected=computed(()=>selectedRows(props.entries,props.selected));
const bytes=computed(()=>selected.value.reduce((sum,r)=>sum+r.entry.size,0));
watch(()=>props.entries,entries=>{if(!validFolder(entries,folder.value))folder.value=[];});
function go(parts:string[]){if(!validFolder(props.entries,parts))return;folder.value=[...parts];query.value="";}
function choose(index:number){if(props.disabled||props.inspectOnly)return;emit("change",toggleCatalogFile(props.entries,props.selected,index,props.limit));}
function amount(size:number){return size.toLocaleString()+" B";}
</script>
<template>
 <div class="file-catalog">
  <div class="catalog-bar">
   <button :disabled="folder.length===0" @click="go(folder.slice(0,-1))" :title="t('catalogUp')">↑ {{ t("catalogUp") }}</button>
   <div class="catalog-breadcrumb" :aria-label="t('catalogTitle')"><button @click="go([])">{{ t("manageRootLevel") }}</button><template v-for="(part,i) in folder" :key="i"><span aria-hidden="true">/</span><button @click="go(folder.slice(0,i+1))">{{ part }}</button></template></div>
  </div>
  <div class="catalog-bar"><input v-model="query" type="search" maxlength="128" :aria-label="t('catalogSearch')" :placeholder="t('catalogSearch')"/><select v-model="sort" :aria-label="t('catalogSort')"><option value="name">{{ t("catalogName") }}</option><option value="size">{{ t("catalogSize") }}</option></select></div>
  <div class="catalog-table" role="region" :aria-label="t('catalogTitle')" tabindex="0">
   <div class="catalog-head"><span>{{ t("catalogName") }}</span><span>{{ t("catalogSize") }}</span></div>
   <div v-for="{entry,index} in rows" :key="index" class="catalog-item" :class="{chosen:selected.some(r=>r.index===index)}">
    <button v-if="entry.directory" class="catalog-entry" @click="go(entry.parts)"><WorkspaceIcon name="files"/><span>{{ query.trim()?entry.parts.slice(folder.length).join("/"):entry.parts.at(-1) }}</span><span class="catalog-chevron">›</span></button>
    <label v-else class="catalog-entry">
     <input v-if="!inspectOnly" :type="limit===1?'radio':'checkbox'" :checked="selected.some(r=>r.index===index)" :disabled="disabled||(limit===4&&selected.length>=limit&&!selected.some(r=>r.index===index))" @change="choose(index)" :aria-label="entry.parts.join('/')"/>
     <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><path d="M6 3h8l4 4v14H6zM14 3v5h4M9 12h6m-6 4h6"/></svg><span>{{ query.trim()?entry.parts.slice(folder.length).join("/"):entry.parts.at(-1) }}</span>
    </label>
    <span class="catalog-size">{{ entry.directory?"—":amount(entry.size) }}</span>
   </div>
   <p v-if="!rows.length" class="empty-state muted">{{ t("catalogNoEntries") }}</p>
  </div>
  <div class="catalog-caption muted"><span>{{ rows.length }} / {{ entries.length }}</span><span v-if="!inspectOnly">{{ t("catalogSelection") }}: {{ selected.length }}/{{ limit }} · {{ amount(bytes) }}</span></div>
 </div>
</template>
