<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";
import type { TextKey } from "./i18n";
import {localeChoices,systemLocale,translate,coverage,type Language} from "./locales";
import LanPanel from "./LanPanel.vue";
import PresencePanel from "./PresencePanel.vue";
import RoutedPanel from "./RoutedPanel.vue";
import ServicePanel from "./ServicePanel.vue";
import RuntimePanel from "./RuntimePanel.vue";
import UpdatePanel from "./UpdatePanel.vue";
import WorkspaceIcon from "./WorkspaceIcon.vue";
import DeviceWorkspace from "./DeviceWorkspace.vue";

type Theme = "system" | "light" | "gray" | "dark" | "black";
type Page = "remote" | "lan" | "routed" | "devices" | "history" | "files" | "server" | "diagnostics" | "service" | "preferences";
type Permission = "granted" | "required" | "system_managed";
type Display = { id: number; name: string; width: number; height: number; primary: boolean };

function preference<T extends string>(key: string, values: readonly T[], fallback: T): T {
  try {
    const value = localStorage.getItem(key);
    return values.includes(value as T) ? value as T : fallback;
  } catch { return fallback; }
}
const language = ref<Language>(preference<Language>("remote-ex.language", ["system",...localeChoices.map(x=>x.id)], "system"));
const theme = ref<Theme>(preference("remote-ex.theme", ["system", "light", "gray", "dark", "black"], "system"));
const active = ref<Page>("remote");
const lanActive = ref(false);
const currentSlot=ref(0),slotActive=ref([false,false,false]);
const routedActive=computed(()=>slotActive.value.some(Boolean));
const serviceActive=ref(false);
const sessionActive=computed(()=>lanActive.value||routedActive.value||serviceActive.value);
const presenceActive = ref(false);
const libraryActive = ref(false);
const inviteDraft=ref(""),initialInvitation=ref(""),initialRole=ref<"host"|"controller">("controller");
const navLocked=computed(()=>busy.value||sessionActive.value||presenceActive.value||libraryActive.value);
const native = isTauri();
const systemLanguage = ref(systemLocale(navigator.languages?.length?navigator.languages:[navigator.language]));
const refreshLanguage=()=>{systemLanguage.value=systemLocale(navigator.languages?.length?navigator.languages:[navigator.language]);};
const locale = computed(() => language.value === "system" ? systemLanguage.value : language.value);
const translationCoverage=computed(()=>coverage(locale.value));
const t = (key: TextKey) => translate(locale.value,key);
const buildLabel=ref("Remote EX");
onMounted(()=>{window.addEventListener("languagechange",refreshLanguage);if(native)void invoke<{version:string;tag:string|null;commit:string|null}>("build_info").then(b=>{buildLabel.value=b.tag??(b.version+" · "+(b.commit?.slice(0,8)??"dev"));}).catch(()=>{});});
onBeforeUnmount(()=>window.removeEventListener("languagechange",refreshLanguage));
watch(locale,value=>{if(native)void invoke("set_viewer_locale",{locale:value}).catch(()=>{});},{immediate:true});
watch([language, theme,locale], ([lang, mode]) => {
  try {
    localStorage.setItem("remote-ex.language", lang);
    localStorage.setItem("remote-ex.theme", mode);
  } catch { /* Private webviews may disable local storage. */ }
  document.documentElement.dataset.theme = mode;
  document.documentElement.lang = localeChoices.find(x=>x.id===locale.value)?.lang??"en";
}, { immediate: true });

const displays = ref<Display[]>([]);
const selected = ref<number | null>(null);
const permission = ref<Permission | null>(null);
const busy = ref(false);
const error = ref("");
const previewUrl = ref("");
const previewSize = ref("");
const permissionText = computed(() => t(permission.value === "granted" ? "permissionGranted" :
  permission.value === "required" ? "permissionRequired" : permission.value === "system_managed" ? "permissionSystem" : "permissionUnknown"));
