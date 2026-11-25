<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { en, zh, type TextKey } from "./i18n";
import LanPanel from "./LanPanel.vue";
import PresencePanel from "./PresencePanel.vue";
import RoutedPanel from "./RoutedPanel.vue";
import ServicePanel from "./ServicePanel.vue";

type Language = "system" | "en" | "zh";
type Theme = "system" | "light" | "dark" | "black";
type Page = "remote" | "routed" | "server" | "diagnostics" | "service" | "preferences";
type Permission = "granted" | "required" | "system_managed";
type Display = { id: number; name: string; width: number; height: number; primary: boolean };

function preference<T extends string>(key: string, values: readonly T[], fallback: T): T {
  try {
    const value = localStorage.getItem(key);
    return values.includes(value as T) ? value as T : fallback;
  } catch { return fallback; }
}
const language = ref<Language>(preference("remote-ex.language", ["system", "en", "zh"], "system"));
const theme = ref<Theme>(preference("remote-ex.theme", ["system", "light", "dark", "black"], "system"));
const active = ref<Page>("remote");
const lanActive = ref(false);
const currentSlot=ref(0),slotActive=ref([false,false,false]);
const routedActive=computed(()=>slotActive.value.some(Boolean));
const sessionActive=computed(()=>lanActive.value||routedActive.value);
const presenceActive = ref(false);
const native = isTauri();
const systemLanguage = navigator.language.toLowerCase().startsWith("zh") ? "zh" : "en";
const locale = computed(() => language.value === "system" ? systemLanguage : language.value);
const t = (key: TextKey) => (locale.value === "zh" ? zh : en)[key];
watch([language, theme], ([lang, mode]) => {
  try {
    localStorage.setItem("remote-ex.language", lang);
    localStorage.setItem("remote-ex.theme", mode);
  } catch { /* Private webviews may disable local storage. */ }
  document.documentElement.dataset.theme = mode;
  document.documentElement.lang = locale.value;
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
  if (busy.value || sessionActive.value || presenceActive.value) return;
  if (active.value === "diagnostics" && page !== "diagnostics") clearPreview();
  error.value = "";
  active.value = page;
}
const pages: { key: Page; symbol: string }[] = [
  { key: "remote", symbol: "▣" }, { key: "routed", symbol: "↔" }, { key: "server", symbol: "◎" }, { key: "diagnostics", symbol: "◫" }, { key: "service", symbol: "◉" }, { key: "preferences", symbol: "⚙" }
];
</script>

<template>
  <div class="app-shell">
    <aside class="sidebar">
      <header class="brand"><span class="brand-icon">▣</span><div><strong>Remote EX</strong><small>{{ t("development") }}</small></div></header>
      <nav aria-label="Navigation">
        <button v-for="page in pages" :key="page.key" class="nav-item" :class="{ active: active === page.key }"
          :aria-current="active === page.key ? 'page' : undefined" :disabled="busy || ((sessionActive || presenceActive) && page.key !== active)" @click="navigate(page.key)">
          <span aria-hidden="true" class="nav-icon">{{ page.symbol }}</span>{{ t(page.key) }}
        </button>
      </nav>
      <div class="side-foot"><i class="dot"></i>{{ t("localOnly") }}</div>
    </aside>
    <main class="content">
      <header class="topbar"><h1>{{ t(active) }}</h1><span class="status">{{ t(sessionActive ? "lanActive" : presenceActive ? "presenceActive" : "unavailable") }}</span></header>
      <div class="body">
        <template v-if="active === 'remote'">
          <LanPanel :t="t" @active="lanActive = $event" />
        </template>
        <template v-else-if="active === 'routed'">
          <div class="actions session-tabs"><button v-for="n in 3" :key="n" :class="{primary:currentSlot===n-1}" @click="currentSlot=n-1">{{ t("sessionTab") }} {{ n }}<span v-if="slotActive[n-1]"> ●</span></button></div>
          <p class="muted">{{ t("multiSessionNotice") }}</p>
          <RoutedPanel v-for="n in 3" v-show="currentSlot===n-1" :key="n"  :slot="n-1" :visible="currentSlot===n-1" :t="t" @active="slotActive[n-1]=$event" />
        </template>
        <template v-else-if="active === 'server'">
          <PresencePanel :t="t" @active="presenceActive = $event" />
        </template>
        <template v-else-if="active === 'diagnostics'">
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
          <ServicePanel :t="t" />
        </template>
        <template v-else>
          <section class="panel">
            <div class="preference-row"><label for="language">{{ t("language") }}</label>
              <select id="language" v-model="language"><option value="system">{{ t("system") }}</option><option value="zh">简体中文</option><option value="en">English</option></select>
            </div>
            <div class="preference-row"><label for="theme">{{ t("appearance") }}</label>
              <select id="theme" v-model="theme"><option value="system">{{ t("system") }}</option><option value="light">{{ t("light") }}</option><option value="dark">{{ t("dark") }}</option><option value="black">{{ t("black") }}</option></select>
            </div>
            <p class="muted">{{ t("saved") }}</p>
          </section>
        </template>
      </div>
      <footer class="footer">{{ t("version") }}<span>{{ t("noRemote") }}</span></footer>
    </main>
  </div>
</template>
