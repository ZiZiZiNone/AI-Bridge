<template>
  <div class="shell">
    <header class="shell__head">
      <span class="shell__brand">AI-Bridge</span>
      <h1 class="shell__title">{{ props.title }}</h1>
      <span v-if="props.no !== ''" class="shell__no">{{ props.no }}</span>
      <div class="shell__actions">
        <button type="button" class="shell__btn" @click="togglePin">
          {{ isPinned ? '取消置顶' : '置顶' }}
        </button>
        <button type="button" class="shell__btn" @click="close">关闭</button>
      </div>
    </header>
    <main class="shell__body">
      <slot />
    </main>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue';
import { useWindowControls } from '../../hooks/useWindowControls';

/**
 * 窗口外壳：面板（W3–W10）与向导（W11–W14）共用的标题条 + 关闭/置顶。
 * 只承载窗口级交互，不含业务数据与业务判断。
 */
interface Props {
  /** 界面名（W 清单里的中文名） */
  title: string;
  /** 界面编号，如 W3；缺省不显示 */
  no?: string;
}

const props = withDefaults(defineProps<Props>(), { no: '' });

const { close, setAlwaysOnTop } = useWindowControls();

/** 置顶属「纯交互状态」白名单：丢失只影响本次交互体验，不影响业务数据。 */
const isPinned = ref(false);

async function togglePin(): Promise<void> {
  isPinned.value = !isPinned.value;
  await setAlwaysOnTop(isPinned.value);
}
</script>

<style scoped>
.shell {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--ab-bg);
}

.shell__head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px var(--ab-gap);
  border-bottom: 1px solid var(--ab-border);
  background: var(--ab-surface);
}

.shell__brand {
  font-size: 12px;
  font-weight: 600;
  color: var(--ab-accent);
}

.shell__title {
  margin: 0;
  font-size: 14px;
  font-weight: 600;
}

.shell__no {
  font-size: 12px;
  color: var(--ab-text-weak);
}

.shell__actions {
  display: flex;
  gap: 6px;
  margin-left: auto;
}

.shell__btn {
  padding: 2px 10px;
  border: 1px solid var(--ab-border);
  border-radius: 6px;
  background: transparent;
  color: var(--ab-text);
  font-size: 12px;
  cursor: pointer;
}

.shell__body {
  flex: 1;
  overflow: auto;
}
</style>
