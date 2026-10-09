#!/usr/bin/env node
/**
 * AI-Bridge 契约校验工具 —— tools/contract-check/check.mjs
 *
 * 职责：读 contracts/*.schema.json 与 tools/contract-check/fixtures/*.json，
 *       对样例做结构与一致性校验，输出 PASS/FAIL 与退出码（0 通过 / 1 失败）。
 *
 * 覆盖的校验规则（schema 层）：required / type / enum / pattern / const /
 *   format(date-time) / additionalProperties=false / minItems / items / $ref。
 * 覆盖的跨字段规则（契约声明的结构约束，与 core/contract/src/lib.rs 的
 *   validate 同义）：kind 与 payload 必须一致
 *   cookie → payload.cookies 非空数组；token → payload.token 非空字符串；
 *   credential → payload.credential 存在。
 *   运行时权威仍是 ab-contract，本工具只做仓库级离线校验。
 *
 * 依赖：仅 Node 内置模块，零第三方依赖（Node >= 22）。
 * 用法：node tools/contract-check/check.mjs
 */

import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = join(SCRIPT_DIR, '..', '..');
const CONTRACTS_DIR = join(REPO_ROOT, 'contracts');
const FIXTURES_DIR = join(SCRIPT_DIR, 'fixtures');

const SCHEMA_SUFFIX = '.schema.json';

/** 契约只有三份 schema；出现第四份或改名即失败。 */
const SCHEMA_FILES = [
  'converter-manifest.schema.json',
  'session-envelope.schema.json',
  'standard-access.schema.json',
];

/** fixtures 文件名里的规则代号 → 期望命中的错误码。 */
const RULE_TO_CODE = {
  'missing-required': 'REQUIRED_MISSING',
  'unknown-field': 'UNKNOWN_FIELD',
  type: 'TYPE_MISMATCH',
  enum: 'ENUM_INVALID',
  pattern: 'PATTERN_MISMATCH',
  const: 'CONST_MISMATCH',
  format: 'FORMAT_INVALID',
  'min-items': 'MIN_ITEMS',
  'kind-payload-mismatch': 'PAYLOAD_KIND_MISMATCH',
};

const SESSION_KINDS = ['cookie', 'token', 'credential'];

const DATE_TIME_RE = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:\d{2})$/;

/* ---------------- 基础工具 ---------------- */

function readJson(file) {
  return JSON.parse(readFileSync(file, 'utf8'));
}

