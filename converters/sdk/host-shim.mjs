/**
 * AI-Bridge 转换器 SDK —— 本地宿主垫片（转换器作者自测 / 夹具校验用）。
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

export class SandboxError extends Error {}

export function parseJsonText(text, label) {
  const clean = String(text);
  if (clean.charCodeAt(0) === 0xfeff) {
    throw new Error(`${label} 带 UTF-8 BOM；SDK 约定 JSON 必须 UTF-8 无 BOM`);
  }
  return JSON.parse(clean);
}

export function readJsonFile(file) {
  return parseJsonText(readFileSync(file, "utf8"), file);
}

export const FIXTURE_DIR_SEGMENTS = ["tests", "fixtures", "raw"];

export function isAbsoluteAnyPlatform(p) {
  const text = String(p);
  return path.posix.isAbsolute(text) || path.win32.isAbsolute(text);
}

export function basenameAnyPlatform(p) {
  const text = String(p);
  return text.includes("\\") ? path.win32.basename(text) : path.basename(text);
}

function expandTemplate(template, env) {
  const expanded = String(template).replace(/\{([A-Za-z_][A-Za-z0-9_]*)\}/g, (_all, name) => {
    const value = env[name];
    if (!value) throw new SandboxError(`可读路径模板引用了未定义的环境变量 {${name}}`);
    return value;
  });
  if (!isAbsoluteAnyPlatform(expanded)) {
    throw new SandboxError(`可读路径展开后不是绝对路径：${template}`);
  }
  if (expanded.includes("..") || expanded.includes("*")) {
    throw new SandboxError(`可读路径不得含 .. 或 *：${template}`);
  }
  return expanded;
}

export async function createLocalHost(options) {
  const converterDir = path.resolve(options.converterDir);
  const env = options.env ?? {
    APPDATA: "C:\\Users\\Default\\AppData\\Roaming",
    LOCALAPPDATA: "C:\\Users\\Default\\AppData\\Local",
    USERPROFILE: "C:\\Users\\Default",
    ...process.env,
  };
  const fixtureDir = options.fixtureDir ?? path.join(converterDir, ...FIXTURE_DIR_SEGMENTS);
  const now = options.now ?? (() => new Date().toISOString());

  const manifest = readJsonFile(path.join(converterDir, "manifest.json"));

  const declared = new Map();
  for (const template of manifest.readable_paths ?? []) {
    const expanded = expandTemplate(template, env);
    declared.set(basenameAnyPlatform(expanded).toLowerCase(), { template, expanded });
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
    ab_read_file(ptr, len) {
      let name;
      try {
        name = readString(instance.memory, ptr, len);
      } catch {
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
        // 日志错误不影响转换结果
      }
    },
  };

  function readDeclared(name) {
    const base = basenameAnyPlatform(String(name)).toLowerCase();
    const hit = declared.get(base);
    if (!hit) throw new SandboxError(`路径未在 manifest.readable_paths 声明，拒绝读取：${name}`);
    return new Uint8Array(readFileSync(path.join(fixtureDir, base)));
  }

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
    readDeclared,
    buildRequest,
    runRaw,
    collect(overrides = {}) {
      return runRaw(buildRequest(overrides));
    },
  };
}
