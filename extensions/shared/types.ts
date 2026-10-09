/**
 * contracts/ 三个 schema 的 TypeScript 镜像 —— 扩展共享层的唯一契约来源。
 *
 * 事实源：仓库根 `contracts/standard-access.schema.json`、
 *         `contracts/converter-manifest.schema.json`、`contracts/session-envelope.schema.json`；
 *         Rust 对应实现为 `core/contract/src/lib.rs`（ab-contract）。
 *
 * 本文件只放数据定义（枚举值域 + 键集合 + 接口），零业务规则、零宿主依赖。
 * 字段名一律 snake_case，与 schema 逐字段一致；`*_KEYS` / `*_REQUIRED_KEYS`
 * 是同一份字段清单的运行时形态，供 envelope.ts / bridge.ts 做结构校验，
 * 并由 test/types.test.ts 与 schema 原文逐字段比对。
 *
 * 编译期双保险：每个接口配 `*KeysComplete` 断言，接口新增字段而键集合未同步时 tsc 直接报错
 * （运行时无法枚举 interface 字段，只能靠类型系统锁）。
 */

/** 键集合常量必须逐项存在，且必须覆盖接口全部字段（由 tsc 校验）。 */
export type AssertNever<T extends never> = T;

/** 契约版本；`schema_version` 与之不符一律拒绝（与 schema 的 `const` 一致）。 */
export const SCHEMA_VERSION = "1.0";

/* ---------------- 枚举值域（与 schema 的 enum / const 逐值一致） ---------------- */

/** 接入等级：c = 会话注入，b = 凭据填表，a = 仅启动。 */
export const ACCESS_LEVELS = ["c", "b", "a"] as const;
export type AccessLevel = (typeof ACCESS_LEVELS)[number];

export const CREDENTIAL_KINDS = ["password", "passkey", "totp"] as const;
export type CredentialKind = (typeof CREDENTIAL_KINDS)[number];

export const SESSION_KINDS = ["cookie", "token", "credential"] as const;
export type SessionKind = (typeof SESSION_KINDS)[number];

export const TARGET_KINDS = ["web", "app", "container"] as const;
export type TargetKind = (typeof TARGET_KINDS)[number];

export const SAME_SITE_VALUES = ["strict", "lax", "none"] as const;
export type SameSite = (typeof SAME_SITE_VALUES)[number];

export const SOURCE_TYPES = ["extension", "converter", "mobile", "manual", "sync"] as const;
export type SourceType = (typeof SOURCE_TYPES)[number];

/** `signature.algorithm` 的唯一合法值。 */
export const SIGNATURE_ALGORITHM = "ed25519";

/* ---------------- 形状约束（schema 的 pattern / format） ---------------- */

/** `app_id`（standard-access）与 `id`（converter-manifest）共用的形状。 */
export const ID_PATTERN = /^[a-z0-9][a-z0-9-]{1,63}$/;

/** `converter-manifest.version` 的形状。 */
export const VERSION_PATTERN = /^[0-9]+\.[0-9]+\.[0-9]+$/;

/** 契约内全部 date-time 字段的形状（RFC 3339 date-time）。 */
export const DATE_TIME_PATTERN = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:\d{2})$/;

/* ---------------- standard-access / session-envelope 公共结构 ---------------- */

/**
 * cookie 必须带完整属性：会话保真度关键，任何环节不得丢弃。
 * 字段与 schema `$defs.cookie`（以及 session-envelope 内联 cookie）完全一致。
 */
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

export const COOKIE_KEYS = [
  "name",
  "value",
  "domain",
  "path",
  "secure",
  "http_only",
  "same_site",
  "expires_at",
] as const satisfies readonly (keyof Cookie & string)[];

export const COOKIE_REQUIRED_KEYS = [
  "name",
  "value",
] as const satisfies readonly (typeof COOKIE_KEYS)[number][];

export type CookieKeysComplete = AssertNever<
  Exclude<keyof Cookie & string, (typeof COOKIE_KEYS)[number]>
>;