function isPlainObject(value) {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function hasOwn(value, key) {
  return Object.prototype.hasOwnProperty.call(value, key);
}

function typeOf(value) {
  if (value === null) return 'null';
  if (Array.isArray(value)) return 'array';
  if (typeof value === 'number') return Number.isInteger(value) ? 'integer' : 'number';
  return typeof value;
}

function matchesType(value, expected) {
  const actual = typeOf(value);
  if (expected === 'integer') return actual === 'integer';
  if (expected === 'number') return actual === 'integer' || actual === 'number';
  return actual === expected;
}

function deepEqual(a, b) {
  if (a === b) return true;
  if (typeOf(a) !== typeOf(b)) return false;
  if (Array.isArray(a)) {
    return a.length === b.length && a.every((item, index) => deepEqual(item, b[index]));
  }
  if (isPlainObject(a)) {
    const keysA = Object.keys(a);
    const keysB = Object.keys(b);
    return keysA.length === keysB.length && keysA.every((key) => hasOwn(b, key) && deepEqual(a[key], b[key]));
  }
  return false;
}

function show(value) {
  const text = JSON.stringify(value);
  return text === undefined ? String(value) : text;
}

function isDateTime(value) {
  return DATE_TIME_RE.test(value) && Number.isFinite(Date.parse(value));
}

/** 解析 JSON Pointer（仅支持 schema 内部的 "#/..." 引用）。 */
function resolveRef(root, ref) {
  if (typeof ref !== 'string' || !ref.startsWith('#/')) return null;
  let node = root;
  for (const rawSegment of ref.slice(2).split('/')) {
    const key = rawSegment.replace(/~1/g, '/').replace(/~0/g, '~');
    if (!isPlainObject(node) && !Array.isArray(node)) return null;
    if (!hasOwn(node, key)) return null;
    node = node[key];
  }
  return node === undefined ? null : node;
}

/* ---------------- 校验内核 ---------------- */

function pushError(errors, code, path, message) {
  errors.push({ code, path, message });
}

function validateValue(root, schema, value, path, errors) {
  if (!isPlainObject(schema)) return;

  if (typeof schema.$ref === 'string') {
    const resolved = resolveRef(root, schema.$ref);
    if (resolved === null) {
      pushError(errors, 'REF_UNRESOLVED', path, `$ref 无法解析：${schema.$ref}`);
      return;
    }
    validateValue(root, resolved, value, path, errors);
    return;
  }

  if (typeof schema.type === 'string' && !matchesType(value, schema.type)) {
    pushError(errors, 'TYPE_MISMATCH', path, `期望类型 ${schema.type}，实际 ${typeOf(value)}`);
    return;
  }

  if (hasOwn(schema, 'const') && !deepEqual(value, schema.const)) {
    pushError(errors, 'CONST_MISMATCH', path, `期望常量 ${show(schema.const)}，实际 ${show(value)}`);
  }

  if (Array.isArray(schema.enum) && !schema.enum.some((allowed) => deepEqual(value, allowed))) {
    pushError(errors, 'ENUM_INVALID', path, `取值 ${show(value)} 不在枚举 [${schema.enum.join(', ')}] 内`);
  }

  if (typeof schema.pattern === 'string' && typeof value === 'string') {
    if (!new RegExp(schema.pattern).test(value)) {
      pushError(errors, 'PATTERN_MISMATCH', path, `取值 ${show(value)} 不匹配 ${schema.pattern}`);
    }
  }

  if (schema.format === 'date-time' && typeof value === 'string' && !isDateTime(value)) {
    pushError(errors, 'FORMAT_INVALID', path, `取值 ${show(value)} 不是 RFC3339 date-time`);
  }

  if (isPlainObject(value)) {
    validateObject(root, schema, value, path, errors);
  } else if (Array.isArray(value)) {
    validateArray(root, schema, value, path, errors);
  }
}

function validateObject(root, schema, value, path, errors) {
  const required = Array.isArray(schema.required) ? schema.required : [];
  for (const key of required) {
    if (!hasOwn(value, key)) {
      pushError(errors, 'REQUIRED_MISSING', `${path}.${key}`, `缺少必填字段 ${key}`);
    }
  }

  const properties = isPlainObject(schema.properties) ? schema.properties : {};
  for (const key of Object.keys(value)) {
    if (!hasOwn(properties, key)) {
      if (schema.additionalProperties === false) {
        pushError(errors, 'UNKNOWN_FIELD', `${path}.${key}`, 'schema 未声明该字段且 additionalProperties=false');
      } else if (isPlainObject(schema.additionalProperties)) {
        validateValue(root, schema.additionalProperties, value[key], `${path}.${key}`, errors);
      }
      continue;
    }
    validateValue(root, properties[key], value[key], `${path}.${key}`, errors);
  }

  checkPayloadKindConsistency(schema, value, path, errors);
}

function validateArray(root, schema, value, path, errors) {
  if (typeof schema.minItems === 'number' && value.length < schema.minItems) {
    pushError(errors, 'MIN_ITEMS', path, `数组长度 ${value.length} 小于 minItems ${schema.minItems}`);
  }
  if (isPlainObject(schema.items)) {
    value.forEach((item, index) => validateValue(root, schema.items, item, `${path}[${index}]`, errors));
  }
}

/**
 * 跨字段规则：kind ↔ payload 一致性。
 * 仅对「kind 枚举恰为 cookie|token|credential 且声明了 payload」的 schema 生效
 * （standard-access 的 $defs.session 与 session-envelope 顶层；$defs.credential 的
 * kind 枚举是 password|passkey|totp，不触发）。
 */
function checkPayloadKindConsistency(schema, value, path, errors) {
  const properties = isPlainObject(schema.properties) ? schema.properties : {};
  const kindEnum = isPlainObject(properties.kind) && Array.isArray(properties.kind.enum) ? properties.kind.enum : null;
  if (kindEnum === null || !isPlainObject(properties.payload)) return;
  const isSessionShape = kindEnum.length === SESSION_KINDS.length && SESSION_KINDS.every((k) => kindEnum.includes(k));
  if (!isSessionShape) return;
  if (!SESSION_KINDS.includes(value.kind) || !isPlainObject(value.payload)) return;

  const { kind, payload } = value;
  if (kind === 'cookie' && !(Array.isArray(payload.cookies) && payload.cookies.length > 0)) {
    pushError(errors, 'PAYLOAD_KIND_MISMATCH', `${path}.payload`, 'kind="cookie" 要求 payload.cookies 为非空数组');
  }
  if (kind === 'token' && !(typeof payload.token === 'string' && payload.token.length > 0)) {
    pushError(errors, 'PAYLOAD_KIND_MISMATCH', `${path}.payload`, 'kind="token" 要求 payload.token 为非空字符串');
  }
  if (kind === 'credential' && !isPlainObject(payload.credential)) {
    pushError(errors, 'PAYLOAD_KIND_MISMATCH', `${path}.payload`, 'kind="credential" 要求 payload.credential 存在');
  }
}

/* ---------------- schema / fixtures 装载 ---------------- */

function checkSchemaShape(name, schema) {
  const problems = [];
  if (!isPlainObject(schema)) {
    problems.push(`contracts/${name} 顶层不是 JSON 对象`);
    return problems;
  }
  if (typeof schema.$schema !== 'string') problems.push(`contracts/${name} 缺少 $schema`);
  if (schema.type !== 'object') problems.push(`contracts/${name} 顶层 type 必须为 object`);
  if (!Array.isArray(schema.required) || schema.required.length === 0) {
    problems.push(`contracts/${name} 顶层 required 必须为非空数组`);
  }
  if (!isPlainObject(schema.properties)) problems.push(`contracts/${name} 顶层缺少 properties`);
  if (schema.additionalProperties !== false) {
    problems.push(`contracts/${name} 顶层 additionalProperties 必须为 false`);
  }
  return problems;
}

function loadSchemas() {
  const schemas = new Map();
  const problems = [];
  let entries = [];
  try {
    entries = readdirSync(CONTRACTS_DIR).filter((name) => name.endsWith(SCHEMA_SUFFIX)).sort();
  } catch (err) {
    problems.push(`无法读取契约目录 ${CONTRACTS_DIR}：${err.message}`);
    return { schemas, problems };
  }
  for (const name of SCHEMA_FILES) {
    if (!entries.includes(name)) problems.push(`缺少契约文件 contracts/${name}`);
  }
  for (const name of entries) {
    if (!SCHEMA_FILES.includes(name)) problems.push(`contracts/ 出现未登记 schema：${name}（契约固定三份）`);
  }
  for (const name of entries) {
    let schema;
    try {
      schema = readJson(join(CONTRACTS_DIR, name));
    } catch (err) {
      problems.push(`contracts/${name} 解析失败：${err.message}`);
      continue;
    }
    problems.push(...checkSchemaShape(name, schema));
    schemas.set(name, schema);
  }
  return { schemas, problems };
}

function discoverCases(schemas) {
  const cases = [];
  const problems = [];
  let names = [];
  try {
    names = readdirSync(FIXTURES_DIR).filter((name) => name.endsWith('.json')).sort();
  } catch (err) {
    problems.push(`无法读取夹具目录 ${FIXTURES_DIR}：${err.message}`);
    return { cases, problems };
  }

  for (const file of names) {
    const matched = /^(.+?)\.(valid|invalid)(?:\.([a-z0-9-]+))?\.json$/.exec(file);
    if (matched === null) {
      problems.push(`fixtures/${file} 命名不符合 <schema>.<valid|invalid>[.<规则代号>].json`);
      continue;
    }
    const [, prefix, expectation, rule] = matched;
    const schemaName = `${prefix}${SCHEMA_SUFFIX}`;
    if (!schemas.has(schemaName)) {
      problems.push(`fixtures/${file} 找不到对应契约 contracts/${schemaName}`);
      continue;
    }
    if (expectation === 'valid' && rule !== undefined) {
      problems.push(`fixtures/${file} 合法样例不应带规则代号`);
      continue;
    }
    if (expectation === 'invalid' && rule === undefined) {
      problems.push(`fixtures/${file} 非法样例必须带规则代号`);
      continue;
    }
    if (expectation === 'invalid' && !hasOwn(RULE_TO_CODE, rule)) {
      problems.push(`fixtures/${file} 规则代号 "${rule}" 未登记（见 RULE_TO_CODE）`);
      continue;
    }
    let data;
    try {
      data = readJson(join(FIXTURES_DIR, file));
    } catch (err) {
      problems.push(`fixtures/${file} 解析失败：${err.message}`);
      continue;
    }
    cases.push({ file, schemaName, expect: expectation, rule, data });
  }

  for (const schemaName of schemas.keys()) {
    const group = cases.filter((item) => item.schemaName === schemaName);
    if (group.filter((item) => item.expect === 'valid').length < 1) {
      problems.push(`contracts/${schemaName} 缺少合法样例（至少一份）`);
    }
    if (group.filter((item) => item.expect === 'invalid').length < 2) {
      problems.push(`contracts/${schemaName} 缺少非法样例（至少两份）`);
    }
  }
  if (!cases.some((item) => item.rule === 'kind-payload-mismatch')) {
    problems.push('fixtures/ 缺少 kind 与 payload 不一致的非法样例');
  }
  return { cases, problems };
}

/* ---------------- 执行 ---------------- */

function runCase(schema, testCase) {
  const errors = [];
  validateValue(schema, schema, testCase.data, '$', errors);
  errors.sort((a, b) => {
    if (a.path !== b.path) return a.path < b.path ? -1 : 1;
    if (a.code === b.code) return 0;
    return a.code < b.code ? -1 : 1;
  });
  const expectedCode = testCase.expect === 'valid' ? null : RULE_TO_CODE[testCase.rule];
  const passed = testCase.expect === 'valid' ? errors.length === 0 : errors.some((err) => err.code === expectedCode);
  return { passed, errors, expectedCode };
}

function main() {
  console.log('AI-Bridge 契约校验（tools/contract-check/check.mjs）');
  console.log(`  契约目录：${CONTRACTS_DIR}`);
  console.log(`  夹具目录：${FIXTURES_DIR}`);
  console.log('');

  const { schemas, problems } = loadSchemas();
  const { cases, problems: fixtureProblems } = discoverCases(schemas);
  const allProblems = [...problems, ...fixtureProblems];

  let passed = 0;
  for (const testCase of cases) {
    const { passed: ok, errors, expectedCode } = runCase(schemas.get(testCase.schemaName), testCase);
    if (ok) passed += 1;
    const expectation = testCase.expect === 'valid' ? '合法样例' : `非法样例(${testCase.rule})`;
    if (ok) {
      console.log(`[PASS] ${testCase.file}  ${expectation} → ${expectedCode === null ? '0 错误' : `命中 ${expectedCode}`}`);
    } else {
      console.log(`[FAIL] ${testCase.file}  ${expectation}`);
      if (testCase.expect === 'valid') {
        for (const err of errors) console.log(`         - ${err.code} @ ${err.path}：${err.message}`);
      } else {
        const actual = errors.length === 0 ? '0 错误（校验通过）' : errors.map((err) => `${err.code}@${err.path}`).join('、');
        console.log(`         - 期望命中 ${expectedCode}，实际：${actual}`);
      }
    }
    if (ok && expectedCode !== null) {
      const extra = [...new Set(errors.filter((err) => err.code !== expectedCode).map((err) => err.code))];
      if (extra.length > 0) console.log(`         · 附带错误：${extra.join('、')}`);
    }
  }

  if (allProblems.length > 0) {
    console.log('');
    console.log('契约 / 夹具自检失败：');
    for (const problem of allProblems) console.log(`  - ${problem}`);
  }

  console.log('');
  console.log(`契约文件：${schemas.size}/${SCHEMA_FILES.length}｜用例：PASS ${passed}/${cases.length}`);
  const finalPass = allProblems.length === 0 && passed === cases.length && cases.length > 0;
  console.log(`结果：${finalPass ? 'PASS' : 'FAIL'}`);
  process.exitCode = finalPass ? 0 : 1;
}

main();
