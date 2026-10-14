<template>
  <WindowShell title="恢复向导" no="W13">
    <div class="view">
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
      <p class="view__empty">选择备份文件或离线传送包（.aibvault）后开始恢复</p>
    </div>
  </WindowShell>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue';
import WindowShell from '../components/WindowShell.vue';
import { useWindowControls } from '../../hooks/useWindowControls';

const steps: readonly string[] = [
  '选择备份文件 / 离线传送包',
  '输入恢复密钥（本机加密副本）',
  '校验时间戳与签名（过期即拒解）',
  '恢复并比对条目数',
];

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
