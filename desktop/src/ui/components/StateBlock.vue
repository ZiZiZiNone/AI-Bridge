<template>
  <div v-if="props.state === 'loading'" class="state state--loading">加载中…</div>
  <div v-else-if="props.state === 'error'" class="state state--error">
    <p class="state__text">{{ props.errorMessage === '' ? '加载失败' : props.errorMessage }}</p>
    <button type="button" class="state__btn" @click="emit('retry')">重试</button>
  </div>
  <div v-else-if="props.state === 'empty'" class="state state--empty">
    <p class="state__text">{{ props.emptyText }}</p>
    <button type="button" class="state__btn" @click="emit('retry')">重新加载</button>
  </div>
  <div v-else-if="props.state === 'success'" class="state state--success">
    <slot />
  </div>
  <div v-else class="state state--idle">尚未加载</div>
</template>

<script setup lang="ts">
import type { UIState } from '../../types/ui';

/**
 * 状态容器（frontend/rules/ui-states.md）：五态由本组件渲染，
 * 仅 success 态渲染默认插槽，调用方不重复写 v-if 链。
 * 整体态（state）与操作反馈（errorMessage）分开存放。
 */
interface Props {
  state: UIState;
  errorMessage?: string;
  emptyText?: string;
}

const props = withDefaults(defineProps<Props>(), {
  errorMessage: '',
  emptyText: '暂无数据',
});

const emit = defineEmits<{
  retry: [];
}>();
</script>

<style scoped>
.state {
  padding: 16px;
  color: var(--ab-text-weak);
  text-align: center;
}

.state--error {
  color: #c0392b;
}

.state__text {
  margin: 0 0 8px;
}

.state__btn {
  padding: 4px 12px;
  border: 1px solid var(--ab-border);
  border-radius: 6px;
  background: var(--ab-surface);
  color: var(--ab-text);
  font-size: 12px;
  cursor: pointer;
}
</style>
