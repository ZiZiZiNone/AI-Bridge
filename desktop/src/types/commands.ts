/**
 * 桌面命令契约镜像（前端侧，零逻辑）。
 *
 * 唯一事实源：`src-tauri/src/commands/*.rs` 的 `#[tauri::command]` 包装与 core DTO
 * （ab_state / ab_mcp / ab_audit / ab_sync）。字段与 Rust 侧逐字段一致（一律 snake_case，
 * 枚举按 serde 属性的序列化形态：snake_case / lowercase）。
 * 契约正文（contracts/*.schema.json）的类型在 `contract.ts`，本文件只镜像命令面；
 * 二者都不含业务规则。已接线的命令才在此登记（无消费者的类型不预造）。
 */

import type {
  AccessLevel,
  CredentialKind,
  SessionKind,
  Source,
  Target,
} from './contract';

/* ---------------- W3 库管理（commands/library.rs） ---------------- */

/** 条目列表行（无秘密字段，S6 明文纪律：秘密不经过非必要通道）。 */
export interface EntryView {
  uuid: string;
  title: string;
  account_ref: string;
  kind: CredentialKind;
  username?: string;
  url?: string;
  has_secret: boolean;
  has_totp: boolean;
}

/** 新建条目输入（全 snake_case，可选字段留空即不传）。 */
export interface EntryInput {
  title: string;
  account_ref: string;
  kind: CredentialKind;
  username?: string;
  secret?: string;
  totp_seed?: string;
  url?: string;
  note?: string;
}

/** 建库结果：恢复密钥只此一次返回（S3），UI 必须当场提示离线保存。 */
export interface InitView {
  recovery_display: string;
}

/* ---------------- W4 状态清单（commands/state.rs ← ab_state） ---------------- */

/** 端类型（serde lowercase）。 */
export type EndpointKind = 'browser' | 'desktop' | 'mobile';

export interface Endpoint {
  kind: EndpointKind;
  id: string;
}

/** 健康分级（serde snake_case；阈值与分级规则在 core）。 */
export type HealthLevel = 'active' | 'expiring_soon' | 'expired';

/** 会话矩阵行（无 payload；cookie 原文不经过 UI 层）。 */
export interface MatrixRow {
  account_ref: string;
  endpoint: Endpoint;
  source: Source;
  target: Target;
  kind: SessionKind;
  expires_at?: string;
  remaining_secs?: number;
  health: HealthLevel;
}

/** 矩阵筛选（serde default：传 {} 即全量）。 */
export interface MatrixFilters {
  account_ref?: string;
  endpoint_id?: string;
  endpoint_kind?: EndpointKind;
  source_type?: string;
  source_id?: string;
  expiry?: { any?: null } | { expired?: null } | { expiring: { within_secs: number } } | { without_expiry?: null };
  sort?: { key: string; ascending: boolean };
}

/* ---------------- W5 临时会话（commands/sessions.rs ← ab_mcp） ---------------- */

/** 签发目标（kind 为字符串：'web' | 'app' | 'container'）。 */
export interface McpTarget {
  kind: string;
  domain?: string;
  app_id?: string;
}

/** 授权范围（AI 的能力上限）。 */
export interface McpScope {
  domain_allowlist: string[];
  read_only: boolean;
  single_use: boolean;
}

/** 签发入参（结构与 MCP 工具面同构）。 */
export interface IssueParams {
  target: McpTarget;
  scope: McpScope;
  ttl_secs?: number;
  account_ref?: string;
}

/** 临时会话总览行（不含句柄）。 */
export interface SessionSummary {
  session_id: string;
  account_ref: string;
  target: McpTarget;
  scope: McpScope;
  source_type: string;
  source_id: string;
  issued_at: string;
  expires_at: string;
  remaining_ttl_secs: number;
  state: string;
  who: string;
}

export interface ListResult {
  sessions: SessionSummary[];
}

/* ---------------- W6 审计（commands/audit.rs ← ab_audit） ---------------- */

export type AuditAction =
  | 'unlock' | 'lock' | 'capture' | 'inject' | 'fill' | 'issue' | 'renew'
  | 'release' | 'revoke' | 'revoke_all' | 'sweep' | 'use' | 'extract'
  | 'converter_load' | 'converter_reject' | 'pair' | 'sync' | 'vault_write'
  | 'vault_delete' | 'restore' | 'export';

export type AuditResultKind = 'ok' | 'denied' | 'failed';

/** 审计事件（只读呈现，UI 不修改历史）。 */
export interface AuditEvent {
  who: string;
  when: string;
  account_ref?: string;
  domain?: string;
  action: AuditAction;
  result: AuditResultKind;
  detail?: Record<string, string>;
}

/** 审计筛选（字段精确匹配，缺省即不限）。 */
export interface AuditFilter {
  who?: string;
  account_ref?: string;
  domain?: string;
  action?: AuditAction;
  result?: AuditResultKind;
}

/* ---------------- W7 备份（commands/backup.rs） ---------------- */

/** 备份状态（最后验证时间 + 文件清单；只回文件名，不回完整路径）。 */
export interface BackupStatusView {
  last_backup_at?: string;
  last_verified_at?: string;
  files: string[];
}

/** 备份报告。 */
export interface BackupReport {
  main_file: string;
  recovery_file: string;
  entry_count: number;
  main_verified?: boolean | null;
  recovery_verified: boolean;
  last_verified_at?: string | null;
}

/** 恢复演练报告（沙盒打开备份并比对，不替换活动库）。 */
export interface DrillReport {
  checked_files: string[];
  main_opened?: boolean | null;
  recovery_opened: boolean;
  entry_count: number;
  entries_match: boolean;
}

/* ---------------- W8 转换器（commands/converters.rs） ---------------- */

/** 转换器列表行（验签结果由 core 信任链给出，UI 只呈现）。 */
export interface ConverterView {
  id: string;
  name: string;
  version: string;
  level: AccessLevel;
  readable_paths: string[];
  signer: string;
  verified: boolean;
  verify_error?: string | null;
}

/* ---------------- W9 配对（commands/pairing.rs ← ab_sync） ---------------- */

/** 配对载荷（QR 内容 + 展示字段；宿主负责渲染图案）。 */
export interface OfferView {
  offer_json: string;
  device_id: string;
  device_name: string;
  public_key_hex: string;
}

/** 设备平台（serde lowercase）。 */
export type DevicePlatform = 'windows' | 'ios' | 'android';

/** 已配对 / 待确认设备（含指纹，供双方比对）。 */
export interface PairedDevice {
  device_id: string;
  device_name: string;
  platform: DevicePlatform;
  public_key: number[];
  fingerprint: string;
  paired_at_secs: number;
}
