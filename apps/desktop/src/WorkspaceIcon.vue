<script setup lang="ts">
defineProps<{name:string}>();
</script>
<template>
<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" class="workspace-icon">
  <g v-if="name==='remote'||name==='routed'"><rect x="3" y="4" width="18" height="13" rx="2"/><path d="M8 21h8m-4-4v4M8 10h8m-3-3 3 3-3 3"/></g>
  <g v-else-if="name==='host'||name==='service'"><rect x="3" y="4" width="18" height="13" rx="2"/><path d="M8 21h8m-4-4v4m-3-11 3-3 3 3m-3-3v7"/></g>
  <g v-else-if="name==='devices'"><rect x="2.5" y="4" width="16" height="11" rx="2"/><path d="M7 19h7m-3.5-4v4"/><rect x="16" y="11" width="5.5" height="10" rx="1.3" fill="var(--panel)"/></g>
  <g v-else-if="name==='history'"><path d="M3 11a9 9 0 1 1 2 7M3 5v6h6M12 7v5l3 2"/></g>
  <g v-else-if="name==='files'"><path d="M3 7a2 2 0 0 1 2-2h5l2 3h7a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2zM8 14h8m-3-3 3 3-3 3"/></g>
  <g v-else-if="name==='server'"><rect x="3" y="4" width="18" height="6.5" rx="1.5"/><rect x="3" y="13.5" width="18" height="6.5" rx="1.5"/><path d="M7 7.2h.1m-.1 9.5h.1M11 7.2h6m-6 9.5h6"/></g>
  <g v-else-if="name==='lan'"><rect x="9" y="3" width="6" height="5" rx="1"/><rect x="2" y="16" width="6" height="5" rx="1"/><rect x="16" y="16" width="6" height="5" rx="1"/><path d="M12 8v4M5 16v-4h14v4"/></g>
  <g v-else-if="name==='diagnostics'"><rect x="3" y="4" width="18" height="16" rx="2"/><path d="M6 13h3l2-5 3 8 2-3h2"/></g>
  <g v-else-if="name==='preferences'"><path d="M10 3h4l.6 3 2 .9 2.6-1.3 2 3.5-2 2.2v2l2 2.2-2 3.5-2.6-1.3-2 .9L14 21h-4l-.6-3-2-.9-2.6 1.3-2-3.5 2-2.2v-2l-2-2.2 2-3.5L7.4 7l2-.9z"/><circle cx="12" cy="12" r="3"/></g>
  <g v-else-if="name==='chevron'"><path d="m9 6 6 6-6 6"/></g>
  <g v-else><path d="M12 22s8-3.5 8-10V5l-8-3-8 3v7c0 6.5 8 10 8 10zM9 12l2 2 4-4"/></g>
</svg>
</template>
