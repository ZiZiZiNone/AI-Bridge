<template>
  <WindowShell title="转换器" no="W8">
    <div class="view">
      <p class="view__hint">
        B4.1 转换器管理：官方验签、启用开关、接入等级（c 会话注入 / b 凭据填表 / a 仅启动）、
        可读路径声明。仅官方签发（S5），验签结果与沙盒执行由 core（ab-converter）负责，UI 只呈现。
      </p>

      <StateBlock
        :state="props.state"
        :error-message="props.errorMessage"
        empty-text="暂无转换器。官方转换器放入 converters/ 目录后在此启用"
        @retry="emit('retry')"
      >
        <table class="list">
          <thead>
            <tr>
              <th>转换器</th>
              <th>版本</th>
              <th>接入等级</th>
              <th>可读路径</th>
              <th>签名者</th>
              <th>验签</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="row in props.converters" :key="row.id">
              <td>{{ row.name }}（{{ row.id }}）</td>
              <td>{{ row.version }}</td>
              <td>{{ levelText(row.level) }}</td>
              <td>{{ row.readable_paths.join('、') }}</td>
              <td>{{ row.signer }}</td>
              <td>{{ row.verified ? '通过' : `未通过：${row.verify_error ?? '未知原因'}` }}</td>
            </tr>
          </tbody>
        </table>
      </StateBlock>
    </div>
  </WindowShell>
</template>

<script setup lang="ts">
import StateBlock from '../components/StateBlock.vue';
import WindowShell from '../components/WindowShell.vue';
import type { AccessLevel } from '../../types/contract';
import type { ConverterView } from '../../types/commands';
import type { UIState } from '../../types/ui';

/**
 * W8 转换器面板（弹出面板）。
 * 受控视图：清单由 core 验签后注入；本视图只按契约字段呈现，不判断验签结果。
 * 启用开关 core 无对应命令，未提供假开关。
 */

interface Props {
  converters?: ConverterView[];
  state?: UIState;
  errorMessage?: string;
}

const props = withDefaults(defineProps<Props>(), {
  converters: () => [],
  state: 'empty',
  errorMessage: '',
});

const emit = defineEmits<{
  retry: [];
}>();

/** 接入等级的中文口径（仅展示映射，等级语义定义在契约与 core）。 */
const LEVEL_TEXT: Readonly<Record<AccessLevel, string>> = {
  c: 'c 会话注入',
  b: 'b 凭据填表',
  a: 'a 仅启动',
};

function levelText(level: AccessLevel): string {
  return LEVEL_TEXT[level];
}
</script>

<style scoped>
.list {
  width: 100%;
  border-collapse: collapse;
  font-size: 13px;
}

.list th,
.list td {
  padding: 6px 8px;
  border-bottom: 1px solid var(--ab-border);
  text-align: left;
}
</style>
