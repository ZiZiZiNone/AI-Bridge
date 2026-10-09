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

/**
 * W2 候选条（输入法式小面板，热键唤起）。
 * 承载：B5（一次性复制 / 剪贴板清空）、B6（全局热键 / 窗口匹配）、B7（TOTP / 一键填充）、
 * A4.1（多账号切换）、A5（生成器内嵌）、D1/D2（带态启动）、C5（快捷吊销）。
 * 薄入口：候选结果与匹配规则在 core（ab-vault + ab-state）；本视图只呈现并回传选择。
 * 骨架期数据未接入（经 tauri command 从 core 取），故只显示空态。
 */

/** 输入法式小面板的查询关键字属「输入中间态」白名单，可自持。 */
const keyword = ref('');

/** 前台窗口 / 域名匹配（B6.2）由宿主提供，骨架期占位。 */
const foreground = ref('（未接入）');
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
