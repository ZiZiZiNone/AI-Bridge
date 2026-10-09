/**
 * AI-Bridge 转换器 SDK —— 本地宿主垫片（转换器作者自测 / 夹具校验用）。
 *
 * 它只做真实宿主（core/converter-host）按 ABI 必做的那几件事：
 *   1. 读 manifest.json，把 readable_paths 展开成沙盒白名单（只读、未声明即拒绝）；
 *   2. 按 ABI 写请求、调 `ab_run`、按 i64 打包值读回响应；
 *   3. 每次采集新建一个 guest 实例（无跨次状态残留）。
 *
 * 它**不做**结构校验、不做业务判断：StandardAccess 的校验权威在 ab-contract。
 * 它也不提供网络、写文件、时间等能力：guest 只有 ab_read_file / ab_log 两个导入。
 *
 * 本地约定：manifest.entry = "converter.wasm" 时，垫片加载同目录同名的 `converter.mjs`
 * 作为可运行的参考实现（真实宿主加载 .wasm，其余 ABI 完全一致）。
 */

import { readFileSync } from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import {
  AB_MAX_REQUEST_BYTES,
  AB_MAX_RESPONSE_BYTES,
  AB_OP_CONVERT,
  AB_FAILURE,
  isFailure,
  packPtrLen,
  readString,
  unpackPtrLen,
  utf8Encode,
} from "./abi.mjs";

/** 沙盒拒绝：请求了未在 manifest.readable_paths 声明的路径。 */
export class SandboxError extends Error {}

/**
 * SDK 约定：清单与夹具一律 UTF-8 无 BOM。
 * 带 BOM 时明确报错而不是静默清洗——静默清洗会让「转换器读真实文件失败」在本地被掩盖。
 */
export function parseJsonText(text, label) {
  const clean = String(text);
  if (clean.charCodeAt(0) === 0xfeff) {
    throw new Error(`${label} 带 UTF-8 BOM；SDK 约定 JSON 必须 UTF-8 无 BOM`);
  }
  return JSON.parse(clean);
}

/** 读 JSON 文件（含上面的 BOM 约定检查）。 */
export function readJsonFile(file) {
  return parseJsonText(readFileSync(file, "utf8"), file);
}

/** 夹具挂载点名称（tests/fixtures/raw/ 下的文件名 = 声明路径的 basename）。 */
export const FIXTURE_DIR_SEGMENTS = ["tests", "fixtures", "raw"];

function expandTemplate(template, env) {
  const expanded = String(template).replace(/\{([A-Za-z_][A-Za-z0-9_]*)\}/g, (_all, name) => {
    const value = env[name];
    if (!value) throw new SandboxError(`可读路径模板引用了未定义的环境变量 {${name}}`);
    return value;
  });
  if (!path.isAbsolute(expanded)) throw new SandboxError(`可读路径展开后不是绝对路径：${template}`);
  if (expanded.includes("..") || expanded.includes("*")) throw new SandboxError(`可读路径不得含 .. 或 *：${template}`);
  return expanded;
}

/**
 * @param {object} options
 * @param {string} options.converterDir 转换器目录（含 manifest.json 与参考入口）
 * @param {Record<string,string>} [options.env] 用于展开 {ENV} 模板的环境变量表
 * @param {string} [options.fixtureDir] 夹具目录，默认 <converterDir>/tests/fixtures/raw
 * @param {() => string} [options.now] 宿主时钟（注入项，转换器不得自取时间）
 */
export async function createLocalHost(options) {
  const converterDir = path.resolve(options.converterDir);
  const env = options.env ?? process.env;
  const fixtureDir = options.fixtureDir ?? path.join(converterDir, ...FIXTURE_DIR_SEGMENTS);
  const now = options.now ?? (() => new Date().toISOString());

  const manifest = readJsonFile(path.join(converterDir, "manifest.json"));

  // basename -> 声明原文；夹具与真实路径都按 basename 对齐
  const declared = new Map();
  for (const template of manifest.readable_paths ?? []) {
    const expanded = expandTemplate(template, env);
    declared.set(path.basename(expanded).toLowerCase(), { template, expanded });
  }

  const entryName = `${path.parse(manifest.entry ?? "").name}.mjs`;
  const entryPath = path.join(converterDir, entryName);
  const entryModule = await import(pathToFileURL(entryPath).href);
  if (typeof entryModule.createInstance !== "function") {
    throw new Error(`${entryName} 必须导出 createInstance(imports)`);
  }

  let instance = null;
  const logs = [];

  const hostImports = {
    /** 宿主唯一文件能力：只读、且只对声明路径开放。 */
    ab_read_file(ptr, len) {
      let name;
      try {
        name = readString(instance.memory, ptr, len);
      } catch (err) {
        return AB_FAILURE;
      }
      let bytes;
      try {
        bytes = readDeclared(name);
      } catch (err) {
        logs.push({ level: 0, message: err.message });
        return AB_FAILURE;
      }
      const outPtr = instance.ab_alloc(bytes.length);
      if (isFailure(outPtr)) return AB_FAILURE;
      instance.memory.set(bytes, outPtr);
      return packPtrLen(outPtr, bytes.length);
    },
    ab_log(level, ptr, len) {
      try {
        logs.push({ level, message: readString(instance.memory, ptr, len) });
      } catch {
        // 日志本身不允许影响转换结果
      }
    },
  };

  function readDeclared(name) {
    const base = path.basename(String(name)).toLowerCase();
    const hit = declared.get(base);
    if (!hit) throw new SandboxError(`路径未在 manifest.readable_paths 声明，拒绝读取：${name}`);
    return new Uint8Array(readFileSync(path.join(fixtureDir, base)));
  }

  /** 组装宿主侧请求（时钟与清单信息由宿主注入，转换器不得自取）。 */
  function buildRequest(overrides = {}) {
    return {
      op: AB_OP_CONVERT,
      manifest_id: manifest.id,
      converter_version: manifest.version,
      level: overrides.level ?? manifest.level,
      readable_paths: [...manifest.readable_paths],
      now: overrides.now ?? now(),
    };
  }

  /** 按 ABI 跑一次采集：每次都是全新实例。 */
  function runRaw(request) {
    logs.length = 0;
    instance = entryModule.createInstance(hostImports);
    const bytes = utf8Encode(JSON.stringify(request));
    if (bytes.length > AB_MAX_REQUEST_BYTES) {
      return { ok: false, reason: `请求超过 ABI 上限 ${AB_MAX_REQUEST_BYTES} 字节`, logs: [...logs] };
    }
    const ptr = instance.ab_alloc(bytes.length);
    if (isFailure(ptr)) return { ok: false, reason: "线性内存不足", logs: [...logs] };
    instance.memory.set(bytes, ptr);
    const packed = instance.ab_run(ptr, bytes.length);
    if (isFailure(packed)) return { ok: false, reason: "ab_run 返回失败码", logs: [...logs] };
    const decoded = unpackPtrLen(packed);
    if (!decoded || decoded.len > AB_MAX_RESPONSE_BYTES) {
      return { ok: false, reason: "响应指针/长度非法或超过 ABI 上限", logs: [...logs] };
    }
    return { ok: true, json: JSON.parse(readString(instance.memory, decoded.ptr, decoded.len)), logs: [...logs] };
  }

  return {
    converterDir,
    fixtureDir,
    manifest,
    declaredPaths: [...declared.values()],
    /** 沙盒读取（供测试直接验证白名单拒绝行为）。 */
    readDeclared,
    buildRequest,
    runRaw,
    collect(overrides = {}) {
      return runRaw(buildRequest(overrides));
    },
  };
}