function clearPreview() {
  if (previewUrl.value) URL.revokeObjectURL(previewUrl.value);
  previewUrl.value = "";
  previewSize.value = "";
}
onBeforeUnmount(clearPreview);
async function operation(task: () => Promise<void>) {
  if (busy.value) return;
  error.value = "";
  if (!native) { error.value = t("browser"); return; }
  busy.value = true;
  try { await task(); } catch (e) {
    error.value = typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
  } finally { busy.value = false; }
}
async function checkPermission(request = false) {
  await operation(async () => {
    permission.value = await invoke<Permission>(request ? "request_capture_permission" : "capture_permission");
  });
}
async function refreshDisplays() {
  await operation(async () => {
    clearPreview();
    displays.value = [];
    selected.value = null;
    permission.value = await invoke<Permission>("capture_permission");
    if (permission.value === "required") return;
    displays.value = await invoke<Display[]>("list_displays");
    selected.value = (displays.value.find(d => d.primary) ?? displays.value[0])?.id ?? null;
    if (!displays.value.length) error.value = t("noDisplays");
  });
}
async function capture() {
  if (selected.value === null) return;
  await operation(async () => {
    clearPreview();
    const data = await invoke<ArrayBuffer>("capture_preview", { displayId: selected.value });
    previewUrl.value = URL.createObjectURL(new Blob([data], { type: "image/png" }));
  });
}
function imageLoaded(event: Event) {
  const image = event.target as HTMLImageElement;
  previewSize.value = image.naturalWidth + " × " + image.naturalHeight;
}
function navigate(page: Page) {
  if (navLocked.value || page===active.value) return;
  initialInvitation.value="";initialRole.value="controller";currentSlot.value=0;
  if (active.value === "diagnostics" && page !== "diagnostics") clearPreview();
  error.value = "";
  active.value = page;
}
const pages: { key: Page; label:TextKey; group:"workspace"|"management" }[] = [
  {key:"remote",label:"remote",group:"workspace"},
  {key:"devices",label:"trustTitle",group:"workspace"},
  {key:"history",label:"deviceHistory",group:"workspace"},
  {key:"files",label:"exchangeFile",group:"workspace"},
  {key:"routed",label:"routed",group:"workspace"},
  {key:"lan",label:"workspaceLan",group:"management"},
  {key:"server",label:"server",group:"management"},
  {key:"service",label:"service",group:"management"},
  {key:"diagnostics",label:"diagnostics",group:"management"},
  {key:"preferences",label:"preferences",group:"management"}
];
const pageTitle=computed(()=>t(pages.find(p=>p.key===active.value)?.label??"remote"));
function openRouted(role:"host"|"controller",useInvite=false){
  if(navLocked.value||useInvite&&(!native||!inviteDraft.value.trim()))return;
  navigate("routed");
  initialRole.value=role;
  if(useInvite){initialInvitation.value=inviteDraft.value.trim();inviteDraft.value="";}
}
</script>

