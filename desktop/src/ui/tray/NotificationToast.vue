<template>
  <section class="view">
    <header class="view__head">
      <h1 class="view__title">通知 toast</h1>
      <span class="view__no">W15</span>
    </header>
    <ul class="toast">
      <li v-for="item in templates" :key="item.id" class="toast__item">
        <strong class="toast__group">{{ item.group }}</strong>
        <span>{{ item.text }}</span>
      </li>
    </ul>
  </section>
</template>

<script setup lang="ts">
interface ToastTemplate {
  id: string;
  group: string;
  text: string;
}

const templates: readonly ToastTemplate[] = [
  { id: 'session-expiring', group: 'A3.1', text: '会话即将到期，可一键重登' },
  { id: 'backup-done', group: 'A6', text: '备份已完成并自动校验通过' },
  { id: 'broker-revoked', group: 'C5.1', text: '临时会话已释放 / 已吊销' },
];
</script>

<style scoped>
.toast {
  margin: 12px 0 0;
  padding: 0;
  list-style: none;
  border: 1px solid var(--ab-border);
  border-radius: var(--ab-radius);
  background: var(--ab-surface);
}

.toast__item {
  display: flex;
  gap: 8px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--ab-border);
}

.toast__item:last-child {
  border-bottom: none;
}

.toast__group {
  color: var(--ab-accent);
  font-size: 12px;
}
</style>
