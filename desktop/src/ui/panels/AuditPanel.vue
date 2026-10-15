<template>
  <WindowShell title="审计" no="W6">
    <div class="view">
      <p class="view__hint">
        C3 审计流：谁 / 何时 / 账号 / 域名 / 动作 / 结果；筛选与加密导出。
        审计记录由 core（ab-audit）追加式写入，只读呈现；UI 不修改历史。
      </p>

      <table class="audit">
        <thead>
          <tr>
            <th>时间</th>
            <th>主体</th>
            <th>账号</th>
            <th>域名</th>
            <th>动作</th>
            <th>结果</th>
          </tr>
        </thead>
        <tbody />
      </table>

      <p class="view__empty">暂无审计记录</p>

      <button type="button" class="view__action">筛选</button>
      <button type="button" class="view__action">加密导出</button>
    </div>
  </WindowShell>
</template>

<script setup lang="ts">
import WindowShell from '../components/WindowShell.vue';

/**
 * W6 审计面板（弹出面板）。
 * 骨架：表头即审计字段口径（C3.1）；记录经 tauri command 从 core 取，接入后替换空态。
 */
</script>

<style scoped>
.audit {
  width: 100%;
  border-collapse: collapse;
  font-size: 13px;
}

.audit th {
  padding: 6px 8px;
  border-bottom: 1px solid var(--ab-border);
  color: var(--ab-text-weak);
  font-weight: 400;
  text-align: left;
}

.view__action {
  margin: 10px 8px 0 0;
  padding: 4px 12px;
  border: 1px solid var(--ab-border);
  border-radius: 6px;
  background: var(--ab-surface);
  color: var(--ab-text);
  cursor: pointer;
}
</style>
