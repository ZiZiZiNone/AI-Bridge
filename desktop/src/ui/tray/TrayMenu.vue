<template>
  <section class="view">
    <header class="view__head">
      <h1 class="view__title">托盘图标 + 托盘菜单</h1>
      <span class="view__no">W1</span>
    </header>
    <ul class="tray">
      <li v-for="item in menuItems" :key="item.id" class="tray__item">
        <span>{{ item.label }}</span>
        <code class="tray__id">{{ item.id }}</code>
      </li>
    </ul>
  </section>
</template>

<script setup lang="ts">
interface TrayMenuItem {
  id: string;
  label: string;
}

const menuItems: readonly TrayMenuItem[] = [
  { id: 'unlock-lock', label: '解锁 / 锁定' },
  { id: 'quick-fill', label: '快捷填充' },
  { id: 'library', label: '库' },
  { id: 'status', label: '状态清单' },
  { id: 'sessions', label: '临时会话' },
  { id: 'audit', label: '审计' },
  { id: 'backup', label: '备份' },
  { id: 'converters', label: '转换器' },
  { id: 'pairing', label: '配对' },
  { id: 'settings', label: '设置' },
  { id: 'quit', label: '退出' },
];
</script>

<style scoped>
.tray {
  margin: 12px 0 0;
  padding: 0;
  list-style: none;
  border: 1px solid var(--ab-border);
  border-radius: var(--ab-radius);
  background: var(--ab-surface);
}

.tray__item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--ab-border);
}

.tray__item:last-child {
  border-bottom: none;
}

.tray__id {
  color: var(--ab-text-weak);
  font-size: 12px;
}
</style>
