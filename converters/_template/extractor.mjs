/**
 * _template/extractor.mjs —— 取数环节参考实现。
 *
 * 职责：只用宿主注入的只读能力（capabilities.readFile）读取 manifest 声明的路径，
 *       产出**转换器自定义**的中间格式（ExtractorOutput）。
 * 边界：
 *   - 不做归一化决策：不产出 schema_version / level / sessions / credentials / meta
 *     —— 出现这些字段即视为越界（conformance 检查会拦下）。
 *   - 不联网、不写文件、不取时间（时间一律用宿主注入的 ctx.now）。
 *   - 中间格式只在沙盒内存在，绝不作为对外产物。
 *
 * 真实转换器可以读 SQLite / LevelDB / protobuf / 注册表导出；模板用 JSON 夹具示范职责边界。
 */

import { LOG_INFO } from "../sdk/abi.mjs";

/** 与 manifest.readable_paths 中的声明逐字一致（宿主按此白名单放行）。 */
const STORE_PATH = "{APPDATA}\\sample-app\\session-store.json";

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
        plain.push({ account_ref: account.account_ref, ...item });
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
    accounts.push({ account_ref: account.account_ref, site: account.site });
    for (const cookie of account.session?.cookies ?? []) {
      cookies.push({
        account_ref: account.account_ref,
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
        account_ref: account.account_ref,
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
