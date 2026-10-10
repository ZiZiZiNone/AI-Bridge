/**
 * envelope.ts —— SessionEnvelope 编解码 + 结构校验镜像。
 *
 * 事实源：`contracts/session-envelope.schema.json`；语义镜像 `core/contract/src/lib.rs`
 * 的 `SessionEnvelope::validate` / `validate_temporary` / `SessionRecord::validate`
 * （check_schema_version + check_target + check_payload）。
 *
 * 分层：JSON 形状（必填、类型、值域、additionalProperties:false）由本文件按 schema 校验，
 * 字段之间的结构一致性镜像 ab-contract。**业务语义**（TTL 策略、scope 授权、是否允许扩散）
 * 一律属于 core（ab-broker / ab-mcp），不写在本层。
 *
 * 与 Rust 侧有意的差异（见 README「已知边界」）：
 * - 本层拒绝额外字段（schema 为 additionalProperties:false，静默丢字段比报错更危险）；
 *   Rust serde 默认忽略未知键。
 * - 不校验 date-time 的 format（与 ab-contract 一致，format 在 JSON Schema 里是断言而非结构）。
 */
import type {
  ClientEnv,
  Cookie,
  Payload,
  PlainCredential,
  ScopeSpec,
  SessionEnvelope,
  SessionKind,
  SessionRecord,
  Source,
  Target,
} from "./types.ts";
import {
  CLIENT_ENV_KEYS,
  CLIENT_ENV_REQUIRED_KEYS,
  COOKIE_KEYS,
  COOKIE_REQUIRED_KEYS,
  PAYLOAD_KEYS,
  PAYLOAD_REQUIRED_KEYS,
  PLAIN_CREDENTIAL_KEYS,
  PLAIN_CREDENTIAL_REQUIRED_KEYS,
  SAME_SITE_VALUES,
  SCHEMA_VERSION,
  SCOPE_KEYS,
  SCOPE_REQUIRED_KEYS,
  SESSION_ENVELOPE_KEYS,
  SESSION_ENVELOPE_REQUIRED_KEYS,
  SESSION_KINDS,
  SOURCE_KEYS,
  SOURCE_REQUIRED_KEYS,
  SOURCE_TYPES,
  TARGET_KEYS,
  TARGET_KINDS,
  TARGET_REQUIRED_KEYS,
} from "./types.ts";

/* ---------------- 错误模型（镜像 ab-contract 的 ContractError） ---------------- */

/** JSON 形状类错误的码（schema 形状检查专用）。 */
export type ShapeErrorCode = "missing_field" | "unexpected_field";

/** 结构校验错误码；前七项与 ab-contract 的 ContractError 变体一一对应。 */
export type ContractErrorCode =
  | ShapeErrorCode
  | "unsupported_schema_version"
  | "invalid_id"
  | "empty_field"
  | "target_field_missing"
  | "payload_mismatch"
  | "unsupported_algorithm"
  | "missing_expiry"
  | "invalid_type"
  | "invalid_enum_value";

export interface ContractError {
  code: ContractErrorCode;
  /** JSON 路径，根为 `$`（如 `payload.cookies[0].same_site`）。 */
  path: string;
  detail: string;
}

export interface ShapeError {
  code: ShapeErrorCode;
  path: string;
  detail: string;
}

/** 校验结果：与 ab-contract 的 `Result<(), ContractError>` 同形，不抛异常。 */
export type ValidationResult = { ok: true } | { ok: false; error: ContractError };

type Parsed<T> = { ok: true; value: T } | { ok: false; error: ContractError };

/* ---------------- 形状检查工具（bridge.ts 复用同一份） ---------------- */

export function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** 拼 JSON 路径（根为 `$`）。 */
export function childPath(path: string, key: string): string {
  return `${path}.${key}`;
}

/** 按 schema 校验键集合：必填齐全 + 不接受额外字段（additionalProperties:false）。 */
export function checkShape(
  value: Record<string, unknown>,
  path: string,
  keys: readonly string[],
  required: readonly string[],
): ShapeError | undefined {
  for (const key of required) {
    if (value[key] === undefined) {
      return {
        code: "missing_field",
        path: childPath(path, key),
        detail: "必填字段缺失",
      };
    }
  }
  for (const key of Object.keys(value)) {
    if (!keys.includes(key)) {
      return {
        code: "unexpected_field",
        path: childPath(path, key),
        detail: "schema 为 additionalProperties:false，不接受额外字段",
      };
    }
  }
  return undefined;
}

