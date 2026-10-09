/**
 * _template/normalizer.mjs —— 归一化环节参考实现。
 *
 * 职责：把 extractor 的中间格式转成 StandardAccess（contracts/standard-access.schema.json）。
 * 边界：
 *   - 纯函数：不读文件、不联网、不取时间（时间用 ctx.now）、不改动入参。
 *   - 只做字段搬运与分组，不做校验（结构校验的唯一实现在 ab-contract）。
 *   - 空数组 / 空对象一律省略，与 ab-contract 的 skip_serializing_if 行为一致。
 */

import { STANDARD_ACCESS_SCHEMA_VERSION } from "../sdk/abi.mjs";

const SOURCE_TYPE = "converter";

/**
 * @param {object} raw extractor 的中间格式（非契约类型）
 * @param {{ level: "c"|"b"|"a", app_id: string, manifest_id: string, converter_version: string, now: string }} ctx
 */
export function normalize(raw, ctx) {
  const document = {
    schema_version: STANDARD_ACCESS_SCHEMA_VERSION,
    app_id: ctx.app_id,
    level: ctx.level,
    generated_at: ctx.now,
  };

  if (ctx.level === "c") {
    document.sessions = buildSessions(raw, ctx);
  } else if (ctx.level === "b") {
    document.credentials = buildCredentials(raw);
  }
  // level=a：不产出 credentials / sessions（键缺省，符合「仅启动」义务）

  document.meta = buildMeta(raw, ctx);
  return document;
}

function source(ctx) {
  return { type: SOURCE_TYPE, id: ctx.manifest_id };
}

/** target.domain 取账号站点 host（去掉协议与路径）；缺站点直接失败，不猜域名。 */
function webTarget(site) {
  if (!site) throw new Error("缺少账号站点，无法构造 target.domain");
  return { kind: "web", domain: new URL(site).host };
}

/** 取最早过期时间；保留来源字符串原样，不重新格式化（不用 toISOString 改写来源时间）。 */
function earliestExpiry(values) {
  let best;
  let bestTime = Number.POSITIVE_INFINITY;
  for (const value of values) {
    if (!value) continue;
    const time = Date.parse(value);
    if (Number.isNaN(time) || time >= bestTime) continue;
    best = value;
    bestTime = time;
  }
  return best;
}

function buildSessions(raw, ctx) {
  const siteByAccount = new Map((raw.accounts ?? []).map((account) => [account.account_ref, account.site]));

  const sessions = [];
  const cookiesByAccount = new Map();
  for (const cookie of raw.cookies ?? []) {
    if (!cookiesByAccount.has(cookie.account_ref)) cookiesByAccount.set(cookie.account_ref, []);
    cookiesByAccount.get(cookie.account_ref).push({
      name: cookie.name,
      value: cookie.value,
      domain: cookie.domain,
      path: cookie.path,
      secure: cookie.secure,
      http_only: cookie.http_only,
      same_site: cookie.same_site,
      ...(cookie.expires_at ? { expires_at: cookie.expires_at } : {}),
    });
  }

  for (const [accountRef, cookies] of cookiesByAccount) {
    const expiresAt = earliestExpiry(cookies.map((cookie) => cookie.expires_at));
    sessions.push({
      account_ref: accountRef,
      target: webTarget(siteByAccount.get(accountRef)),
      kind: "cookie",
      payload: { cookies },
      ...(expiresAt ? { expires_at: expiresAt } : {}),
      source: source(ctx),
    });
  }

  for (const token of raw.tokens ?? []) {
    sessions.push({
      account_ref: token.account_ref,
      target: webTarget(siteByAccount.get(token.account_ref)),
      kind: "token",
      payload: { token: token.token },
      ...(token.expires_at ? { expires_at: token.expires_at } : {}),
      source: source(ctx),
    });
  }

  return sessions;
}

function buildCredentials(raw) {
  return (raw.plain_credentials ?? []).map((item) => ({
    account_ref: item.account_ref,
    kind: item.kind,
    ...(item.username !== undefined ? { username: item.username } : {}),
    // 敏感字段只在内存中搬运；缺省即省略（passkey 不输出任何密钥材料）
    ...(item.secret !== undefined ? { secret: item.secret } : {}),
    ...(item.totp_seed !== undefined ? { totp_seed: item.totp_seed } : {}),
    ...(item.url !== undefined ? { url: item.url } : {}),
  }));
}

function buildMeta(raw, ctx) {
  const meta = {
    converter_version: ctx.converter_version,
    collected_at: raw.collected_at,
    source_files: [...(raw.source_files ?? [])],
  };
  if (raw.launch) {
    meta.launch = { app_id: raw.launch.app_id, executable_hint: raw.launch.executable_hint };
  }
  return meta;
}
