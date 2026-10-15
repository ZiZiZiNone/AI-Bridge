<template>
  <WindowShell title="库管理" no="W3">
    <div class="view">
      <p class="view__hint">
        A1 条目管理 / A4 多账号与身份档案 / A5 生成器。
        含条目新建与编辑表单屏（全字段 + 账号密码 / passkey / TOTP 类型切换）。
        数据经 tauri command 从 core（ab-vault）取得，本视图只组装与转发。
      </p>
      <button type="button" class="view__action">新建条目</button>
      <p class="view__empty">还没有条目，先新建第一条账号密码</p>
    </div>
  </WindowShell>
</template>

<script setup lang="ts">
import WindowShell from '../components/WindowShell.vue';

/**
 * W3 库管理面板（弹出面板，用完即关）。
 * 骨架：列表区 + 表单屏入口；条目字段与校验规则在 core，UI 不判断。
 */
</script>

<style scoped>
.view__action {
  margin-top: 10px;
  padding: 4px 12px;
  border: 1px solid var(--ab-border);
  border-radius: 6px;
  background: var(--ab-surface);
  color: var(--ab-text);
  cursor: pointer;
}
</style>
