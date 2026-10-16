<template>
  <BackupPanel
    :status="backup.status.value"
    :state="backup.state.value"
    :error-message="backup.errorMessage.value"
    :action-message="backup.actionMessage.value"
    :restore-visible="restoreVisible"
    :is-busy="backup.isBusy.value"
    @retry="backup.reload"
    @run="backup.run"
    @drill="backup.drill"
    @open-restore="restoreVisible = true"
    @close-restore="restoreVisible = false"
    @restore="handleRestore"
  />
</template>

<script setup lang="ts">
import { ref } from 'vue';
import BackupPanel from '../panels/BackupPanel.vue';
import { useBackup } from '../../hooks/useBackup';

/**
 * W7 备份容器（Page 层）：持有备份状态 hook 与恢复口令行可见性。
 */
const backup = useBackup();
const restoreVisible = ref(false);

void backup.reload();

async function handleRestore(password: string): Promise<void> {
  if (await backup.restore(password)) restoreVisible.value = false;
}
</script>
