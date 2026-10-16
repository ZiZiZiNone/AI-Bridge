/**
 * W4 状态清单命令（commands/state.rs ← ab-state）。
 * 矩阵筛选/排序、健康分级规则在 core，本层只搬运。
 */

import type { MatrixFilters, MatrixRow } from '../types/commands';
import type { Result } from '../types/result';
import { invokeCommand } from './tauriClient';

export function querySessionMatrix(filters: MatrixFilters): Promise<Result<MatrixRow[]>> {
  return invokeCommand<MatrixRow[]>('state_matrix', { filters });
}