function error(code: ContractErrorCode, path: string, detail: string): ContractError {
  return { code, path, detail };
}

function invalidType(path: string, detail: string): Parsed<never> {
  return { ok: false, error: error("invalid_type", path, detail) };
}

function invalidEnum(path: string, values: readonly string[], value: unknown): Parsed<never> {
  return {
    ok: false,
    error: error(
      "invalid_enum_value",
      path,
      `取值须为 ${values.join(" | ")}，收到 ${String(value)}`,
    ),
  };
}

function parseString(value: unknown, path: string): Parsed<string> {
  if (typeof value !== "string") return invalidType(path, `应为 string，实为 ${typeof value}`);
  return { ok: true, value };
}

function parseBoolean(value: unknown, path: string): Parsed<boolean> {
  if (typeof value !== "boolean") return invalidType(path, `应为 boolean，实为 ${typeof value}`);
  return { ok: true, value };
}

function parseEnum<T extends string>(value: unknown, values: readonly T[], path: string): Parsed<T> {
  if (typeof value !== "string") return invalidType(path, `应为 string，实为 ${typeof value}`);
  if (!(values as readonly string[]).includes(value)) return invalidEnum(path, values, value);
  return { ok: true, value: value as T };
}

/* ---------------- schema 形状解析（顺带产出类型化对象） ---------------- */

function parseCookie(value: unknown, path: string): Parsed<Cookie> {
  if (!isPlainObject(value)) return invalidType(path, "cookie 应为对象");
  const shape = checkShape(value, path, COOKIE_KEYS, COOKIE_REQUIRED_KEYS);
  if (shape !== undefined) return { ok: false, error: shape };

  const name = parseString(value.name, childPath(path, "name"));
  if (!name.ok) return name;
  const rawValue = parseString(value.value, childPath(path, "value"));
  if (!rawValue.ok) return rawValue;

  const cookie: Cookie = { name: name.value, value: rawValue.value };

  for (const key of ["domain", "path"] as const) {
    if (value[key] !== undefined) {
      const parsed = parseString(value[key], childPath(path, key));
      if (!parsed.ok) return parsed;
      cookie[key] = parsed.value;
    }
  }
  for (const key of ["secure", "http_only"] as const) {
    if (value[key] !== undefined) {
      const parsed = parseBoolean(value[key], childPath(path, key));
      if (!parsed.ok) return parsed;
      cookie[key] = parsed.value;
    }
  }
  if (value.same_site !== undefined) {
    const parsed = parseEnum(value.same_site, SAME_SITE_VALUES, childPath(path, "same_site"));
    if (!parsed.ok) return parsed;
    cookie.same_site = parsed.value;
  }
  if (value.expires_at !== undefined) {
    const parsed = parseString(value.expires_at, childPath(path, "expires_at"));
    if (!parsed.ok) return parsed;
    cookie.expires_at = parsed.value;
  }

  return { ok: true, value: cookie };
}

