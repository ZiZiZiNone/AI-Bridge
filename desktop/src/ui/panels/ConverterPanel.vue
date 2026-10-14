<template>
  <WindowShell title="转换器" no="W8">
    <div class="view">
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
            </tr>
          </thead>
          <tbody>
            <tr v-for="row in props.manifests" :key="row.id">
              <td>{{ row.name }}（{{ row.id }}）</td>
              <td>{{ row.version }}</td>
              <td>{{ levelText(row.level) }}</td>
              <td>{{ row.readable_paths.join('、') }}</td>
              <td>{{ row.signature.signer }}</td>
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
import type { AccessLevel, ConverterManifest } from '../../types/contract';
import type { UIState } from '../../types/ui';

interface Props {
  manifests?: ConverterManifest[];
  state?: UIState;
  errorMessage?: string;
}

const props = withDefaults(defineProps<Props>(), {
  manifests: () => [],
  state: 'empty',
  errorMessage: '',
});

const emit = defineEmits<{
  retry: [];
}>();

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
