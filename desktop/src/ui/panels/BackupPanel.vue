<template>
  <WindowShell title="备份" no="W7">
    <div class="view">
      <p class="view__hint">
        A6 备份与恢复。全部备份强制加密（A6.4），本视图只呈现状态与开关，
        加密、快照保留策略、自动校验规则均在 core。
      </p>

      <dl class="facts">
        <dt>最后验证时间</dt>
        <dd>{{ props.status?.last_verified_at ?? '尚未备份' }}</dd>
        <dt>托管目录</dt>
        <dd class="facts__path">（未设置，建议放外接盘或异盘，防勒索连锅端）</dd>
        <dt>备份文件</dt>
        <dd class="facts__path">{{ props.status?.files.join('、') ?? '—' }}</dd>
        <dt>定时备份</dt>
        <dd><label><input v-model="scheduled" type="checkbox" /> 默认月度，频率可调（core 暴露配置接口后接入）</label></dd>
      </dl>

      <p v-if="props.errorMessage !== ''" class="facts__error">
        {{ props.errorMessage }}
        <button v-if="props.state === 'error'" type="button" class="view__action" @click="emit('retry')">重新加载</button>
      </p>
      <p v-if="props.actionMessage !== ''" class="facts__ok">{{ props.actionMessage }}</p>

      <button type="button" class="view__action" :disabled="props.isBusy" @click="emit('run')">
        {{ props.isBusy ? '处理中…' : '立即备份' }}
      </button>
      <button type="button" class="view__action" :disabled="props.isBusy" @click="emit('drill')">恢复演练</button>
      <button type="button" class="view__action" @click="emit('open-restore')">从备份恢复</button>

      <form v-if="props.restoreVisible" class="restore" @submit.prevent="submitRestore">
        <label class="restore__field">
          <span>主口令</span>
          <input v-model="password" type="password" autocomplete="current-password" required />
        </label>
        <div class="restore__actions">
          <button type="submit" class="view__action" :disabled="props.isBusy">
            {{ props.isBusy ? '恢复中…' : '恢复' }}
          </button>
          <button type="button" class="view__action" @click="emit('close-restore')">取消</button>
        </div>
        <p class="restore__hint">从最新主备份恢复并进入就绪态；恢复密钥副本路径见恢复向导</p>
      </form>
    </div>
  </WindowShell>
</template>

<script setup lang="ts">
import { ref } from 'vue';
import WindowShell from '../components/WindowShell.vue';
import type { BackupStatusView } from '../../types/commands';
import type { UIState } from '../../types/ui';

/**
 * W7 备份面板（弹出面板）。
 * 承载：A6.1 手动备份、A6.5 托管目录、A6.6 滚动保留、A6.7 自动校验、A6.8 恢复演练、A6.9 恢复导入。
 * 受控视图：状态与动作反馈经 props 传入，意图经 emit 上报；加密与校验规则在 core。
 * 定时备份开关 core 无配置命令，保留现状（如实标注未接入）。
 */

interface Props {
  status?: BackupStatusView | null;
  state?: UIState;
  errorMessage?: string;
  actionMessage?: string;
  restoreVisible?: boolean;
  isBusy?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  status: null,
  state: 'idle',
  errorMessage: '',
  actionMessage: '',
  restoreVisible: false,
  isBusy: false,
});

const emit = defineEmits<{
  retry: [];
  run: [];
  drill: [];
  'open-restore': [];
  'close-restore': [];
  restore: [password: string];
}>();

/** 定时备份为骨架占位开关（core 无配置命令），不接任何命令。 */
const scheduled = ref(false);
const password = ref('');

function submitRestore(): void {
  emit('restore', password.value);
}
</script>

<style scoped>
.facts {
  display: grid;
  grid-template-columns: max-content 1fr;
  gap: 6px 12px;
  margin: 12px 0 0;
  font-size: 13px;
}

.facts dt {
  color: var(--ab-text-weak);
}

.facts dd {
  margin: 0;
}

.facts__path {
  color: var(--ab-text-weak);
  word-break: break-all;
}

.facts__error {
  margin: 10px 0 0;
  color: #c0392b;
  font-size: 13px;
}

.facts__ok {
  margin: 10px 0 0;
  font-size: 13px;
}

.view__action {
  margin: 12px 8px 0 0;
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

.restore {
  margin-top: 12px;
  padding: 12px;
  border: 1px solid var(--ab-border);
  border-radius: var(--ab-radius);
  background: var(--ab-surface);
  display: grid;
  gap: 8px;
}

.restore__field {
  display: grid;
  grid-template-columns: 70px 1fr;
  align-items: center;
  gap: 8px;
  font-size: 13px;
}

.restore__field span {
  color: var(--ab-text-weak);
}

.restore__field input {
  padding: 4px 8px;
  border: 1px solid var(--ab-border);
  border-radius: 6px;
  background: var(--ab-bg);
  color: var(--ab-text);
}

.restore__actions {
  display: flex;
  gap: 8px;
}

.restore__actions .view__action {
  margin-top: 0;
}

.restore__hint {
  margin: 0;
  color: var(--ab-text-weak);
  font-size: 12px;
}
</style>