function parsePayload(value: unknown, path: string): Parsed<Payload> {
  if (!isPlainObject(value)) return invalidType(path, "payload 应为对象");
  const shape = checkShape(value, path, PAYLOAD_KEYS, PAYLOAD_REQUIRED_KEYS);
  if (shape !== undefined) return { ok: false, error: shape };

  const payload: Payload = {};

  if (value.cookies !== undefined) {
    const cookiesPath = childPath(path, "cookies");
    if (!Array.isArray(value.cookies)) return invalidType(cookiesPath, "应为数组");
    const cookies: Cookie[] = [];
    for (let index = 0; index < value.cookies.length; index += 1) {
      const parsed = parseCookie(value.cookies[index], `${cookiesPath}[${index}]`);
      if (!parsed.ok) return parsed;
      cookies.push(parsed.value);
    }
    payload.cookies = cookies;
  }

  if (value.token !== undefined) {
    const parsed = parseString(value.token, childPath(path, "token"));
    if (!parsed.ok) return parsed;
    payload.token = parsed.value;
  }

  if (value.credential !== undefined) {
    const credentialPath = childPath(path, "credential");
    const raw = value.credential;
    if (!isPlainObject(raw)) return invalidType(credentialPath, "应为对象");
    const credentialShape = checkShape(
      raw,
      credentialPath,
      PLAIN_CREDENTIAL_KEYS,
      PLAIN_CREDENTIAL_REQUIRED_KEYS,
    );
    if (credentialShape !== undefined) return { ok: false, error: credentialShape };
    const username = parseString(raw.username, childPath(credentialPath, "username"));
    if (!username.ok) return username;
    const secret = parseString(raw.secret, childPath(credentialPath, "secret"));
    if (!secret.ok) return secret;
    const credential: PlainCredential = { username: username.value, secret: secret.value };
    payload.credential = credential;
  }

  if (value.local_storage !== undefined) {
    const storagePath = childPath(path, "local_storage");
    const raw = value.local_storage;
    if (!isPlainObject(raw)) return invalidType(storagePath, "应为对象");
    const storage: Record<string, string> = {};
    for (const key of Object.keys(raw)) {
      const parsed = parseString(raw[key], childPath(storagePath, key));
      if (!parsed.ok) return parsed;
      storage[key] = parsed.value;
    }
    payload.local_storage = storage;
  }

  return { ok: true, value: payload };
}

function parseTarget(value: unknown, path: string): Parsed<Target> {
  if (!isPlainObject(value)) return invalidType(path, "target 应为对象");
  const shape = checkShape(value, path, TARGET_KEYS, TARGET_REQUIRED_KEYS);
  if (shape !== undefined) return { ok: false, error: shape };

  const kind = parseEnum(value.kind, TARGET_KINDS, childPath(path, "kind"));
  if (!kind.ok) return kind;
  const target: Target = { kind: kind.value };

  for (const key of ["domain", "app_id"] as const) {
    if (value[key] !== undefined) {
      const parsed = parseString(value[key], childPath(path, key));
      if (!parsed.ok) return parsed;
      target[key] = parsed.value;
    }
  }

  return { ok: true, value: target };
}

function parseSource(value: unknown, path: string): Parsed<Source> {
  if (!isPlainObject(value)) return invalidType(path, "source 应为对象");
  const shape = checkShape(value, path, SOURCE_KEYS, SOURCE_REQUIRED_KEYS);
  if (shape !== undefined) return { ok: false, error: shape };

  const type = parseEnum(value.type, SOURCE_TYPES, childPath(path, "type"));
  if (!type.ok) return type;
  const id = parseString(value.id, childPath(path, "id"));
  if (!id.ok) return id;

  return { ok: true, value: { type: type.value, id: id.value } };
}

/** 只做形状校验：scope 的**业务强制**（授权、TTL）在 ab-broker，不在本层。 */
function parseScope(value: unknown, path: string): Parsed<ScopeSpec> {
  if (!isPlainObject(value)) return invalidType(path, "scope 应为对象");
  const shape = checkShape(value, path, SCOPE_KEYS, SCOPE_REQUIRED_KEYS);
  if (shape !== undefined) return { ok: false, error: shape };

  const allowlistPath = childPath(path, "domain_allowlist");
  if (!Array.isArray(value.domain_allowlist)) return invalidType(allowlistPath, "应为数组");
  const domainAllowlist: string[] = [];
  for (let index = 0; index < value.domain_allowlist.length; index += 1) {
    const parsed = parseString(value.domain_allowlist[index], `${allowlistPath}[${index}]`);
    if (!parsed.ok) return parsed;
    domainAllowlist.push(parsed.value);
  }
  const readOnly = parseBoolean(value.read_only, childPath(path, "read_only"));
  if (!readOnly.ok) return readOnly;
  const singleUse = parseBoolean(value.single_use, childPath(path, "single_use"));
  if (!singleUse.ok) return singleUse;

  return {
    ok: true,
    value: {
      domain_allowlist: domainAllowlist,
      read_only: readOnly.value,
      single_use: singleUse.value,
    },
  };
}

