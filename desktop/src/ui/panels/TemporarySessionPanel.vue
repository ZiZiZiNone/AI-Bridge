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
            <tr v-for="(row, index) in props.sessions" :key="`${row.session_id}-${index}`">
              <td>{{ row.account_ref }}</td>
              <td>{{ row.target.domain ?? row.target.app_id ?? '—' }}</td>
              <td>{{ row.source_type }}</td>
              <td>{{ row.expires_at }}</td>
            </tr>
          </tbody>
        </table>
      </StateBlock>

      <p v-if="props.errorMessage !== ''" class="view__feedback view__feedback--error">{{ props.errorMessage }}</p>
      <p v-if="props.actionMessage !== ''" class="view__feedback">{{ props.actionMessage }}</p>

      <button type="button" class="view__action" @click="emit('open-form')">新建会话</button>
      <button type="button" class="view__action" :disabled="props.isBusy" @click="emit('revoke-all')">
        {{ props.isBusy ? '处理中…' : '一键吊销全部' }}
      </button>

      <form v-if="props.formVisible" class="issue" @submit.prevent="submitIssue">
        <h2 class="issue__title">新建临时会话</h2>
        <label class="issue__field">
          <span>目标域名</span>
          <input v-model="domain" type="text" placeholder="example.com" required />
        </label>
        <label class="issue__field">
          <span>有效期（秒）</span>
          <input v-model.number="ttlSecs" type="number" min="1" />
        </label>
        <label class="issue__check">
          <input v-model="readOnly" type="checkbox" /> 只读
        </label>
        <label class="issue__check">
          <input v-model="singleUse" type="checkbox" /> 单次
        </label>
        <div class="issue__actions">
          <button type="submit" class="view__action" :disabled="props.isBusy">
            {{ props.isBusy ? '签发中…' : '签发' }}
          </button>
          <button type="button" class="view__action" @click="emit('close-form')">取消</button>
        </div>
        <p class="issue__hint">域名白名单即本表单的目标域名；scope 与 TTL 上限由 core 强制</p>
      </form>
    </div>
  </WindowShell>
</template>

<script setup lang="ts">
import { reactive, toRefs } from 'vue';
import StateBlock from '../components/StateBlock.vue';
import WindowShell from '../components/WindowShell.vue';
import type { IssueParams, SessionSummary } from '../../types/commands';
import type { UIState } from '../../types/ui';

/**
 * W5 临时会话面板（弹出面板）。
 * 受控视图：会话总览与五态经 props 传入，意图（签发 / 吊销）经 emit 上报；
 * TTL / scope / 消歧规则在 core（ab-broker），UI 不判断。
 * 表单草稿属「不需保留」白名单，可自持。
 */

interface Props {
  sessions?: SessionSummary[];
  state?: UIState;
  errorMessage?: string;
  actionMessage?: string;
  formVisible?: boolean;
  isBusy?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  sessions: () => [],
  state: 'empty',
  errorMessage: '',
  actionMessage: '',
  formVisible: false,
  isBusy: false,
});

const emit = defineEmits<{
  retry: [];
  'open-form': [];
  'close-form': [];
  issue: [params: IssueParams];
  'revoke-all': [];
}>();

const draft = reactive({
  domain: '',
  ttlSecs: 3600,
  readOnly: false,
  singleUse: false,
});
const { domain, ttlSecs, readOnly, singleUse } = toRefs(draft);

function submitIssue(): void {
  const target = draft.domain.trim();
  emit('issue', {
    target: { kind: 'web', domain: target },
    scope: { domain_allowlist: [target], read_only: draft.readOnly, single_use: draft.singleUse },
    ttl_secs: draft.ttlSecs,
  });
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

.view__feedback {
  margin: 10px 0 0;
  font-size: 13px;
}

.view__feedback--error {
  color: #c0392b;
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

.view__action:disabled {
  opacity: 0.6;
  cursor: default;
}

.issue {
  margin-top: 12px;
  padding: 12px;
  border: 1px solid var(--ab-border);
  border-radius: var(--ab-radius);
  background: var(--ab-surface);
  display: grid;
  gap: 8px;
}

.issue__title {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
}

.issue__field {
  display: grid;
  grid-template-columns: 110px 1fr;
  align-items: center;
  gap: 8px;
  font-size: 13px;
}

.issue__field span {
  color: var(--ab-text-weak);
}

.issue__field input {
  padding: 4px 8px;
  border: 1px solid var(--ab-border);
  border-radius: 6px;
  background: var(--ab-bg);
  color: var(--ab-text);
}

.issue__check {
  font-size: 13px;
}

.issue__actions {
  display: flex;
  gap: 8px;
}

.issue__actions .view__action {
  margin-top: 0;
}

.issue__hint {
  margin: 0;
  color: var(--ab-text-weak);
  font-size: 12px;
}
</style>
