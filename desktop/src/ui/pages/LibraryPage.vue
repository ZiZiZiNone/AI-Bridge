<template>
  <VaultGate
    v-if="!gate.isUnlocked.value || gate.recoveryDisplay.value !== ''"
    :state="gate.state.value"
    :error-message="gate.errorMessage.value"
    :recovery-display="gate.recoveryDisplay.value"
    :is-busy="gate.isBusy.value"
    @submit="handleGateSubmit"
    @retry="gate.check"
    @ack-recovery="gate.acknowledgeRecovery"
  />
  <LibraryPanel
    v-else
    :entries="entries.entries.value"
    :state="entries.state.value"
    :error-message="entries.errorMessage.value"
    :form-visible="formVisible"
    :is-busy="entries.isBusy.value"
    @open-form="formVisible = true"
    @close-form="formVisible = false"
    @add="handleAdd"
    @lock="handleLock"
    @retry="entries.reload"
  />
</template>

<script setup lang="ts">
import { ref } from 'vue';
import VaultGate from '../components/VaultGate.vue';
import LibraryPanel from '../panels/LibraryPanel.vue';
import { useVaultEntries } from '../../hooks/useVaultEntries';
import { useVaultGate } from '../../hooks/useVaultGate';
import type { EntryInput } from '../../types/commands';
import type { GateSubmit } from '../../types/ui';

/**
 * W3 库管理容器（Page 层）：持有 hook 与页面级状态，数据 props 下传、事件上接。
 * 面板与门屏仍是纯受控视图，main.ts 保持薄入口（窗口标签 → 容器）。
 */
const gate = useVaultGate();
const entries = useVaultEntries();
const formVisible = ref(false);

void gate.check();

async function handleGateSubmit(payload: GateSubmit): Promise<void> {
  const succeeded = payload.mode === 'create'
    ? await gate.create(payload.name, payload.password)
    : await gate.unlock(payload.password);
  if (succeeded) await entries.reload();
}

async function handleLock(): Promise<void> {
  if (await gate.lock()) entries.reset();
}

async function handleAdd(input: EntryInput): Promise<void> {
  if (await entries.add(input)) formVisible.value = false;
}
</script>
