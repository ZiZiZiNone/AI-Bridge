<template>
  <WindowShell title="备份" no="W7">
    <div class="view">
      <dl class="facts">
        <dt>最后验证时间</dt>
        <dd>{{ lastVerifiedAt }}</dd>
        <dt>托管目录</dt>
        <dd class="facts__path">建议放置于外接盘或异盘</dd>
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

const lastVerifiedAt = ref('—');
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
