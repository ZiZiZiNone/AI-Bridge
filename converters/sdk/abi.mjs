/**
 * AI-Bridge 转换器 SDK —— WASM 宿主 ABI（唯一事实源）。
 *
 * 契约边界：guest 侧只导出 `ab_run`，输入与输出均为 UTF-8 JSON；
 * `ab_run` 的唯一合法产物是 StandardAccess JSON（contracts/standard-access.schema.json）。
 *
 * 本文件只放 ABI 常量与指针/长度编解码，**不实现任何结构校验**：
 * 契约字段校验的唯一实现在 ab-contract（宿主侧）；转换器不得自行发明第二套校验规则。
 */

/** 唯一的正式 op（ABI 当前只定义这一个；新增 op 属 ABI 次版本变更）。 */
export const AB_OP_CONVERT = "convert";

/**
 * 宿主可提供给 guest 的导入函数白名单。
 * 真实宿主必须逐项枚举 wasm import 段：出现白名单之外的导入（含 wasi_snapshot_preview1）即拒绝加载。
 * 注意白名单里**没有**网络、写文件、进程、时间相关的导入：沙盒放权面为零。
 */
export const HOST_IMPORTS = Object.freeze(["ab_read_file", "ab_log"]);

/** ab_log 的级别取值（真实宿主持久化时按级别过滤）。 */
export const LOG_ERROR = 0;
export const LOG_WARN = 1;
export const LOG_INFO = 2;
export const LOG_DEBUG = 3;

/** ab_run 失败码：i64 = -1（0xFFFF_FFFF_FFFF_FFFF）。失败时不返回任何 JSON。 */
export const AB_FAILURE = -1n;

/** ABI 尺寸约束：宿主与 guest 双向都检查，越界即失败。 */
export const AB_MAX_REQUEST_BYTES = 64 * 1024;
export const AB_MAX_RESPONSE_BYTES = 512 * 1024;
export const AB_MIN_MEMORY_BYTES = 1 << 20;

/**
 * StandardAccess 的契约版本。
 * 权威定义在 contracts/standard-access.schema.json 的 `const`；此处是转换器侧唯一副本，
 * 漂移由 converters/sdk/tests/conformance.mjs 的版本一致性检查拦下。
 */
export const STANDARD_ACCESS_SCHEMA_VERSION = "1.0";

/** 已知 op 判定（ABI 路由，不是业务校验）。 */
export function isKnownOp(op) {
  return op === AB_OP_CONVERT;
}

/** 失败码判定；接受 BigInt 与 number 两种表示。 */
export function isFailure(value) {
  return value === AB_FAILURE || value === -1;
}

/** i64 打包：高 32 位 = 线性内存指针，低 32 位 = 字节长度。 */
export function packPtrLen(ptr, len) {
  if (!Number.isInteger(ptr) || !Number.isInteger(len) || ptr < 0 || len < 0) return AB_FAILURE;
  return (BigInt(ptr) << 32n) | BigInt(len);
}

/** i64 解包；失败码或非法打包返回 null。 */
export function unpackPtrLen(value) {
  const packed = typeof value === "bigint" ? value : Number.isInteger(value) ? BigInt(value) : null;
  if (packed === null || packed < 0n) return null;
  return { ptr: Number(packed >> 32n), len: Number(packed & 0xffff_ffffn) };
}

const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8", { fatal: true });

export function utf8Encode(text) {
  return encoder.encode(text);
}

export function utf8Decode(bytes) {
  return decoder.decode(bytes);
}

/** 把字符串写入线性内存（经 guest 自己的分配器），返回 { ptr, len }。 */
export function writeString(memory, alloc, text) {
  const bytes = utf8Encode(text);
  const ptr = alloc(bytes.length);
  if (typeof ptr !== "number" || ptr < 0) throw new Error("线性内存分配失败");
  memory.set(bytes, ptr);
  return { ptr, len: bytes.length };
}

/** 从线性内存读字符串；越界或非法 UTF-8 直接抛错（静默截断比报错更危险）。 */
export function readString(memory, ptr, len) {
  if (!Number.isInteger(ptr) || !Number.isInteger(len) || ptr < 0 || len < 0 || ptr + len > memory.length) {
    throw new Error("线性内存越界读取");
  }
  return utf8Decode(memory.subarray(ptr, ptr + len));
}

/**
 * 调用宿主导入并把返回值当字符串读回。
 * 失败或越界一律抛错——由 guest 的 ab_run 统一转成 AB_FAILURE。
 */
export function callHostString(imports, name, memory, alloc, text) {
  const fn = imports?.[name];
  if (typeof fn !== "function") throw new Error(`宿主未提供导入 ${name}`);
  const { ptr, len } = writeString(memory, alloc, text);
  const decoded = unpackPtrLen(fn(ptr, len));
  if (!decoded) throw new Error(`导入 ${name} 调用失败`);
  return readString(memory, decoded.ptr, decoded.len);
}
