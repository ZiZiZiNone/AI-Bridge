<template>
  <WindowShell title="配对" no="W9">
    <div class="view">
      <p class="view__hint">
        E3 QR 配对 / A8 离线传送包：仅局域网或面对面；对方公钥入码 + 指纹确认。
        配对算法与密钥由 core（ab-sync）负责，本视图只展示 QR 与指纹，并回传确认 / 拒绝意图。
      </p>

      <div class="qr" role="img" aria-label="配对二维码占位（前端无 QR 库，下方为配对码文本）">QR</div>
      <p class="qr__offer">{{ props.offer?.offer_json ?? '配对码生成中…' }}</p>
      <button type="button" class="view__action" @click="emit('retry')">重新加载</button>

      <section class="fingerprint">
        <h2 class="fingerprint__title">指纹比对</h2>
        <p class="fingerprint__value">本机指纹：{{ props.offer?.public_key_hex ?? '（待生成）' }}</p>
        <p class="fingerprint__value">对方指纹：{{ props.peerFingerprint !== '' ? props.peerFingerprint : '（粘贴对端配对码并确认后显示）' }}</p>
        <p class="fingerprint__hint">两端指纹一致再确认；不一致立即拒绝。</p>

        <label class="fingerprint__field">
          <span>对端配对码</span>
          <textarea v-model="peerOfferJson" rows="3" placeholder="粘贴对端展示的配对码（JSON）" />
        </label>

        <button type="button" class="view__action" :disabled="props.isBusy" @click="emit('confirm', peerOfferJson.trim())">
          {{ props.isBusy ? '处理中…' : '确认' }}
        </button>
        <button type="button" class="view__action" @click="emit('reject')">拒绝</button>
      </section>

      <p v-if="props.errorMessage !== ''" class="view__error">{{ props.errorMessage }}</p>
      <p v-if="props.actionMessage !== ''" class="view__ok">{{ props.actionMessage }}</p>
    </div>
  </WindowShell>
</template>

<script setup lang="ts">
import { ref } from 'vue';
import WindowShell from '../components/WindowShell.vue';
import type { OfferView } from '../../types/commands';

/**
 * W9 配对面板（弹出面板）。
 * 受控视图：本机配对载荷经 props 传入，确认 / 拒绝意图经 emit 上报；
 * 公钥 / 指纹由 core（ab-sync）生成，UI 不生成密钥、不做比对判断。
 * QR 图案渲染无依赖库（不新增依赖），以配对码文本如实替代。
 * 对端配对码草稿属「不需保留」白名单，可自持。
 */

interface Props {
  offer?: OfferView | null;
  errorMessage?: string;
  actionMessage?: string;
  peerFingerprint?: string;
  isBusy?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  offer: null,
  errorMessage: '',
  actionMessage: '',
  peerFingerprint: '',
  isBusy: false,
});

const emit = defineEmits<{
  retry: [];
  confirm: [offerJson: string];
  reject: [];
}>();

const peerOfferJson = ref('');
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

.qr__offer {
  margin: 6px 0 0;
  padding: 6px 8px;
  border: 1px solid var(--ab-border);
  border-radius: 6px;
  color: var(--ab-text-weak);
  font-family: 'Cascadia Mono', Consolas, monospace;
  font-size: 11px;
  word-break: break-all;
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

.fingerprint__field {
  display: grid;
  gap: 4px;
  margin-top: 10px;
  font-size: 12px;
}

.fingerprint__field span {
  color: var(--ab-text-weak);
}

.fingerprint__field textarea {
  padding: 4px 8px;
  border: 1px solid var(--ab-border);
  border-radius: 6px;
  background: var(--ab-bg);
  color: var(--ab-text);
  font-family: 'Cascadia Mono', Consolas, monospace;
  font-size: 11px;
  resize: vertical;
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

.view__action:disabled {
  opacity: 0.6;
  cursor: default;
}

.view__error {
  margin: 10px 0 0;
  color: #c0392b;
  font-size: 13px;
}

.view__ok {
  margin: 10px 0 0;
  font-size: 13px;
}
</style>