function parseClientEnv(value: unknown, path: string): Parsed<ClientEnv> {
  if (!isPlainObject(value)) return invalidType(path, "client_env 应为对象");
  const shape = checkShape(value, path, CLIENT_ENV_KEYS, CLIENT_ENV_REQUIRED_KEYS);
  if (shape !== undefined) return { ok: false, error: shape };

  const userAgent = parseString(value.user_agent, childPath(path, "user_agent"));
  if (!userAgent.ok) return userAgent;
  const platform = parseString(value.platform, childPath(path, "platform"));
  if (!platform.ok) return platform;
  const clientEnv: ClientEnv = { user_agent: userAgent.value, platform: platform.value };

  if (value.device_id !== undefined) {
    const parsed = parseString(value.device_id, childPath(path, "device_id"));
    if (!parsed.ok) return parsed;
    clientEnv.device_id = parsed.value;
  }

  return { ok: true, value: clientEnv };
}

/** 按 session-envelope schema 解析信封（不做 schema_version 值域与 target/payload 语义判断）。 */
function parseSessionEnvelope(value: unknown): Parsed<SessionEnvelope> {
  if (!isPlainObject(value)) return invalidType("$", "SessionEnvelope 应为对象");
  const shape = checkShape(value, "$", SESSION_ENVELOPE_KEYS, SESSION_ENVELOPE_REQUIRED_KEYS);
  if (shape !== undefined) return { ok: false, error: shape };

  const schemaVersion = parseString(value.schema_version, "$.schema_version");
  if (!schemaVersion.ok) return schemaVersion;
  const accountRef = parseString(value.account_ref, "$.account_ref");
  if (!accountRef.ok) return accountRef;
  const kind = parseEnum(value.kind, SESSION_KINDS, "$.kind");
  if (!kind.ok) return kind;
  const target = parseTarget(value.target, "$.target");
  if (!target.ok) return target;
  const payload = parsePayload(value.payload, "$.payload");
  if (!payload.ok) return payload;
  const issuedAt = parseString(value.issued_at, "$.issued_at");
  if (!issuedAt.ok) return issuedAt;
  const source = parseSource(value.source, "$.source");
  if (!source.ok) return source;

  let expiresAtText: string | undefined;
  if (value.expires_at !== undefined) {
    const expiresAt = parseString(value.expires_at, "$.expires_at");
    if (!expiresAt.ok) return expiresAt;
    expiresAtText = expiresAt.value;
  }

  let scope: ScopeSpec | undefined;
  if (value.scope !== undefined) {
    const parsed = parseScope(value.scope, "$.scope");
    if (!parsed.ok) return parsed;
    scope = parsed.value;
  }

  let clientEnv: ClientEnv | undefined;
  if (value.client_env !== undefined) {
    const parsed = parseClientEnv(value.client_env, "$.client_env");
    if (!parsed.ok) return parsed;
    clientEnv = parsed.value;
  }

  // 字段插入顺序 = schema 声明顺序，encode 输出因此稳定可对账
  const envelope: SessionEnvelope = {
    schema_version: schemaVersion.value,
    account_ref: accountRef.value,
    target: target.value,
    kind: kind.value,
    payload: payload.value,
    issued_at: issuedAt.value,
    ...(expiresAtText === undefined ? {} : { expires_at: expiresAtText }),
    source: source.value,
    ...(scope === undefined ? {} : { scope }),
    ...(clientEnv === undefined ? {} : { client_env: clientEnv }),
  };

  return { ok: true, value: envelope };
}

/* ---------------- 结构一致性（镜像 ab-contract 的语义校验） ---------------- */

function isEmpty(value: string | undefined): boolean {
  return value === undefined || value.length === 0;
}

function checkSchemaVersion(version: string): ContractError | undefined {
  if (version !== SCHEMA_VERSION) {
    return error(
      "unsupported_schema_version",
      "$.schema_version",
      `不支持的契约版本 ${version}（当前 ${SCHEMA_VERSION}）`,
    );
  }
  return undefined;
}

function checkTarget(target: Target): ContractError | undefined {
  if (target.kind === "web" && isEmpty(target.domain)) {
    return error("target_field_missing", "target.domain", "kind=web 时 domain 必填");
  }
  if (target.kind === "app" && isEmpty(target.app_id)) {
    return error("target_field_missing", "target.app_id", "kind=app 时 app_id 必填");
  }
  return undefined;
}

