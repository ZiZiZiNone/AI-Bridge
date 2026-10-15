<template>
  <WindowShell title="设置" no="W10">
    <div class="view">
      <p class="view__hint">
        S1 解锁 / S2 就绪态与空闲锁 / S4 每次填充前确认 / S7 手动锁定与快捷键 /
        B5.2 剪贴板清空 / B6.1 全局热键。所有开关的生效规则在 core，本视图只呈现与回传。
      </p>

      <dl class="settings">
        <template v-for="item in items" :key="item.id">
          <dt>{{ item.label }}</dt>
          <dd>{{ item.value }}</dd>
        </template>
      </dl>

      <p class="view__empty">设置项将在 core 暴露配置接口后接入</p>
    </div>
  </WindowShell>
</template>

<script setup lang="ts">
import WindowShell from '../components/WindowShell.vue';

/**
 * W10 设置面板（弹出面板）。
 * 骨架：仅列设置项口径与默认值文案，不写任何业务判断（默认值定义在 core 与 docs）。
 */
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
