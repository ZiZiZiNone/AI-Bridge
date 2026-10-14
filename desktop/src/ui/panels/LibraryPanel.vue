<template>
  <WindowShell title="库管理" no="W3">
    <div class="view">
      <button type="button" class="view__action">新建条目</button>
      <p class="view__empty">还没有条目，先新建第一条账号密码</p>
    </div>
  </WindowShell>
</template>

<script setup lang="ts">
import WindowShell from '../components/WindowShell.vue';
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
