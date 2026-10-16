/**
 * W7 备份状态与三个面板动作（A6.1 立即备份 / A6.8 恢复演练 / A6.9 恢复）。
 * 单一关注点：备份状态加载与动作编排；加密与校验规则在 core。
 */

import { ref, shallowRef } from 'vue';
import { attemptMessage, resolveListState } from '../logic/uiState.logic';
import * as backupService from '../services/backup.service';
import type { BackupStatusView } from '../types/commands';
import type { UIState } from '../types/ui';
import { useRequestGuard } from './useRequestGuard';

export function useBackup() {
  const guard = useRequestGuard();

  const status = shallowRef<BackupStatusView | null>(null);
  const state = ref<UIState>('idle');
  const errorMessage = ref('');
  /** 动作反馈（备份 / 演练 / 恢复的结果文案），与整体态分开存放。 */
  const actionMessage = ref('');
  const isBusy = ref(false);

  async function reload(): Promise<void> {
    const { isStale } = guard.start();
    const hasData = state.value === 'success';
    state.value = 'loading';
    const result = await backupService.fetchBackupStatus();
    if (isStale()) return;
    if (!result.success) {
      errorMessage.value = attemptMessage(result.error.message, guard.attempts.value);
      if (!hasData) state.value = 'error';
      return;
    }
    status.value = result.data;
    state.value = resolveListState(result.data.files);
    errorMessage.value = '';
  }

  async function run(): Promise<void> {
    isBusy.value = true;
    const result = await backupService.runBackup();
    isBusy.value = false;
    if (!result.success) {
      errorMessage.value = `备份失败：${result.error.message}`;
      return;
    }
    actionMessage.value = `备份完成：${result.data.main_file}（${result.data.entry_count} 条，已自动校验）`;
    errorMessage.value = '';
    await reload();
  }

  async function drill(): Promise<void> {
    isBusy.value = true;
    const result = await backupService.runBackupDrill();
    isBusy.value = false;
    if (!result.success) {
      errorMessage.value = `演练失败：${result.error.message}`;
      return;
    }
    actionMessage.value = result.data.entries_match
      ? `演练通过：${result.data.entry_count} 条与当前库一致`
      : '演练失败：备份与当前库条目集不一致';
    errorMessage.value = '';
  }

  /** 从最新主备份恢复（主口令路径；恢复后进入就绪态）。 */
  async function restore(password: string): Promise<boolean> {
    isBusy.value = true;
    const result = await backupService.restoreFromBackup(password);
    isBusy.value = false;
    if (!result.success) {
      errorMessage.value = `恢复失败：${result.error.message}`;
      return false;
    }
    actionMessage.value = '已从备份恢复，库进入就绪态';
    errorMessage.value = '';
    return true;
  }

  return { status, state, errorMessage, actionMessage, isBusy, reload, run, drill, restore };
}
