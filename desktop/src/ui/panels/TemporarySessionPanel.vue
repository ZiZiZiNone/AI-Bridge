<template>
  <WindowShell title="临时会话" no="W5">
    <div class="view">
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
              <th>权限模式</th>
              <th>到期</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(row, index) in props.envelopes" :key="`${row.account_ref}-${index}`">
              <td>{{ row.account_ref }}</td>
              <td>{{ row.target.domain ?? row.target.app_id ?? '—' }}</td>
              <td>{{ row.source.type }}</td>
              <td>{{ row.scope?.read_only === false ? '读写' : '只读 / 写操作挂起审批' }}</td>
              <td>{{ row.expires_at ?? '—' }}</td>
              <td>
                <button
                  type="button"
                  class="view__inline-action"
                  @click="emit('revoke', row.account_ref)"
                >
                  立即收回
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </StateBlock>

      <div class="view__toolbar">
        <button type="button" class="view__action" @click="emit('issue')">新建会话</button>
        <button type="button" class="view__action" @click="emit('revoke-all')">一键吊销全部</button>
      </div>
    </div>
  </WindowShell>
</template>

<script setup lang="ts">
import StateBlock from '../components/StateBlock.vue';
import WindowShell from '../components/WindowShell.vue';
import type { SessionEnvelope } from '../../types/contract';
import type { UIState } from '../../types/ui';

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
  issue: [];
  revoke: [accountRef: string];
  'revoke-all': [];
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

.view__toolbar {
  margin-top: 10px;
}

.view__action,
.view__inline-action {
  margin-right: 8px;
  padding: 4px 12px;
  border: 1px solid var(--ab-border);
  border-radius: 6px;
  background: var(--ab-surface);
  color: var(--ab-text);
  cursor: pointer;
}
</style>