/** kind = web 时 `domain` 必填，kind = app 时 `app_id` 必填（由 validate 校验）。 */
export interface Target {
  kind: TargetKind;
  domain?: string;
  app_id?: string;
}

export const TARGET_KEYS = ["kind", "domain", "app_id"] as const satisfies readonly (keyof Target & string)[];

export const TARGET_REQUIRED_KEYS = ["kind"] as const satisfies readonly (typeof TARGET_KEYS)[number][];

export type TargetKeysComplete = AssertNever<
  Exclude<keyof Target & string, (typeof TARGET_KEYS)[number]>
>;

export interface PlainCredential {
  username: string;
  secret: string;
}

export const PLAIN_CREDENTIAL_KEYS = [
  "username",
  "secret",
] as const satisfies readonly (keyof PlainCredential & string)[];

export const PLAIN_CREDENTIAL_REQUIRED_KEYS = [
  "username",
  "secret",
] as const satisfies readonly (typeof PLAIN_CREDENTIAL_KEYS)[number][];

export type PlainCredentialKeysComplete = AssertNever<
  Exclude<keyof PlainCredential & string, (typeof PLAIN_CREDENTIAL_KEYS)[number]>
>;

/**
 * 按 kind 取用：cookie→cookies；token→token；credential→credential。
 * 一致性由核心 `validate` 校验（见 envelope.ts 镜像）。
 */
export interface Payload {
  cookies?: Cookie[];
  token?: string;
  credential?: PlainCredential;
  local_storage?: Record<string, string>;
}

export const PAYLOAD_KEYS = [
  "cookies",
  "token",
  "credential",
  "local_storage",
] as const satisfies readonly (keyof Payload & string)[];

export const PAYLOAD_REQUIRED_KEYS = [] as const satisfies readonly (typeof PAYLOAD_KEYS)[number][];

export type PayloadKeysComplete = AssertNever<
  Exclude<keyof Payload & string, (typeof PAYLOAD_KEYS)[number]>
>;

/** `type` 是保留字，字段名与 schema 一致。 */
export interface Source {
  type: SourceType;
  id: string;
}

export const SOURCE_KEYS = ["type", "id"] as const satisfies readonly (keyof Source & string)[];

export const SOURCE_REQUIRED_KEYS = [
  "type",
  "id",
] as const satisfies readonly (typeof SOURCE_KEYS)[number][];

export type SourceKeysComplete = AssertNever<
  Exclude<keyof Source & string, (typeof SOURCE_KEYS)[number]>
>;

/* ---------------- standard-access.schema.json ---------------- */

export interface CredentialRecord {
  account_ref: string;
  kind: CredentialKind;
  username?: string;
  secret?: string;
  totp_seed?: string;
  url?: string;
}

export const CREDENTIAL_RECORD_KEYS = [
  "account_ref",
  "kind",
  "username",
  "secret",
  "totp_seed",
  "url",
] as const satisfies readonly (keyof CredentialRecord & string)[];

export const CREDENTIAL_RECORD_REQUIRED_KEYS = [
  "account_ref",
  "kind",
] as const satisfies readonly (typeof CREDENTIAL_RECORD_KEYS)[number][];

export type CredentialRecordKeysComplete = AssertNever<
  Exclude<keyof CredentialRecord & string, (typeof CREDENTIAL_RECORD_KEYS)[number]>
>;

export interface SessionRecord {
  account_ref: string;
  target: Target;
  kind: SessionKind;
  payload: Payload;
  expires_at?: string;
  source: Source;
}

export const SESSION_RECORD_KEYS = [
  "account_ref",
  "target",
  "kind",
  "payload",
  "expires_at",
  "source",
] as const satisfies readonly (keyof SessionRecord & string)[];

export const SESSION_RECORD_REQUIRED_KEYS = [
  "account_ref",
  "target",
  "kind",
  "payload",
  "source",
] as const satisfies readonly (typeof SESSION_RECORD_KEYS)[number][];

export type SessionRecordKeysComplete = AssertNever<
  Exclude<keyof SessionRecord & string, (typeof SESSION_RECORD_KEYS)[number]>
>;

