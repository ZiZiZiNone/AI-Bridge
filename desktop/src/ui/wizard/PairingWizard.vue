<template>
  <WindowShell title="配对向导" no="W12">
    <div class="view">
      <p class="view__hint">
        一次性窗口（用完即焚）。E3：仅局域网或面对面；QR 展示本机公钥 → 对方扫码 →
        指纹确认（双方比对）。
      </p>
      <ol class="steps">
        <li v-for="(step, index) in steps" :key="step" :class="{ 'steps__item--active': index === stepIndex }">
          {{ step }}
        </li>
      </ol>

      <div v-if="stepIndex === 0" class="qr" role="img" aria-label="配对二维码占位">QR</div>
      <p v-else-if="stepIndex === 1" class="view__empty">等待对方扫码…（局域网 / 面对面）</p>
      <p v-else class="view__empty">指纹一致再确认；指纹由 core（ab-sync）计算</p>

      <div class="steps__actions">
        <button type="button" class="view__action" :disabled="stepIndex === 0" @click="prev">上一步</button>
        <button v-if="!isLast" type="button" class="view__action" @click="next">下一步</button>
        <button v-else type="button" class="view__action" @click="destroy">完成</button>
      </div>
    </div>
  </WindowShell>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue';
import WindowShell from '../components/WindowShell.vue';
import { useWindowControls } from '../../hooks/useWindowControls';

/**
 * W12 配对向导（一次性窗口）。
 * 承载：E3 QR 配对 + 指纹确认步骤。密钥生成与指纹计算在 core（ab-sync），UI 不生成密钥。
 */
const steps: readonly string[] = ['展示本机 QR（公钥入码）', '等待对方扫码与回传', '指纹确认'];

const stepIndex = ref(0);
const isLast = computed(() => stepIndex.value === steps.length - 1);

const { destroy } = useWindowControls();

function prev(): void {
  if (stepIndex.value > 0) {
    stepIndex.value -= 1;
  }
}

function next(): void {
  if (stepIndex.value < steps.length - 1) {
    stepIndex.value += 1;
  }
}
</script>

<style scoped>
.steps {
  margin: 12px 0 0;
  padding-left: 22px;
  font-size: 13px;
}

.steps__item--active {
  font-weight: 600;
  color: var(--ab-accent);
}

.steps__actions {
  margin-top: 12px;
}

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

.view__action {
  margin-right: 8px;
  padding: 4px 12px;
  border: 1px solid var(--ab-border);
  border-radius: 6px;
  background: var(--ab-surface);
  color: var(--ab-text);
  cursor: pointer;
}

.view__action:disabled {
  color: var(--ab-text-weak);
  cursor: not-allowed;
}
</style>
