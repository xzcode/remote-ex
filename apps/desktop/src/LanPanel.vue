<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref, watch } from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";
import type { TextKey } from "./i18n";

type Stage = "idle" | "starting" | "awaiting_approval" | "connecting" | "listening" | "connected" | "stopping" | "passed" | "failed" | "canceled";
type Profile = "diagnostic" | "secure_test" | "screen" | "control";
type Display = { id: number; name: string; width: number; height: number; primary: boolean };
type Snapshot = { stage: Stage; active: boolean; local_key: string | null; detail: string | null; profile: Profile; input_released: boolean };
const props = defineProps<{ t: (key: TextKey) => string }>();
const emit = defineEmits<{ active: [value: boolean] }>();
const role = ref<"host" | "controller">("controller");
const profile = ref<Profile>("screen");
const displays = ref<Display[]>([]);
const selectedDisplay = ref<number | null>(null);
const address = ref("127.0.0.1:47821");
const preview = ref(false);
const peer = ref("");
const confirmed = ref(false);
const snapshot = ref<Snapshot>({ stage: "idle", active: false, local_key: null, detail: null, profile: "diagnostic", input_released: false });
const busy = ref(false);
const approvalSent = ref(false);
const error = ref("");
const native = isTauri();
let timer: number | undefined;
let pollBusy = false;
let disposed = false;
let revision = 0;
const statusKeys: Record<Stage, TextKey> = {
  idle: "lanIdle", starting: "lanStarting", awaiting_approval: "lanAwaiting",
  connecting: "lanConnecting", listening: "lanListening", connected: "lanConnected",
  stopping: "controlStopping", passed: "lanPassed", failed: "lanFailed", canceled: "lanCanceled",
};
const statusLabel = computed(() => {
  if (snapshot.value.profile === "control" && snapshot.value.stage === "connected") return props.t("controlConnected");
  if (snapshot.value.profile === "control" && snapshot.value.stage === "passed") return props.t("controlEnded");
  if (snapshot.value.profile !== "diagnostic" && snapshot.value.stage === "connected") return props.t("viewConnected");
  if (snapshot.value.profile === "screen" && snapshot.value.stage === "passed") return props.t("viewEnded");
  return props.t(statusKeys[snapshot.value.stage]);
});
const keyLength = computed(() => snapshot.value.profile === "diagnostic" ? 64 : 128);
const canApprove = computed(() => confirmed.value && peer.value.trim().length === keyLength.value && /^[a-fA-F0-9]+$/.test(peer.value.trim())
  && peer.value.trim().toLowerCase() !== snapshot.value.local_key && !approvalSent.value);
const realProfile = computed(() => profile.value === "screen" || profile.value === "control");
const canPrepare = computed(() => native && !busy.value && !snapshot.value.active && !!address.value.trim()
  && (!realProfile.value || role.value !== "host" || selectedDisplay.value !== null));
const description = computed(() => props.t(profile.value === "control" ? "controlDescription" : profile.value === "diagnostic" ? "lanDescription" : profile.value === "secure_test" ? "secureTestDescription" : "viewDescription"));
const consentLabel = computed(() => props.t(snapshot.value.profile === "control" ? (role.value === "host" ? "controlHostConsent" : "controlControllerConsent") : snapshot.value.profile === "diagnostic" ? "lanConsent" : snapshot.value.profile === "secure_test" ? "secureTestConsent" : role.value === "host" ? "viewHostConsent" : "viewControllerConsent"));
async function loadDisplays() {
  if (!native || busy.value || snapshot.value.active) return;
  busy.value = true;
  error.value = "";
  selectedDisplay.value = null;
  displays.value = [];
  try {
    displays.value = await invoke<Display[]>("list_displays");
    if (profile.value === "control") displays.value = displays.value.filter(d => d.primary);
    if (!displays.value.length) error.value = props.t("noDisplays");
  } catch (e) { error.value = String(e); }
  finally { busy.value = false; }
}
watch([profile, role], () => { selectedDisplay.value = null; displays.value = []; });
watch(() => snapshot.value.active || busy.value, value => emit("active", value), { immediate: true });
watch(() => snapshot.value.stage, stage => {
  if (stage !== "awaiting_approval") { confirmed.value = false; peer.value = ""; approvalSent.value = false; }
});
async function poll() {
  if (!native || disposed || busy.value || pollBusy) return;
  pollBusy = true;
  const version = revision;
  try {
    const next = await invoke<Snapshot>("lan_status");
    if (!disposed && version === revision) snapshot.value = next;
  } catch (e) { if (!disposed) error.value = String(e); }
  finally { pollBusy = false; }
}
async function action(command: string, args: Record<string, unknown> = {}) {
  if (!native || busy.value) return;
  busy.value = true;
  revision++;
  error.value = "";
  try { snapshot.value = await invoke<Snapshot>(command, args); }
  catch (e) { error.value = String(e); if (command === "lan_confirm") approvalSent.value = false; }
  finally { busy.value = false; }
}
function prepare() {
  peer.value = "";
  confirmed.value = false;
  approvalSent.value = false;
  return action("lan_prepare", { role: role.value, address: address.value,
    preview: role.value === "controller" && preview.value,
    profile: profile.value, displayId: realProfile.value && role.value === "host" ? selectedDisplay.value : null });
}
function approve() {
  if (!canApprove.value || busy.value) return;
  approvalSent.value = true;
  return action("lan_confirm", { peerKey: peer.value.trim(), approved: confirmed.value });
}
onMounted(() => { void poll(); timer = window.setInterval(poll, 400); });
onBeforeUnmount(() => {
  disposed = true;
  if (timer !== undefined) window.clearInterval(timer);
  // Navigation is locked while active; if the component is nevertheless
  // destroyed, stop the known diagnostic process. Native window exit also
  // stops it independently of frontend lifecycle.
  if (native && snapshot.value.active) void invoke("lan_disconnect").catch(() => {});
});
</script>

