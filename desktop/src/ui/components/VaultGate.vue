<template>
  <WindowShell title="库管理" no="W3">
    <div class="view">
      <p class="view__hint">
        A1 条目管理 / A4 多账号与身份档案 / A5 生成器。
        数据经 tauri command 从 core（ab-vault）取得，本视图只组装与转发。
      </p>

      <div v-if="props.state === 'loading'" class="gate__status">正在检查库状态…</div>

      <StateBlock
        v-else-if="props.state === 'error'"
        :state="'error'"
        :error-message="props.errorMessage"
        @retry="emit('retry')"
      />

      <!-- S3：恢复密钥只此一次展示，确认知悉后才继续 -->
      <section v-else-if="props.recoveryDisplay !== ''" class="gate__recovery">
        <h2 class="gate__title">恢复密钥（只显示这一次）</h2>
        <p class="gate__key">{{ props.recoveryDisplay }}</p>
        <p class="gate__hint">请立即抄写并离线保存；丢失后库将无法找回。</p>
        <button type="button" class="view__action" @click="emit('ack-recovery')">我已离线保存</button>
      </section>

      <section v-else class="gate">
        <div class="gate__tabs" role="tablist">
          <button
            type="button"
            :class="['gate__tab', { 'gate__tab--active': mode === 'unlock' }]"
            @click="mode = 'unlock'"
          >解锁已有库</button>
          <button
            type="button"
            :class="['gate__tab', { 'gate__tab--active': mode === 'create' }]"
            @click="mode = 'create'"
          >新建库</button>
        </div>

        <form class="gate__form" @submit.prevent="submit">
          <label v-if="mode === 'create'" class="gate__field">
            <span>库名</span>
            <input v-model="name" type="text" :required="mode === 'create'" />
          </label>
          <label class="gate__field">
            <span>主口令</span>
            <input v-model="password" type="password" autocomplete="current-password" required />
          </label>
          <button type="submit" class="view__action" :disabled="props.isBusy">
            {{ props.isBusy ? '处理中…' : mode === 'create' ? '建库并生成恢复密钥' : '解锁' }}
          </button>
        </form>

        <p v-if="props.errorMessage !== ''" class="gate__error">{{ props.errorMessage }}</p>
      </section>
    </div>
  </WindowShell>
</template>

<script setup lang="ts">
import { ref } from 'vue';
import StateBlock from './StateBlock.vue';
import WindowShell from './WindowShell.vue';
import type { GateSubmit, UIState } from '../../types/ui';

/**
 * W3 库就绪门（解锁 / 建库 / 恢复密钥展示）。
 * 受控视图：就绪态与错误经 props 传入，提交意图经 emit 上报；
 * 口令校验、Argon2id、恢复密钥生成全部在 core，UI 不判断。
 * 模式切换与表单草稿属纯 UI 交互状态（白名单），可自持。
 */

interface Props {
  state?: UIState;
  errorMessage?: string;
  /** 建库后的恢复密钥（S3 只此一次）；非空时优先于表单展示。 */
  recoveryDisplay?: string;
  isBusy?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  state: 'idle',
  errorMessage: '',
  recoveryDisplay: '',
  isBusy: false,
});

const emit = defineEmits<{
  submit: [payload: GateSubmit];
  retry: [];
  'ack-recovery': [];
}>();

const mode = ref<'unlock' | 'create'>('unlock');
const name = ref('');
const password = ref('');

function submit(): void {
  emit('submit', { mode: mode.value, name: name.value.trim(), password: password.value });
}
</script>

<style scoped>
.gate__status {
  padding: 16px;
  color: var(--ab-text-weak);
  text-align: center;
}

.gate__tabs {
  display: flex;
  gap: 6px;
  margin-top: 12px;
}

.gate__tab {
  padding: 4px 12px;
  border: 1px solid var(--ab-border);
  border-radius: 6px;
  background: var(--ab-surface);
  color: var(--ab-text-weak);
  cursor: pointer;
}

.gate__tab--active {
  color: var(--ab-accent);
  border-color: var(--ab-accent);
}

.gate__form {
  margin-top: 12px;
  display: grid;
  gap: 8px;
}

.gate__field {
  display: grid;
  grid-template-columns: 90px 1fr;
  align-items: center;
  gap: 8px;
  font-size: 13px;
}

.gate__field span {
  color: var(--ab-text-weak);
}

.gate__field input {
  padding: 4px 8px;
  border: 1px solid var(--ab-border);
  border-radius: 6px;
  background: var(--ab-bg);
  color: var(--ab-text);
}

.gate__error {
  margin: 10px 0 0;
  color: #c0392b;
  font-size: 13px;
}

.gate__recovery {
  margin-top: 12px;
  padding: 12px;
  border: 1px solid var(--ab-accent);
  border-radius: var(--ab-radius);
  background: var(--ab-surface);
}

.gate__title {
  margin: 0 0 8px;
  font-size: 13px;
  font-weight: 600;
}

.gate__key {
  margin: 0 0 8px;
  font-family: 'Cascadia Mono', Consolas, monospace;
  font-size: 12px;
  word-break: break-all;
}

.gate__hint {
  margin: 0 0 10px;
  color: var(--ab-text-weak);
  font-size: 12px;
}

.view__action {
  margin-top: 4px;
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
</style>
