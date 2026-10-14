<template>
  <section class="candidate">
    <header class="candidate__head">
      <span class="candidate__brand">AI-Bridge</span>
      <input
        v-model="keyword"
        class="candidate__search"
        type="search"
        placeholder="搜索账号 / 站点"
        aria-label="搜索账号或站点"
      />
      <span class="candidate__window">当前窗口：{{ foreground }}</span>
    </header>

    <p class="candidate__empty">未找到相关内容，试试其他关键词</p>

    <footer class="candidate__foot">
      <span>热键 Ctrl+Alt+P</span>
      <span>TOTP 实时显示 · 一次性复制（30 秒后自动清空）</span>
    </footer>
  </section>
</template>

<script setup lang="ts">
import { ref } from 'vue';

// W2 候选条
const keyword = ref('');
const foreground = ref('—');
</script>

<style scoped>
.candidate {
  display: flex;
  flex-direction: column;
  height: 100%;
  border: 1px solid var(--ab-border);
  border-radius: var(--ab-radius);
  background: var(--ab-surface);
  overflow: hidden;
}

.candidate__head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  border-bottom: 1px solid var(--ab-border);
}

.candidate__brand {
  font-size: 12px;
  font-weight: 600;
  color: var(--ab-accent);
}

.candidate__search {
  flex: 1;
  padding: 4px 8px;
  border: 1px solid var(--ab-border);
  border-radius: 6px;
  background: var(--ab-bg);
  color: var(--ab-text);
}

.candidate__window {
  font-size: 12px;
  color: var(--ab-text-weak);
}

.candidate__empty {
  flex: 1;
  margin: 0;
  padding: 24px;
  color: var(--ab-text-weak);
  text-align: center;
}

.candidate__foot {
  display: flex;
  justify-content: space-between;
  padding: 6px 10px;
  border-top: 1px solid var(--ab-border);
  color: var(--ab-text-weak);
  font-size: 12px;
}
</style>
