/**
 * 契约镜像（前端侧，零逻辑）。
 *
 * 唯一事实源：仓库根 `contracts/*.schema.json`；
 * Rust 侧唯一实现：`core/contract`（ab-contract）。
 * 本文件只镜像数据结构，字段与 schema 逐字段一致（一律 snake_case），
 * 由 `npm run check:structure` 逐字段校验；禁止在此写任何业务规则或校验逻辑。
 */

/** 契约版本；与 schema_version 不符的数据一律视为不兼容。 */
export const SCHEMA_VERSION = '1.0';

/* ---------------- 枚举（与 schema enum / core/contract 枚举一致） ---------------- */

/** 接入等级：c = 会话注入，b = 凭据填表，a = 仅启动。 */
export type AccessLevel = 'c' | 'b' | 'a';

export type CredentialKind = 'password' | 'passkey' | 'totp';

export type SessionKind = 'cookie' | 'token' | 'credential';

export type TargetKind = 'web' | 'app' | 'container';

export type SameSite = 'strict' | 'lax' | 'none';

export type SourceType = 'extension' | 'converter' | 'mobile' | 'manual' | 'sync';

/* ---------------- 公共结构 ---------------- */

/** cookie 属性必须完整携带：会话保真度关键，任何环节不得丢弃。 */
export interface Cookie {
  name: string;
  value: string;
  domain?: string;
  path?: string;
  secure?: boolean;
  http_only?: boolean;
  same_site?: SameSite;
  expires_at?: string;
}

export interface Target {
  kind: TargetKind;
  domain?: string;
  app_id?: string;
}

export interface PlainCredential {
  username: string;
  secret: string;
}

/** 按 kind 取用：cookie→cookies；token→token；credential→credential。 */
export interface Payload {
  cookies?: Cookie[];
  token?: string;
  credential?: PlainCredential;
  local_storage?: Record<string, string>;
}

export interface Source {
  type: SourceType;
  id: string;
}

/** 临时会话授权范围：AI 的能力上限（session-envelope.schema.json scope）。 */
export interface ScopeSpec {
  domain_allowlist: string[];
  read_only: boolean;
  single_use: boolean;
}

/** 会话捕获环境（UA 绑定）：捕获时记录、扩散随行、注入容器时对齐。 */
export interface ClientEnv {
  user_agent: string;
  platform: string;
  device_id?: string;
}

/* ---------------- StandardAccess（standard-access.schema.json） ---------------- */

export interface CredentialRecord {
  account_ref: string;
  kind: CredentialKind;
  username?: string;
  secret?: string;
  totp_seed?: string;
  url?: string;
}

export interface SessionRecord {
  account_ref: string;
  target: Target;
  kind: SessionKind;
  payload: Payload;
  expires_at?: string;
  source: Source;
  client_env?: ClientEnv;
}

export interface StandardAccess {
  schema_version: string;
  app_id: string;
  level: AccessLevel;
  generated_at: string;
  credentials?: CredentialRecord[];
  sessions?: SessionRecord[];
  meta?: Record<string, unknown>;
  client_env?: ClientEnv;
}

/* ---------------- SessionEnvelope（session-envelope.schema.json） ---------------- */

export interface SessionEnvelope {
  schema_version: string;
  account_ref: string;
  target: Target;
  kind: SessionKind;
  payload: Payload;
  issued_at: string;
  expires_at?: string;
  source: Source;
  scope?: ScopeSpec;
  client_env?: ClientEnv;
}

/* ---------------- ConverterManifest（converter-manifest.schema.json） ---------------- */

export interface Signature {
  algorithm: string;
  signer: string;
  value: string;
}

export interface ConverterManifest {
  id: string;
  name: string;
  version: string;
  level: AccessLevel;
  readable_paths: string[];
  entry: string;
  module_sha256: string;
  signature: Signature;
  min_app_version?: string;
}