function checkPayload(kind: SessionKind, payload: Payload): ContractError | undefined {
  if (kind === "cookie") {
    if (payload.cookies === undefined || payload.cookies.length === 0) {
      return error("payload_mismatch", "payload.cookies", "kind=cookie 需要非空 cookies");
    }
  }
  if (kind === "token") {
    if (isEmpty(payload.token)) {
      return error("payload_mismatch", "payload.token", "kind=token 需要非空 token");
    }
  }
  if (kind === "credential") {
    if (payload.credential === undefined) {
      return error("payload_mismatch", "payload.credential", "kind=credential 需要 credential");
    }
  }
  return undefined;
}

/** 校验只用到这三个字段（镜像 `SessionRecord::validate` 的入参面）。 */
export type SessionPart = Pick<SessionRecord, "target" | "kind" | "payload">;

/**
 * 会话记录结构校验：target 必填规则 + kind/payload 一致性。
 * standard-access 的 `sessions[]` 与 session-envelope 共用同一份规则。
 */
export function validateSessionRecord(record: SessionPart): ValidationResult {
  const problem = checkTarget(record.target) ?? checkPayload(record.kind, record.payload);
  return problem === undefined ? { ok: true } : { ok: false, error: problem };
}

function parseAndValidate(value: unknown): Parsed<SessionEnvelope> {
  const parsed = parseSessionEnvelope(value);
  if (!parsed.ok) return parsed;

  const version = checkSchemaVersion(parsed.value.schema_version);
  if (version !== undefined) return { ok: false, error: version };

  const record = validateSessionRecord(parsed.value);
  if (!record.ok) return record;

  return parsed;
}

/** 结构校验镜像（入参为已解析的 JSON 值，不抛异常）。 */
export function validateSessionEnvelope(value: unknown): ValidationResult {
  const parsed = parseAndValidate(value);
  return parsed.ok ? { ok: true } : { ok: false, error: parsed.error };
}

/** 临时会话校验：结构校验 + 必须携带 `expires_at`（TTL 策略在 ab-broker）。 */
export function validateTemporaryEnvelope(envelope: SessionEnvelope): ValidationResult {
  const parsed = parseAndValidate(envelope);
  if (!parsed.ok) return { ok: false, error: parsed.error };
  if (isEmpty(parsed.value.expires_at)) {
    return {
      ok: false,
      error: error("missing_expiry", "$.expires_at", "临时会话必须携带 expires_at"),
    };
  }
  return { ok: true };
}

/* ---------------- 编解码 ---------------- */

/** 编解码错误：`invalid_json` 为 JSON 语法层，其余沿用 ContractErrorCode。 */
export class EnvelopeError extends Error {
  code: ContractErrorCode | "invalid_json";
  path: string;

  constructor(code: ContractErrorCode | "invalid_json", path: string, detail: string) {
    super(`SessionEnvelope 编解码失败：${code} @ ${path} —— ${detail}`);
    this.name = "EnvelopeError";
    this.code = code;
    this.path = path;
  }
}

/**
 * 编码：先按同一份规则校验，再序列化**校验后重新构造的对象**。
 * 因此输出字段与 schema 声明一致、顺序固定、绝无额外字段；非法信封不产生字节。
 */
export function encodeEnvelope(envelope: SessionEnvelope): string {
  const parsed = parseAndValidate(envelope);
  if (!parsed.ok) {
    throw new EnvelopeError(parsed.error.code, parsed.error.path, parsed.error.detail);
  }
  return JSON.stringify(parsed.value);
}

/** 解码：JSON 语法 → schema 形状 → 结构一致性；任一步失败抛 EnvelopeError。 */
export function decodeEnvelope(text: string): SessionEnvelope {
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch (cause) {
    throw new EnvelopeError("invalid_json", "$", `不是合法 JSON：${String(cause)}`);
  }
  const parsed = parseAndValidate(value);
  if (!parsed.ok) {
    throw new EnvelopeError(parsed.error.code, parsed.error.path, parsed.error.detail);
  }
  return parsed.value;
}
