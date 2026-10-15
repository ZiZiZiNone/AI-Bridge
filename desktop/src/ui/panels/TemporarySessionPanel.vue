<template>
  <WindowShell title="临时会话" no="W5">
    <div class="view">
      <p class="view__hint">
        C1–C5 临时会话。含新建会话表单屏（域名白名单、只读 / 单次、TTL）。
        真实会话由 core（ab-broker）代持，AI 端只见临时凭证；本视图只呈现信封元数据，
        不接触也不落任何明文（S6 明文纪律）。
      </p>

      <StateBlock
        :state="props.state"
        :error-message="props.errorMessage"
        empty-text="暂无临时会话，AI 端签发后会出现在这里"
        @retry="emit('retry')"
      >
        <table class="list">
          <thead>
            <tr>
              <th>账号</th>
              <th>目标</th>
              <th>来源</th>
              <th>到期</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(row, index) in props.envelopes" :key="`${row.account_ref}-${index}`">
              <td>{{ row.account_ref }}</td>
              <td>{{ row.target.domain ?? row.target.app_id ?? '—' }}</td>
              <td>{{ row.source.type }}</td>
              <td>{{ row.expires_at ?? '—' }}</td>
            </tr>
          </tbody>
        </table>
      </StateBlock>

      <button type="button" class="view__action">新建会话</button>
      <button type="button" class="view__action">一键吊销全部</button>
    </div>
  </WindowShell>
</template>

<script setup lang="ts">
import StateBlock from '../components/StateBlock.vue';
import WindowShell from '../components/WindowShell.vue';
import type { SessionEnvelope } from '../../types/contract';
import type { UIState } from '../../types/ui';

/**
 * W5 临时会话面板（弹出面板）。
 * 受控视图：TTL / scope / 吊销等业务规则在 core（ab-broker），UI 只呈现与转发意图。
 */
interface Props {
  envelopes?: SessionEnvelope[];
  state?: UIState;
  errorMessage?: string;
}

const props = withDefaults(defineProps<Props>(), {
  envelopes: () => [],
  state: 'empty',
  errorMessage: '',
});

const emit = defineEmits<{
  retry: [];
}>();
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
