<template>
  <WindowShell title="审计" no="W6">
    <div class="view">
      <p class="view__hint">
        C3 审计流：谁 / 何时 / 账号 / 域名 / 动作 / 结果；筛选与加密导出。
        审计记录由 core（ab-audit）追加式写入，只读呈现；UI 不修改历史。
      </p>

      <StateBlock
        :state="props.state"
        :error-message="props.errorMessage"
        empty-text="暂无审计记录"
        @retry="emit('retry')"
      >
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
          <tbody>
            <tr v-for="(row, index) in props.events" :key="`${row.when}-${index}`">
              <td>{{ row.when }}</td>
              <td>{{ row.who }}</td>
              <td>{{ row.account_ref ?? '—' }}</td>
              <td>{{ row.domain ?? '—' }}</td>
              <td>{{ actionText(row.action) }}</td>
              <td>{{ resultText(row.result) }}</td>
            </tr>
          </tbody>
        </table>
      </StateBlock>

      <button type="button" class="view__action" @click="filterVisible = !filterVisible">筛选</button>
      <button type="button" class="view__action" @click="notice = '加密导出尚未接入：core（ab-audit）暂无对应命令，如实保留入口'">加密导出</button>

      <form v-if="filterVisible" class="filter" @submit.prevent="applyFilter">
        <label class="filter__field">
          <span>域名</span>
          <input v-model="domain" type="text" placeholder="精确匹配，留空不限" />
        </label>
        <label class="filter__field">
          <span>账号</span>
          <input v-model="accountRef" type="text" placeholder="精确匹配，留空不限" />
        </label>
        <button type="submit" class="view__action">应用筛选</button>
      </form>

      <p v-if="notice !== ''" class="view__notice">{{ notice }}</p>
    </div>
  </WindowShell>
</template>

<script setup lang="ts">
import { ref } from 'vue';
import StateBlock from '../components/StateBlock.vue';
import WindowShell from '../components/WindowShell.vue';
import type { AuditAction, AuditEvent, AuditFilter, AuditResultKind } from '../../types/commands';
import type { UIState } from '../../types/ui';

/**
 * W6 审计面板（弹出面板）。
 * 受控视图：审计记录与五态经 props 传入，查询意图经 emit 上报；
 * 筛选语义在 core，记录只读。「加密导出」core 无命令，点击如实说明未接入。
 * 筛选行展开与草稿属纯 UI 交互状态（白名单），可自持。
 */

interface Props {
  events?: AuditEvent[];
  state?: UIState;
  errorMessage?: string;
}

const props = withDefaults(defineProps<Props>(), {
  events: () => [],
  state: 'empty',
  errorMessage: '',
});

const emit = defineEmits<{
  retry: [];
  query: [filter: AuditFilter];
}>();

const filterVisible = ref(false);
const domain = ref('');
const accountRef = ref('');
const notice = ref('');

/** 动作 / 结果的中文口径（仅展示映射，语义定义在 core 的审计枚举）。 */
const ACTION_TEXT: Readonly<Record<AuditAction, string>> = {
  unlock: '解锁',
  lock: '锁定',
  capture: '捕获',
  inject: '注入',
  fill: '填充',
  issue: '签发',
  renew: '续期',
  release: '释放',
  revoke: '吊销',
  revoke_all: '全部吊销',
  sweep: '清理',
  use: '使用',
  extract: '提取',
  converter_load: '转换器载入',
  converter_reject: '转换器拒载',
  pair: '配对',
  sync: '同步',
  vault_write: '库写入',
  vault_delete: '库删除',
  restore: '恢复',
  export: '导出',
};

const RESULT_TEXT: Readonly<Record<AuditResultKind, string>> = {
  ok: '成功',
  denied: '拒绝',
  failed: '失败',
};

function actionText(action: AuditAction): string {
  return ACTION_TEXT[action];
}

function resultText(result: AuditResultKind): string {
  return RESULT_TEXT[result];
}

function applyFilter(): void {
  const filter: AuditFilter = {};
  if (domain.value.trim() !== '') filter.domain = domain.value.trim();
  if (accountRef.value.trim() !== '') filter.account_ref = accountRef.value.trim();
  emit('query', filter);
}
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

.audit td {
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

.filter {
  margin-top: 10px;
  padding: 10px;
  border: 1px solid var(--ab-border);
  border-radius: var(--ab-radius);
  background: var(--ab-surface);
  display: grid;
  gap: 8px;
}

.filter__field {
  display: grid;
  grid-template-columns: 60px 1fr;
  align-items: center;
  gap: 8px;
  font-size: 13px;
}

.filter__field span {
  color: var(--ab-text-weak);
}

.filter__field input {
  padding: 4px 8px;
  border: 1px solid var(--ab-border);
  border-radius: 6px;
  background: var(--ab-bg);
  color: var(--ab-text);
}

.view__notice {
  margin: 10px 0 0;
  color: var(--ab-text-weak);
  font-size: 13px;
}
</style>
