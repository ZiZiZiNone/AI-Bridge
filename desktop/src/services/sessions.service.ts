/**
 * W5 临时会话命令（commands/sessions.rs ← ab-mcp / ab-broker）。
 * scope / TTL 强制与多账号消歧在 core，本层只搬运；句柄不出面板。
 */

import type { IssueParams, ListResult } from '../types/commands';
import type { Result } from '../types/result';
import { invokeCommand } from './tauriClient';

export function listTemporarySessions(): Promise<Result<ListResult>> {
  return invokeCommand<ListResult>('session_list');
}

export function issueTemporarySession(params: IssueParams): Promise<Result<unknown>> {
  return invokeCommand<unknown>('session_issue', { params });
}

export function revokeAllTemporarySessions(): Promise<Result<number>> {
  return invokeCommand<number>('session_revoke_all');
}
