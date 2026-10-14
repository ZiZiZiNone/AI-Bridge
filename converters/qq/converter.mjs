/**
 * qq/converter.mjs —— 参考入口（对应 manifest.entry = converter.wasm 的导出 `ab_run`）。
 *
 * 它只做 ABI 翻译：JSON 请求 ↔ 进程内调用；不承载任何取数或归一化决策
 * （取数在 extractor.mjs，归一化在 normalizer.mjs）。与 _template/converter.mjs
 * 同为「ABI 翻译层」的机械实现，业务语义在另外两个文件里。
 *
 * ABI 摘要（详见 ../sdk/README.md）：
 *   导出：memory、ab_alloc(len)、ab_free(ptr,len)、ab_run(ptr,len) -> i64
 *   导入：ab_read_file(ptr,len) -> i64（只读、仅声明路径）、ab_log(level,ptr,len)
 *   成功 = (ptr << 32) | len 指向 UTF-8 StandardAccess JSON；失败 = -1
 */

import {
  AB_FAILURE,
  AB_MAX_REQUEST_BYTES,
  AB_MAX_RESPONSE_BYTES,
  AB_MIN_MEMORY_BYTES,
  AB_OP_CONVERT,
  LOG_ERROR,
  callHostString,
  isKnownOp,
  packPtrLen,
  readString,
  utf8Encode,
  writeString,
} from "../sdk/abi.mjs";
import { extract } from "./extractor.mjs";
import { normalize } from "./normalizer.mjs";

const MEMORY_BYTES = 1 << 20;

/** guest 实例工厂：真实宿主加载 wasm 得到的导出集与之等价。 */
export function createInstance(imports) {
  if (MEMORY_BYTES < AB_MIN_MEMORY_BYTES) throw new Error("线性内存低于 ABI 下限");
  const memory = new Uint8Array(MEMORY_BYTES);
  // 参考实现用单调分配器：宿主每次采集新建实例，故无需回收（真实实现改用 wasm 分配器）。
  let bump = 0;

  function ab_alloc(len) {
    if (!Number.isInteger(len) || len < 0) return AB_FAILURE;
    const ptr = (bump + 7) & ~7;
    if (ptr + len > memory.length) return AB_FAILURE;
    bump = ptr + len;
    return ptr;
  }

  function ab_free() {
    /* 单调分配器，无需回收 */
  }

  function ab_log(level, message) {
    try {
      const { ptr, len } = writeString(memory, ab_alloc, String(message));
      imports?.ab_log?.(level, ptr, len);
    } catch {
      /* 日志不得影响转换结果 */
    }
  }

  function ab_run(requestPtr, requestLen) {
    try {
      if (!Number.isInteger(requestLen) || requestLen <= 0 || requestLen > AB_MAX_REQUEST_BYTES) {
        ab_log(LOG_ERROR, "ab_run: 请求长度非法或超过 ABI 上限");
        return AB_FAILURE;
      }
      const request = JSON.parse(readString(memory, requestPtr, requestLen));
      if (!isKnownOp(request.op)) {
        ab_log(LOG_ERROR, `ab_run: 未知 op（当前仅支持 ${AB_OP_CONVERT}）`);
        return AB_FAILURE;
      }
      if (!["c", "b", "a"].includes(request.level)) {
        ab_log(LOG_ERROR, "ab_run: level 非法");
        return AB_FAILURE;
      }

      const ctx = {
        level: request.level,
        app_id: request.manifest_id,
        manifest_id: request.manifest_id,
        converter_version: request.converter_version,
        now: request.now,
        log: (level, message) => ab_log(level, message),
      };

      const capabilities = {
        readFile: (declaredPath) => callHostString(imports, "ab_read_file", memory, ab_alloc, declaredPath),
        log: (level, message) => ab_log(level, message),
      };

      const raw = extract(capabilities, ctx);
      const document = normalize(raw, ctx);

      // 自检：产物等级必须与清单/请求一致，不一致宁可整体失败也不上抛半成品。
      if (document.level !== request.level) {
        ab_log(LOG_ERROR, "ab_run: 产物 level 与请求不一致，拒绝输出");
        return AB_FAILURE;
      }

      const bytes = utf8Encode(JSON.stringify(document));
      if (bytes.length > AB_MAX_RESPONSE_BYTES) {
        ab_log(LOG_ERROR, "ab_run: 响应超过 ABI 上限");
        return AB_FAILURE;
      }
      const outPtr = ab_alloc(bytes.length);
      if (outPtr === AB_FAILURE) {
        ab_log(LOG_ERROR, "ab_run: 线性内存不足，无法写出响应");
        return AB_FAILURE;
      }
      memory.set(bytes, outPtr);
      return packPtrLen(outPtr, bytes.length);
    } catch (error) {
      ab_log(LOG_ERROR, `ab_run 失败：${error?.message ?? error}`);
      return AB_FAILURE;
    }
  }

  return { memory, ab_alloc, ab_free, ab_run };
}
