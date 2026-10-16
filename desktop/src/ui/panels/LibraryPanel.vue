<template>
  <WindowShell title="库管理" no="W3">
    <div class="view">
      <p class="view__hint">
        A1 条目管理 / A4 多账号与身份档案 / A5 生成器。
        含条目新建与编辑表单屏（全字段 + 账号密码 / passkey / TOTP 类型切换）。
        数据经 tauri command 从 core（ab-vault）取得，本视图只组装与转发。
      </p>

      <div class="view__actions">
        <button type="button" class="view__action" @click="emit('open-form')">新建条目</button>
        <button type="button" class="view__action" @click="emit('lock')">锁定库</button>
      </div>

      <p v-if="props.errorMessage !== ''" class="view__error">{{ props.errorMessage }}</p>

      <StateBlock
        :state="props.state"
        :error-message="props.errorMessage"
        empty-text="还没有条目，先新建第一条账号密码"
        @retry="emit('retry')"
      >
        <table class="list">
          <thead>
            <tr>
              <th>标题</th>
              <th>账号引用</th>
              <th>类型</th>
              <th>用户名</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="row in props.entries" :key="row.uuid">
              <td>{{ row.title }}</td>
              <td>{{ row.account_ref }}</td>
              <td>{{ row.kind }}</td>
              <td>{{ row.username ?? '—' }}</td>
            </tr>
          </tbody>
        </table>
      </StateBlock>

      <EntryForm
        v-if="props.formVisible"
        :is-busy="props.isBusy"
        @submit="emit('add', $event)"
        @cancel="emit('close-form')"
      />
    </div>
  </WindowShell>
</template>

<script setup lang="ts">
import EntryForm from '../components/EntryForm.vue';
import StateBlock from '../components/StateBlock.vue';
import WindowShell from '../components/WindowShell.vue';
import type { EntryInput, EntryView } from '../../types/commands';
import type { UIState } from '../../types/ui';

/**
 * W3 库管理面板（弹出面板，用完即关）——解锁后的条目视图。
 * 受控视图：条目列表与五态经 props 传入，意图（新建 / 锁定 / 提交）经 emit 上报；
 * 条目字段与校验规则在 core，UI 不判断。列表行不含秘密（S6）。
 */

interface Props {
  entries?: EntryView[];
  state?: UIState;
  errorMessage?: string;
  formVisible?: boolean;
  isBusy?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  entries: () => [],
  state: 'empty',
  errorMessage: '',
  formVisible: false,
  isBusy: false,
});

const emit = defineEmits<{
  'open-form': [];
  'close-form': [];
  add: [input: EntryInput];
  lock: [];
  retry: [];
}>();
</script>

<style scoped>
.view__actions {
  display: flex;
  gap: 8px;
  margin-top: 10px;
}

.view__action {
  padding: 4px 12px;
  border: 1px solid var(--ab-border);
  border-radius: 6px;
  background: var(--ab-surface);
  color: var(--ab-text);
  cursor: pointer;
}

.view__error {
  margin: 10px 0 0;
  color: #c0392b;
  font-size: 13px;
}

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
