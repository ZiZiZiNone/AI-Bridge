<template>
  <WindowShell title="状态清单" no="W4">
    <div class="view">
      <p class="view__hint">
        A2 会话矩阵 / A3 会话健康 / A7 快照回滚 / B1.3 目标状态。
        双视图：①总览（主库 / 设备同步 / 安全 / 最近操作）②会话矩阵（账号 × 端 × 来源 × 到期）。
        矩阵数据来自 core（ab-state）归一化的 SessionRecord，UI 只按契约字段呈现。
      </p>

      <StateBlock
        :state="props.state"
        :error-message="props.errorMessage"
        empty-text="暂无会话记录，先登录一次或从浏览器扩展捕获"
        @retry="emit('retry')"
      >
        <table class="matrix">
          <thead>
            <tr>
              <th>账号</th>
              <th>目标</th>
              <th>来源</th>
              <th>到期</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(row, index) in props.sessions" :key="`${row.account_ref}-${index}`">
              <td>{{ row.account_ref }}</td>
              <td>{{ row.target.domain ?? row.target.app_id ?? '—' }}</td>
              <td>{{ row.source.type }}</td>
              <td>{{ row.expires_at ?? '—' }}</td>
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
import type { SessionRecord } from '../../types/contract';
import type { UIState } from '../../types/ui';

/**
 * W4 状态清单面板（弹出面板）。
 * 受控视图：数据由 props 传入、刷新意图经 emit 上报；业务判断（到期、健康）在 core。
 */
interface Props {
  sessions?: SessionRecord[];
  state?: UIState;
  errorMessage?: string;
}

const props = withDefaults(defineProps<Props>(), {
  sessions: () => [],
  state: 'empty',
  errorMessage: '',
});

const emit = defineEmits<{
  retry: [];
}>();
</script>

<style scoped>
.matrix {
  width: 100%;
  border-collapse: collapse;
  font-size: 13px;
}

.matrix th,
.matrix td {
  padding: 6px 8px;
  border-bottom: 1px solid var(--ab-border);
  text-align: left;
}
</style>