/** 一切端向核心输出的统一格式。 */
export interface StandardAccess {
  schema_version: string;
  app_id: string;
  level: AccessLevel;
  generated_at: string;
  credentials?: CredentialRecord[];
  sessions?: SessionRecord[];
  /** 来源自述信息（转换器版本、采集时间等），仅展示用。 */
  meta?: Record<string, unknown>;
}

export const STANDARD_ACCESS_KEYS = [
  "schema_version",
  "app_id",
  "level",
  "generated_at",
  "credentials",
  "sessions",
  "meta",
] as const satisfies readonly (keyof StandardAccess & string)[];

export const STANDARD_ACCESS_REQUIRED_KEYS = [
  "schema_version",
  "app_id",
  "level",
  "generated_at",
] as const satisfies readonly (typeof STANDARD_ACCESS_KEYS)[number][];

export type StandardAccessKeysComplete = AssertNever<
  Exclude<keyof StandardAccess & string, (typeof STANDARD_ACCESS_KEYS)[number]>
>;

/* ---------------- session-envelope.schema.json ---------------- */

/**
 * 会话信封：扩散 / 临时会话 / 离线传送包(A8) 的统一载体。
 * cookie 属性完整携带，任何环节不得丢弃。
 */
export interface SessionEnvelope {
  schema_version: string;
  account_ref: string;
  target: Target;
  kind: SessionKind;
  payload: Payload;
  issued_at: string;
  /** 临时会话必填（由核心 validate 校验，TTL 策略在 ab-broker）。 */
  expires_at?: string;
  source: Source;
}

export const SESSION_ENVELOPE_KEYS = [
  "schema_version",
  "account_ref",
  "target",
  "kind",
  "payload",
  "issued_at",
  "expires_at",
  "source",
] as const satisfies readonly (keyof SessionEnvelope & string)[];

export const SESSION_ENVELOPE_REQUIRED_KEYS = [
  "schema_version",
  "account_ref",
  "target",
  "kind",
  "payload",
  "issued_at",
  "source",
] as const satisfies readonly (typeof SESSION_ENVELOPE_KEYS)[number][];

export type SessionEnvelopeKeysComplete = AssertNever<
  Exclude<keyof SessionEnvelope & string, (typeof SESSION_ENVELOPE_KEYS)[number]>
>;

/* ---------------- converter-manifest.schema.json ---------------- */

export interface Signature {
  algorithm: typeof SIGNATURE_ALGORITHM;
  signer: string;
  value: string;
}

export const SIGNATURE_KEYS = [
  "algorithm",
  "signer",
  "value",
] as const satisfies readonly (keyof Signature & string)[];

export const SIGNATURE_REQUIRED_KEYS = [
  "algorithm",
  "signer",
  "value",
] as const satisfies readonly (typeof SIGNATURE_KEYS)[number][];

export type SignatureKeysComplete = AssertNever<
  Exclude<keyof Signature & string, (typeof SIGNATURE_KEYS)[number]>
>;

/** 转换器声明（manifest.json）；仅官方签发，signature 校验通过方可启用。 */
export interface ConverterManifest {
  id: string;
  name: string;
  version: string;
  level: AccessLevel;
  /** 可读路径声明：沙盒只开放这些路径的只读访问，超出即拒绝。 */
  readable_paths: string[];
  entry: string;
  signature: Signature;
  min_app_version?: string;
}

export const CONVERTER_MANIFEST_KEYS = [
  "id",
  "name",
  "version",
  "level",
  "readable_paths",
  "entry",
  "signature",
  "min_app_version",
] as const satisfies readonly (keyof ConverterManifest & string)[];

export const CONVERTER_MANIFEST_REQUIRED_KEYS = [
  "id",
  "name",
  "version",
  "level",
  "readable_paths",
  "entry",
  "signature",
] as const satisfies readonly (typeof CONVERTER_MANIFEST_KEYS)[number][];

export type ConverterManifestKeysComplete = AssertNever<
  Exclude<keyof ConverterManifest & string, (typeof CONVERTER_MANIFEST_KEYS)[number]>
>;
