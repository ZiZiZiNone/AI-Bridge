/**
 * W8 转换器命令（commands/converters.rs）。信任链验证（结构 + 验签 + 摘要一致）
 * 在 core（ab-converter + 注入的验签器），本层只取列表。
 * 安装走 `converter_install`（显式命令 + 官方签名），当前 UI 无安装入口，不调用。
 */

import type { ConverterView } from '../types/commands';
import type { Result } from '../types/result';
import { invokeCommand } from './tauriClient';

export function listConverters(): Promise<Result<ConverterView[]>> {
  return invokeCommand<ConverterView[]>('converter_list');
}
