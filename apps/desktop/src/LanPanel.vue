<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref, watch } from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";
import type { TextKey } from "./i18n";

type Stage = "idle" | "starting" | "awaiting_approval" | "connecting" | "listening" | "connected" | "passed" | "failed" | "canceled";
type Snapshot = { stage: Stage; active: boolean; local_key: string | null; detail: string | null };
const props = defineProps<{ t: (key: TextKey) => string }>();
const emit = defineEmits<{ active: [value: boolean] }>();
const role = ref<"host" | "controller">("controller");
const address = ref("127.0.0.1:47821");
const preview = ref(false);
const peer = ref("");
const confirmed = ref(false);
const snapshot = ref<Snapshot>({ stage: "idle", active: false, local_key: null, detail: null });
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
  passed: "lanPassed", failed: "lanFailed", canceled: "lanCanceled",
};
const statusLabel = computed(() => props.t(statusKeys[snapshot.value.stage]));
const canApprove = computed(() => confirmed.value && /^[a-fA-F0-9]{64}$/.test(peer.value.trim())
  && peer.value.trim().toLowerCase() !== snapshot.value.local_key && !approvalSent.value);
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
  return action("lan_prepare", { role: role.value, address: address.value, preview: role.value === "controller" && preview.value });
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
    <p>{{ t("lanDescription") }}</p>
    <div class="field">
      <label for="lan-role">{{ t("lanRole") }}</label>
      <select id="lan-role" v-model="role" :disabled="snapshot.active || busy">
        <option value="controller">{{ t("lanController") }}</option><option value="host">{{ t("lanHost") }}</option>
      </select>
      <label for="lan-address">{{ t("lanAddress") }}</label>
      <input id="lan-address" v-model="address" placeholder="192.168.1.10:47821" maxlength="64" :disabled="snapshot.active || busy" />
      <label v-if="role === 'controller'" class="check-label"><input type="checkbox" v-model="preview" :disabled="snapshot.active || busy" />{{ t("lanPreview") }}</label>
      <div class="actions">
        <button class="primary" :disabled="!native || busy || snapshot.active || !address.trim()" @click="prepare">{{ t("lanPrepare") }}</button>
        <button :disabled="!snapshot.active || busy" @click="action('lan_disconnect')">{{ t("lanDisconnect") }}</button>
      </div>
    </div>
    <div v-if="snapshot.local_key" class="field">
      <label for="local-key">{{ t("lanLocalKey") }}</label>
      <input id="local-key" :value="snapshot.local_key" readonly class="key-input" />
      <template v-if="snapshot.stage === 'awaiting_approval'">
        <p class="muted">{{ t("lanVerify") }}</p>
        <label for="peer-key">{{ t("lanPeerKey") }}</label>
        <input id="peer-key" v-model="peer" maxlength="64" class="key-input" spellcheck="false" autocomplete="off" :disabled="busy || approvalSent" />
        <label class="check-label"><input type="checkbox" v-model="confirmed" :disabled="busy || approvalSent" />{{ t("lanConsent") }}</label>
        <div class="actions"><button class="primary" :disabled="busy || !canApprove" @click="approve">{{ t("lanApprove") }}</button></div>
      </template>
    </div>
    <p v-if="error || snapshot.detail" class="error" role="alert">{{ t("error") }}: {{ error || snapshot.detail }}</p>
    <p v-if="!native" class="muted">{{ t("browser") }}</p>
  </section>
</template>
