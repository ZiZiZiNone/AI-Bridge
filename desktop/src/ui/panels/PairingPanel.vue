<template>
  <WindowShell title="配对" no="W9">
    <div class="view">
      <div class="qr" role="img" aria-label="配对二维码">QR</div>

      <section class="fingerprint">
        <h2 class="fingerprint__title">指纹比对</h2>
        <p class="fingerprint__value">本机指纹：—</p>
        <p class="fingerprint__value">对方指纹：—</p>
        <p class="fingerprint__hint">两端指纹一致再确认；不一致立即拒绝。</p>
        <button type="button" class="view__action">确认</button>
        <button type="button" class="view__action">拒绝</button>
      </section>
    </div>
  </WindowShell>
</template>

<script setup lang="ts">
import WindowShell from '../components/WindowShell.vue';
</script>

<style scoped>
.qr {
  display: grid;
  place-items: center;
  width: 160px;
  height: 160px;
  margin: 12px auto 0;
  border: 1px dashed var(--ab-border);
  border-radius: var(--ab-radius);
  background: var(--ab-surface);
  color: var(--ab-text-weak);
  font-size: 24px;
}

.fingerprint {
  margin-top: 16px;
  padding: 12px;
  border: 1px solid var(--ab-border);
  border-radius: var(--ab-radius);
  background: var(--ab-surface);
}

.fingerprint__title {
  margin: 0 0 8px;
  font-size: 13px;
  font-weight: 600;
}

.fingerprint__value {
  margin: 0 0 4px;
  font-family: 'Cascadia Mono', Consolas, monospace;
  font-size: 12px;
  word-break: break-all;
}

.fingerprint__hint {
  margin: 8px 0 0;
  color: var(--ab-text-weak);
  font-size: 12px;
}

.view__action {
  margin: 12px 8px 0 0;
  padding: 4px 12px;
  border: 1px solid var(--ab-border);
  border-radius: 6px;
  background: var(--ab-surface);
  color: var(--ab-text);
  cursor: pointer;
}
</style>
