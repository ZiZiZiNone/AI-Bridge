<template>
  <WindowShell title="初始化向导" no="W11">
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
      <p class="view__empty">恢复密钥只在完成后展示一次，请离线保存</p>
    </div>
  </WindowShell>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue';
import WindowShell from '../components/WindowShell.vue';
import { useWindowControls } from '../../hooks/useWindowControls';

const steps: readonly string[] = [
  '创建加密库（选择库文件位置）',
  '设置主密码（Argon2id 派生的解锁口令）',
  '保存恢复密钥（一次性展示，唯一后门）',
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
