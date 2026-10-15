<template>
  <WindowShell title="配对" no="W9">
    <div class="view">
      <p class="view__hint">
        E3 QR 配对 / A8 离线传送包：仅局域网或面对面；对方公钥入码 + 指纹确认。
        配对算法与密钥由 core（ab-sync）负责，本视图只展示 QR 与指纹，并回传确认 / 拒绝意图。
      </p>

      <div class="qr" role="img" aria-label="配对二维码占位">QR</div>

      <section class="fingerprint">
        <h2 class="fingerprint__title">指纹比对</h2>
        <p class="fingerprint__value">本机指纹：（未接入）</p>
        <p class="fingerprint__value">对方指纹：（未接入）</p>
        <p class="fingerprint__hint">两端指纹一致再确认；不一致立即拒绝。</p>
        <button type="button" class="view__action">确认</button>
        <button type="button" class="view__action">拒绝</button>
      </section>
    </div>
  </WindowShell>
</template>

<script setup lang="ts">
import WindowShell from '../components/WindowShell.vue';

/**
 * W9 配对面板（弹出面板）。
 * 骨架：QR 展示区 + 指纹确认屏；公钥 / 指纹由 core 注入，UI 不生成密钥、不做比对判断。
 */
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
