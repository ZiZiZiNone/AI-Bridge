/**
 * W6 审计命令（commands/audit.rs ← ab-audit）。筛选语义在 core；
 * 「加密导出」core 无 API（README 缺口清单），不在本层补。
 */

import type { AuditEvent, AuditFilter } from '../types/commands';
import type { Result } from '../types/result';
import { invokeCommand } from './tauriClient';

export function queryAuditTrail(filter: AuditFilter): Promise<Result<AuditEvent[]>> {
  return invokeCommand<AuditEvent[]>('audit_query', { filter });
}