<template>
  <div class="app-shell">
    <aside class="sidebar">
      <header class="brand"><span class="brand-icon"><WorkspaceIcon name="remote"/></span><div><strong>Remote EX</strong><small>{{ t("development") }}</small></div></header>
      <nav :aria-label="t('workspace')">
        <template v-for="group in (['workspace','management'] as const)" :key="group">
          <div class="nav-group">{{ t(group) }}</div>
          <button v-for="page in pages.filter(p=>p.group===group)" :key="page.key" class="nav-item" :class="{ active: active === page.key }"
            :aria-label="t(page.label)" :title="t(page.label)" :aria-current="active === page.key ? 'page' : undefined" :disabled="navLocked && page.key !== active" @click="navigate(page.key)">
            <WorkspaceIcon :name="page.key"/><span>{{ t(page.label) }}</span>
          </button>
        </template>
      </nav>
      <div class="side-foot"><i class="dot" :class="{live:sessionActive||presenceActive}"></i>{{ t(sessionActive?"workspaceSessionActive":presenceActive?"presenceActive":"unavailable") }}</div>
    </aside>
    <main class="content">
      <header class="topbar"><h1>{{ pageTitle }}</h1><span class="status status-pill"><i class="dot" :class="{live:sessionActive||presenceActive}"></i>{{ t(sessionActive ? "workspaceSessionActive" : presenceActive ? "presenceActive" : "unavailable") }}</span></header>
      <div class="body">
        <template v-if="active === 'remote'">
          <div class="workspace-heading"><h2>{{ t("workspaceWelcome") }}</h2><p class="muted">{{ t("workspaceWelcomeNote") }}</p></div>
          <div class="home-grid">
            <section class="panel home-card lan-panel">
              <div class="feature-icon"><WorkspaceIcon name="host"/></div>
              <h2>{{ t("workspaceReceive") }}</h2>
              <p class="muted">{{ t("workspaceReceiveNote") }}</p>
              <div class="actions"><button class="primary" :disabled="navLocked" @click="openRouted('host')">{{ t("workspacePrepareHost") }}</button></div>
              <button class="quiet-link" :disabled="navLocked" @click="navigate('server')">{{ t("presenceTitle") }} <WorkspaceIcon name="chevron"/></button>
            </section>
            <section class="panel home-card lan-panel">
              <div class="feature-icon"><WorkspaceIcon name="remote"/></div>
              <h2>{{ t("workspaceConnect") }}</h2>
              <label for="quick-invite">{{ t("invitationCode") }}</label>
              <textarea id="quick-invite" v-model="inviteDraft" rows="2" maxlength="2048" autocomplete="off" spellcheck="false"/>
              <button class="primary" :disabled="navLocked||!native||!inviteDraft.trim()" @click="openRouted('controller',true)">{{ t("workspaceReviewInvite") }}</button>
              <button class="quiet-link" :disabled="navLocked" @click="openRouted('controller')">{{ t("routedManual") }} <WorkspaceIcon name="chevron"/></button>
            </section>
          </div>
          <section class="panel quick-links">
            <h2>{{ t("workspaceShortcuts") }}</h2>
            <button v-for="item in pages.filter(p=>['devices','history','files','lan'].includes(p.key))" :key="item.key" class="workspace-link" :disabled="navLocked" @click="navigate(item.key)">
              <WorkspaceIcon :name="item.key"/><span>{{ t(item.label) }}</span><WorkspaceIcon name="chevron"/>
            </button>
          </section>
          <p class="workspace-security muted"><WorkspaceIcon name="shield"/>{{ t("invitationNotice") }}</p>
        </template>
        <template v-else-if="active === 'lan'">
          <LanPanel :t="t" @active="lanActive = $event" />
        </template>
        <template v-else-if="active === 'devices'||active === 'history'">
          <DeviceWorkspace :key="active" :section="active" :t="t" @active="libraryActive=$event" @server="navigate('server')"/>
        </template>
        <template v-else-if="active === 'routed'||active === 'files'">
          <div class="actions session-tabs"><button v-for="n in 3" :key="n" :class="{primary:currentSlot===n-1}" @click="currentSlot=n-1">{{ t("sessionTab") }} {{ n }}<span v-if="slotActive[n-1]"> ●</span></button></div>
          <p class="muted">{{ t("multiSessionNotice") }}</p>
          <RoutedPanel v-for="n in 3" v-show="currentSlot===n-1" :key="active+'-'+n" :slot="n-1" :visible="currentSlot===n-1" :t="t" :initial-mode="active==='files'?'file':'view'" :initial-role="n===1?initialRole:'controller'" :initial-invitation="n===1?initialInvitation:''" @active="slotActive[n-1]=$event" />
        </template>
        <template v-else-if="active === 'server'">
          <PresencePanel :t="t" @active="presenceActive = $event" />
        </template>
        <template v-else-if="active === 'diagnostics'">
          <RuntimePanel :t="t" :blocked="busy" @active="busy=$event" />
          <section class="panel">
            <h2>{{ t("diagTitle") }}</h2><p>{{ t("diagDescription") }}</p>
            <div class="field">
              <label>{{ t("permission") }}</label><span class="muted">{{ permissionText }}</span>
              <div class="actions">
                <button :disabled="busy" @click="checkPermission()">{{ t("checkPermission") }}</button>
                <button :disabled="busy" @click="checkPermission(true)">{{ t("requestPermission") }}</button>
              </div>
            </div>
            <div class="field">
              <label for="display-select">{{ t("display") }}</label>
              <div class="actions">
                <select id="display-select" v-model="selected" :disabled="busy || displays.length === 0" @change="clearPreview">
                  <option :value="null" disabled>{{ t("select") }}</option>
                  <option v-for="d in displays" :key="d.id" :value="d.id">{{ d.name }} · {{ d.width }} × {{ d.height }}{{ d.primary ? " · " + t("primary") : "" }}</option>
                </select>
                <button :disabled="busy" @click="refreshDisplays">{{ t("refresh") }}</button>
              </div>
              <div class="actions">
                <button class="primary" :disabled="busy || selected === null" @click="capture">{{ busy ? t("busy") : t("capture") }}</button>
                <button :disabled="busy || !previewUrl" @click="clearPreview">{{ t("clear") }}</button>
              </div>
            </div>
            <p v-if="error" class="error" role="alert">{{ t("error") }}: {{ error }}</p>
          </section>
          <section class="panel">
            <header class="preview-head"><h2>{{ t("preview") }}</h2><span class="muted">{{ previewSize }}</span></header>
            <div class="preview">
              <img v-if="previewUrl" :src="previewUrl" :alt="t('preview')" @load="imageLoaded" />
              <span v-else class="muted">{{ t("previewEmpty") }}</span>
            </div>
          </section>
        </template>
        <template v-else-if="active === 'service'">
          <ServicePanel :t="t" @active="serviceActive=$event" />
        </template>
        <template v-else>
          <section class="panel">
            <div class="preference-row"><label for="language">{{ t("language") }}</label>
              <select id="language" v-model="language"><option value="system">{{ t("system") }}</option><option v-for="choice in localeChoices" :key="choice.id" :value="choice.id">{{ choice.label }}</option></select>
            </div>
            <div class="preference-row"><label for="theme">{{ t("appearance") }}</label>
              <select id="theme" v-model="theme"><option value="system">{{ t("system") }}</option><option value="light">{{ t("light") }}</option><option value="gray">{{ t("workspaceGray") }}</option><option value="dark">{{ t("dark") }}</option><option value="black">{{ t("black") }}</option></select>
            </div>
            <p v-if="translationCoverage.translated!==translationCoverage.total" class="muted">{{ t("localePartial") }} {{ t("localeCoverage") }}: {{ translationCoverage.translated }}/{{ translationCoverage.total }}</p>
            <p class="muted">{{ t("saved") }}</p>
          </section>
          <UpdatePanel :t="t" :blocked="busy" @active="busy=$event" />
        </template>
      </div>
      <footer class="footer">{{ buildLabel }}<span>{{ t("noRemote") }}</span></footer>
    </main>
  </div>
</template>
