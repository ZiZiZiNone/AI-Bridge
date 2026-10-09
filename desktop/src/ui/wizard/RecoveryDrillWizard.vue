<template>
  <WindowShell title="恢复演练" no="W14">
    <div class="view">
      <p class="view__hint">
        一次性窗口（用完即焚）。A6.8：在沙盒中恢复备份并比对条目数与完整性，不改动现有库。
      </p>
      <ol class="steps">
        <li v-for="(step, index) in steps" :key="step" :class="{ 'steps__item--active': index === stepIndex }">
          {{ step }}
        </li>
      </ol>
      <div class="steps__actions">
        <button type="button" class="view__action" :disabled="stepIndex === 0" @click="prev">上一步</button>
        <button v-if="!isLast" type="button" class="view__action" @click="next">下一步</button>
        <button v-else type="button" class="view__action" @click="destroy">完成</button>
      </div>
      <p class="view__empty">演练结果（条目数 / 完整性）将在 core 返回后显示</p>
    </div>
  </WindowShell>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue';
import WindowShell from '../components/WindowShell.vue';
import { useWindowControls } from '../../hooks/useWindowControls';

/**
 * W14 恢复演练向导（一次性窗口）。
 * 承载：A6.8 沙盒恢复与比对。解密与比对在 core；沙盒本身不落明文（S6）。
 */
const steps: readonly string[] = ['选择要演练的备份', '沙盒解密恢复', '比对条目数与完整性'];

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
