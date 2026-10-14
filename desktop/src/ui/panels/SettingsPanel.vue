<template>
  <WindowShell title="设置" no="W10">
    <div class="view">
      <dl class="settings">
        <template v-for="item in items" :key="item.id">
          <dt>{{ item.label }}</dt>
          <dd>{{ item.value }}</dd>
        </template>
      </dl>
    </div>
  </WindowShell>
</template>

<script setup lang="ts">
import WindowShell from '../components/WindowShell.vue';

interface SettingItem {
  id: string;
  label: string;
  value: string;
}

const items: readonly SettingItem[] = [
  { id: 'unlock', label: '解锁方式', value: '主密码（Argon2id）/ Windows Hello 快速解锁' },
  { id: 'idle-lock', label: '空闲自动锁定', value: '5 分钟（可调）' },
  { id: 'confirm-fill', label: '每次填充前确认', value: '关（偏执档可开）' },
  { id: 'manual-lock', label: '手动锁定', value: '固定快捷键锁定' },
  { id: 'clipboard-clear', label: '剪贴板自动清空', value: '30 秒（可调）' },
  { id: 'hotkey', label: '全局热键', value: 'Ctrl+Alt+P（候选条）' },
];
</script>

<style scoped>
.settings {
  display: grid;
  grid-template-columns: max-content 1fr;
  gap: 6px 12px;
  margin: 12px 0 0;
  font-size: 13px;
}

.settings dt {
  color: var(--ab-text-weak);
}

.settings dd {
  margin: 0;
}
</style>
