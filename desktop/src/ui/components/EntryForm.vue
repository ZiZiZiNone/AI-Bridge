<template>
  <form class="entry-form" @submit.prevent="submit">
    <h2 class="entry-form__title">新建条目</h2>

    <label class="entry-form__field">
      <span>标题</span>
      <input v-model="draft.title" type="text" required />
    </label>
    <label class="entry-form__field">
      <span>账号引用（account_ref）</span>
      <input v-model="draft.account_ref" type="text" required />
    </label>
    <label class="entry-form__field">
      <span>类型</span>
      <select v-model="draft.kind">
        <option value="password">账号密码</option>
        <option value="passkey">Passkey</option>
        <option value="totp">TOTP</option>
      </select>
    </label>
    <label class="entry-form__field">
      <span>用户名</span>
      <input v-model="draft.username" type="text" autocomplete="off" />
    </label>
    <label v-if="draft.kind !== 'totp'" class="entry-form__field">
      <span>秘密</span>
      <input v-model="draft.secret" type="password" autocomplete="new-password" />
    </label>
    <label v-if="draft.kind === 'totp'" class="entry-form__field">
      <span>TOTP 种子</span>
      <input v-model="draft.totp_seed" type="text" autocomplete="off" />
    </label>
    <label class="entry-form__field">
      <span>网址</span>
      <input v-model="draft.url" type="url" placeholder="https://example.com/login" />
    </label>
    <label class="entry-form__field">
      <span>备注</span>
      <input v-model="draft.note" type="text" />
    </label>

    <div class="entry-form__actions">
      <button type="submit" class="entry-form__btn">{{ isBusy ? '提交中…' : '提交' }}</button>
      <button type="button" class="entry-form__btn" @click="emit('cancel')">取消</button>
    </div>
    <p class="entry-form__hint">字段校验与加密由 core 完成，失败时表单保留已填内容</p>
  </form>
</template>

<script setup lang="ts">
import { reactive } from 'vue';
import type { CredentialKind } from '../../types/contract';
import type { EntryInput } from '../../types/commands';

/**
 * 新建条目表单（W3 的表单屏）。
 * 表单草稿属「不需保留」白名单，可自持；提交只上报意图（EntryInput），
 * 成败与关闭由容器决定（失败保留输入，见 async-operations.md 手动重试）。
 */

interface Props {
  /** 提交进行中（禁用提交按钮，防重复提交）。 */
  isBusy?: boolean;
}

withDefaults(defineProps<Props>(), { isBusy: false });

const emit = defineEmits<{
  submit: [input: EntryInput];
  cancel: [];
}>();

interface EntryDraft {
  title: string;
  account_ref: string;
  kind: CredentialKind;
  username: string;
  secret: string;
  totp_seed: string;
  url: string;
  note: string;
}

const draft = reactive<EntryDraft>({
  title: '',
  account_ref: '',
  kind: 'password',
  username: '',
  secret: '',
  totp_seed: '',
  url: '',
  note: '',
});

/** 空串不进入载荷（可选项留空即不传），字段名与后端 serde 一致（snake_case）。 */
function submit(): void {
  const input: EntryInput = {
    title: draft.title.trim(),
    account_ref: draft.account_ref.trim(),
    kind: draft.kind,
  };
  if (draft.username.trim() !== '') input.username = draft.username.trim();
  if (draft.secret !== '') input.secret = draft.secret;
  if (draft.totp_seed.trim() !== '') input.totp_seed = draft.totp_seed.trim();
  if (draft.url.trim() !== '') input.url = draft.url.trim();
  if (draft.note.trim() !== '') input.note = draft.note.trim();
  emit('submit', input);
}
</script>

<style scoped>
.entry-form {
  margin-top: 12px;
  padding: 12px;
  border: 1px solid var(--ab-border);
  border-radius: var(--ab-radius);
  background: var(--ab-surface);
  display: grid;
  gap: 8px;
}

.entry-form__title {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
}

.entry-form__field {
  display: grid;
  grid-template-columns: 140px 1fr;
  align-items: center;
  gap: 8px;
  font-size: 13px;
}

.entry-form__field span {
  color: var(--ab-text-weak);
}

.entry-form__field input,
.entry-form__field select {
  padding: 4px 8px;
  border: 1px solid var(--ab-border);
  border-radius: 6px;
  background: var(--ab-bg);
  color: var(--ab-text);
}

.entry-form__actions {
  display: flex;
  gap: 8px;
}

.entry-form__btn {
  padding: 4px 12px;
  border: 1px solid var(--ab-border);
  border-radius: 6px;
  background: var(--ab-surface);
  color: var(--ab-text);
  cursor: pointer;
}

.entry-form__hint {
  margin: 0;
  color: var(--ab-text-weak);
  font-size: 12px;
}
</style>