<template>
  <section class="panel lan-panel">
    <header class="preview-head"><h2>{{ t("lanHeading") }}</h2><span class="status" aria-live="polite">{{ statusLabel }}</span></header>
    <p>{{ description }}</p>
    <div v-if="snapshot.active && snapshot.profile === 'screen'" class="view-notice" role="status">
      {{ t(role === "host" ? "viewHostNotice" : "viewControllerNotice") }}
    </div>
    <div v-if="snapshot.active && snapshot.profile === 'control'" class="view-notice" role="status">
      {{ t(role === "host" ? "controlHostNotice" : "controlControllerNotice") }}
    </div>
    <div class="field">
      <label for="lan-profile">{{ t("sessionProfile") }}</label>
      <select id="lan-profile" v-model="profile" :disabled="snapshot.active || busy">
        <option value="screen">{{ t("screenView") }}</option>
        <option value="control">{{ t("controlMode") }}</option>
        <option value="secure_test">{{ t("secureTest") }}</option>
        <option value="diagnostic">{{ t("lanHeading") }}</option>
      </select>
      <label for="lan-role">{{ t("lanRole") }}</label>
      <select id="lan-role" v-model="role" :disabled="snapshot.active || busy">
        <option value="controller">{{ t("lanController") }}</option><option value="host">{{ t("lanHost") }}</option>
      </select>
      <label for="lan-address">{{ t("lanAddress") }}</label>
      <input id="lan-address" v-model="address" placeholder="192.168.1.10:47821" maxlength="64" :disabled="snapshot.active || busy" />
      <template v-if="realProfile && role === 'host'">
        <label for="share-display">{{ t("display") }}</label>
        <div class="actions">
          <select id="share-display" v-model="selectedDisplay" :disabled="snapshot.active || busy">
            <option :value="null" disabled>{{ t("select") }}</option>
            <option v-for="display in displays" :key="display.id" :value="display.id">{{ display.name }} · {{ display.width }} × {{ display.height }}</option>
          </select>
          <button :disabled="snapshot.active || busy" @click="loadDisplays">{{ t("refresh") }}</button>
        </div>
        <p class="muted">{{ t(profile === "control" ? "controlPermission" : "viewPermission") }}</p>
      </template>
      <label v-if="role === 'controller' && profile === 'diagnostic'" class="check-label"><input type="checkbox" v-model="preview" :disabled="snapshot.active || busy" />{{ t("lanPreview") }}</label>
      <div class="actions">
        <button class="primary" :disabled="!canPrepare" @click="prepare">{{ t(profile === "control" ? "controlPrepare" : profile === "screen" ? "viewPrepare" : "lanPrepare") }}</button>
        <button :disabled="!snapshot.active || busy || snapshot.stage === 'stopping'" @click="action('lan_disconnect')">{{ t("lanDisconnect") }}</button>
      </div>
    </div>
    <div v-if="snapshot.local_key" class="field">
      <label for="local-key">{{ t("lanLocalKey") }}</label>
      <input id="local-key" :value="snapshot.local_key" readonly class="key-input" />
      <template v-if="snapshot.stage === 'awaiting_approval'">
        <p class="muted">{{ t("lanVerify") }}</p>
        <label for="peer-key">{{ t("lanPeerKey") }}</label>
        <input id="peer-key" v-model="peer"  :maxlength="keyLength" class="key-input" spellcheck="false" autocomplete="off" :disabled="busy || approvalSent" />
        <label class="check-label"><input type="checkbox" v-model="confirmed" :disabled="busy || approvalSent" />{{ consentLabel }}</label>
        <div class="actions"><button class="primary" :disabled="busy || !canApprove" @click="approve">{{ t("lanApprove") }}</button></div>
      </template>
    </div>
    <p v-if="snapshot.profile === 'control' && snapshot.input_released" class="muted">{{ t("controlReleased") }}</p>
    <p v-if="error || snapshot.detail" class="error" role="alert">{{ t("error") }}: {{ error || snapshot.detail }}</p>
    <p v-if="!native" class="muted">{{ t("browser") }}</p>
  </section>
</template>
