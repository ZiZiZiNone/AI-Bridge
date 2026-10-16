/**
 * W7 备份命令（commands/backup.rs）。备份内容的加密、恢复密钥副本、
 * 校验口径在 core；本层只搬运。主口令可选（缺省时只校验恢复密钥副本）。
 */

import type { BackupReport, BackupStatusView, DrillReport } from '../types/commands';
import type { Result } from '../types/result';
import { invokeCommand } from './tauriClient';

export function fetchBackupStatus(): Promise<Result<BackupStatusView>> {
  return invokeCommand<BackupStatusView>('backup_status');
}

export function runBackup(): Promise<Result<BackupReport>> {
  return invokeCommand<BackupReport>('backup_run');
}

export function runBackupDrill(): Promise<Result<DrillReport>> {
  return invokeCommand<DrillReport>('backup_drill');
}

export function restoreFromBackup(password: string): Promise<Result<null>> {
  return invokeCommand<null>('backup_restore_with_password', { password });
}
