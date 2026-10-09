<template>
  <WindowShell title="备份" no="W7">
    <div class="view">
      <p class="view__hint">
        A6 备份与恢复。全部备份强制加密（A6.4），本视图只呈现状态与开关，
        加密、快照保留策略、自动校验规则均在 core。
      </p>

      <dl class="facts">
        <dt>最后验证时间</dt>
        <dd>{{ lastVerifiedAt }}</dd>
        <dt>托管目录</dt>
        <dd class="facts__path">（未设置，建议放外接盘或异盘，防勒索连锅端）</dd>
        <dt>定时备份</dt>
        <dd><label><input v-model="scheduled" type="checkbox" /> 默认月度，频率可调</label></dd>
      </dl>

      <button type="button" class="view__action">立即备份</button>
      <button type="button" class="view__action">恢复演练</button>
      <button type="button" class="view__action">从备份恢复</button>
    </div>
  </WindowShell>
</template>

<script setup lang="ts">
import { ref } from 'vue';
import WindowShell from '../components/WindowShell.vue';

/**
 * W7 备份面板（弹出面板）。
 * 承载：A6.1 手动备份、A6.5 托管目录、A6.6 滚动保留、A6.7 自动校验、A6.8 恢复演练、A6.9 恢复导入。
 * 骨架期未接 core：验证时间与开关为占位（开关的持久化与生效规则在 core，UI 不判断）。
 */
const lastVerifiedAt = ref('（未接入）');
const scheduled = ref(false);
</script>

<style scoped>
.facts {
  display: grid;
  grid-template-columns: max-content 1fr;
  gap: 6px 12px;
  margin: 12px 0 0;
  font-size: 13px;
}

.facts dt {
  color: var(--ab-text-weak);
}

.facts dd {
  margin: 0;
}

.facts__path {
  color: var(--ab-text-weak);
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
