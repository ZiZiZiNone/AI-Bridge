/**
 * qq/extractor.mjs —— 取数环节（QQ 首接，B4.3）。
 *
 * 职责：只用宿主注入的只读能力（capabilities.readFile）读取 manifest 声明的路径，
 *       产出**转换器自定义**的中间格式（ExtractorOutput）。
 * 边界：
 *   - 不做归一化决策：账号身份映射（uin → account_ref）在 normalizer；
 *     中间格式不出现 schema_version / level / sessions / credentials / meta。
 *   - 不联网、不写文件、不取时间（时间一律用宿主注入的 ctx.now）。
 *
 * 【待确认 · 开工首验后校准】QQ 本地数据产物的真实路径与格式尚未取得事实
 * （docs 仅记载「QQ 首接（开工首验取数，失败自动降 b/a）」）。本文件的
 * STORE_PATH 与数据产物形状为占位实现，验收口径只有两条：
 *   1) 与 manifest.readable_paths 逐字一致（宿主白名单）；
 *   2) conformance / schema-check 实跑通过（管线有效）。
 * 首验拿到真实产物后，只改本文件的读取与解析段，normalizer 的身份映射与义务不变。
 *
 * 降级语义（失败自动降 b/a）：三个等级的取数分支均已实现（c 会话 / b 凭据 / a 仅启动），
 * 但「产物 level 必须与清单一致」与运行时降级的衔接待首验裁决（见 README 待确认清单）。
 */

import { LOG_INFO } from "../sdk/abi.mjs";

/** 与 manifest.readable_paths 中的声明逐字一致（宿主按此白名单放行）。【待确认】 */
const STORE_PATH = "{APPDATA}\\Tencent\\QQ\\session-store.json";

const LEVELS = ["c", "b", "a"];

/**
 * @param {{ readFile: (path: string) => string, log: (level: number, message: string) => void }} capabilities
 * @param {{ level: "c"|"b"|"a", now: string }} ctx
 */
export function extract(capabilities, ctx) {
  if (!LEVELS.includes(ctx.level)) throw new Error(`未知接入等级：${ctx.level}`);

  const store = JSON.parse(capabilities.readFile(STORE_PATH));
  const output = { collected_at: ctx.now, source_files: [STORE_PATH] };

  // level=a：只保留启动所需信息，账号 / 会话 / 凭据一律不进中间产物。
  if (ctx.level === "a") {
    output.launch = {
      app_id: store.launch.app_id,
      executable_hint: store.launch.executable_hint,
    };
    capabilities.log(LOG_INFO, "extract: level=a 仅保留启动信息，未采集账号/会话/凭据");
    return output;
  }

  // level=b：只取凭据，不取会话。
  if (ctx.level === "b") {
    const plain = [];
    for (const account of store.accounts ?? []) {
      for (const item of account.plain_credentials ?? []) {
        plain.push({ uin: account.uin, ...item });
      }
    }
    output.plain_credentials = plain;
    capabilities.log(LOG_INFO, `extract: level=b 账号=${(store.accounts ?? []).length} 凭据=${plain.length}，未采集会话`);
    return output;
  }

  // level=c：取会话（cookie / token）及其站点归属，不取明文凭据。
  const accounts = [];
  const cookies = [];
  const tokens = [];
  for (const account of store.accounts ?? []) {
    accounts.push({ uin: account.uin, site: account.site });
    for (const cookie of account.session?.cookies ?? []) {
      cookies.push({
        uin: account.uin,
        name: cookie.name,
        value: cookie.value,
        domain: cookie.domain,
        path: cookie.path,
        secure: cookie.secure,
        http_only: cookie.http_only,
        same_site: cookie.same_site,
        // 会话 cookie（无过期时间）保持缺省，不用 null 占位
        ...(cookie.expires_at ? { expires_at: cookie.expires_at } : {}),
      });
    }
    if (account.session?.token) {
      tokens.push({
        uin: account.uin,
        token: account.session.token,
        ...(account.session.token_expires_at ? { expires_at: account.session.token_expires_at } : {}),
      });
    }
  }
  output.accounts = accounts;
  output.cookies = cookies;
  output.tokens = tokens;
  capabilities.log(LOG_INFO, `extract: level=c 账号=${accounts.length} cookie=${cookies.length} token=${tokens.length}`);
  return output;
}
