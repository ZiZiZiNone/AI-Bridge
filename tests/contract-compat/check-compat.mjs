#!/usr/bin/env node
/**
 * AI-Bridge 契约漂移锁 —— tests/contract-compat/check-compat.mjs
 *
 * 职责：锁定 contracts/ 三份 schema 的「required 字段集合」与「schema_version 常量」，
 *       任何悄悄改动（增删/改名必填字段、改版本常量、删除版本声明）都判定 FAIL。
 * 边界：只读 contracts/，不修改任何文件；不承载业务规则。
 * 依赖：仅 Node 内置模块，零第三方依赖（Node >= 22）。
 *
 * 用法：
 *   node tests/contract-compat/check-compat.mjs           # 校验锁定值
 *   node tests/contract-compat/check-compat.mjs --selftest # 额外证明锁能咬住漂移
 *
 * 退出码：0 = 锁定值与基线一致（selftest 模式下另加"漂移均被检出"）；1 = 不一致。
 */

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
const CONTRACTS_DIR = join(SCRIPT_DIR, '..', '..', 'contracts');

/** 锁定的顶层 schema_version 常量值。 */
const SCHEMA_VERSION = '1.0';

/**
 * 基线 1：required 字段集合（已排序），按 JSON Pointer 定位。
 * 改动本表 = 一次显式契约变更，必须与 contracts/ 同步评审后一起提交。
 */
const EXPECTED_REQUIRED = {
  'converter-manifest.schema.json': {
    '#': ['entry', 'id', 'level', 'name', 'readable_paths', 'signature', 'version'],
    '#/properties/signature': ['algorithm', 'signer', 'value'],
  },
  'session-envelope.schema.json': {
    '#': ['account_ref', 'issued_at', 'kind', 'payload', 'schema_version', 'source', 'target'],
    '#/properties/target': ['kind'],
    '#/properties/payload/properties/cookies/items': ['name', 'value'],
    '#/properties/payload/properties/credential': ['secret', 'username'],
    '#/properties/source': ['id', 'type'],
  },
  'standard-access.schema.json': {
    '#': ['app_id', 'generated_at', 'level', 'schema_version'],
    '#/$defs/credential': ['account_ref', 'kind'],
    '#/$defs/cookie': ['name', 'value'],
    '#/$defs/session': ['account_ref', 'kind', 'payload', 'source', 'target'],
    '#/$defs/target': ['kind'],
    '#/$defs/payload/properties/credential': ['secret', 'username'],
    '#/$defs/source': ['id', 'type'],
  },
};

/**
 * 基线 2：schema_version 声明位置 → 类型与常量。
 * converter-manifest 用 version（转换器自身版本）而非 schema_version，锁定为「无声明」。
 */
const EXPECTED_SCHEMA_VERSION_DECLS = {
  'converter-manifest.schema.json': {},
  'session-envelope.schema.json': { '#': { type: 'string', const: SCHEMA_VERSION } },
  'standard-access.schema.json': { '#': { type: 'string', const: SCHEMA_VERSION } },
};

const SCHEMA_FILES = Object.keys(EXPECTED_REQUIRED).sort();

/* ---------------- 工具 ---------------- */

function isPlainObject(value) {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function hasOwn(value, key) {
  return Object.prototype.hasOwnProperty.call(value, key);
}

function jsonPointerKey(pointer) {
  return pointer === '' ? '#' : `#${pointer}`;
}

function escapeSegment(segment) {
  return segment.replace(/~/g, '~0').replace(/\//g, '~1');
}

/** 收集所有 required 声明：pointer → 已排序字段集合。 */
function collectRequired(schema) {
  const found = new Map();
  const walk = (node, pointer) => {
    if (Array.isArray(node)) {
      node.forEach((item, index) => walk(item, `${pointer}/${index}`));
      return;
    }
    if (!isPlainObject(node)) return;
    if (Array.isArray(node.required)) {
      found.set(jsonPointerKey(pointer), [...node.required].sort());
    }
    for (const key of Object.keys(node)) {
      walk(node[key], `${pointer}/${escapeSegment(key)}`);
    }
  };
  walk(schema, '');
  return found;
}

/** 收集所有 schema_version 声明：容器 pointer → { type, const }。 */
function collectSchemaVersionDecls(schema) {
  const found = new Map();
  const walk = (node, pointer) => {
    if (Array.isArray(node)) {
      node.forEach((item, index) => walk(item, `${pointer}/${index}`));
      return;
    }
    if (!isPlainObject(node)) return;
    if (isPlainObject(node.properties) && hasOwn(node.properties, 'schema_version')) {
      found.set(jsonPointerKey(pointer), node.properties.schema_version);
    }
    for (const key of Object.keys(node)) {
      walk(node[key], `${pointer}/${escapeSegment(key)}`);
    }
  };
  walk(schema, '');
  return found;
}

function loadSchemas() {
  const schemas = new Map();
  const problems = [];
  for (const name of SCHEMA_FILES) {
    try {
      schemas.set(name, JSON.parse(readFileSync(join(CONTRACTS_DIR, name), 'utf8')));
    } catch (err) {
      problems.push(`contracts/${name} 读取失败：${err.message}`);
    }
  }
  return { schemas, problems };
}

/** 锁定校验：返回失败原因列表，空数组即锁定值与基线一致。 */
function checkLock(schemas) {
  const failures = [];

  for (const name of SCHEMA_FILES) {
    const schema = schemas.get(name);
    if (schema === undefined) {
      failures.push(`缺失契约文件 ${name}`);
      continue;
    }

    const actualRequired = collectRequired(schema);
    const expectedRequired = EXPECTED_REQUIRED[name] ?? {};
    const pointers = [...new Set([...actualRequired.keys(), ...Object.keys(expectedRequired)])].sort();
    for (const pointer of pointers) {
      const actual = actualRequired.get(pointer);
      const expected = expectedRequired[pointer];
      if (expected === undefined) {
        failures.push(`${name} ${pointer}：新增未锁定的 required [${(actual ?? []).join(', ')}]`);
      } else if (actual === undefined) {
        failures.push(`${name} ${pointer}：required 声明被删除（锁定值 [${expected.join(', ')}]）`);
      } else if (actual.join('|') !== expected.join('|')) {
        failures.push(`${name} ${pointer}：required 集合变化 锁定=[${expected.join(', ')}] 实际=[${actual.join(', ')}]`);
      }
    }

    const actualDecls = collectSchemaVersionDecls(schema);
    const expectedDecls = EXPECTED_SCHEMA_VERSION_DECLS[name] ?? {};
    const declPointers = [...new Set([...actualDecls.keys(), ...Object.keys(expectedDecls)])].sort();
    for (const pointer of declPointers) {
      const actual = actualDecls.get(pointer);
      const expected = expectedDecls[pointer];
      if (expected === undefined) {
        failures.push(`${name} ${pointer}：新增未锁定的 schema_version 声明 ${JSON.stringify(actual)}`);
        continue;
      }
      if (actual === undefined) {
        failures.push(`${name} ${pointer}：schema_version 声明被删除（锁定值 ${JSON.stringify(expected)}）`);
        continue;
      }
      if (actual.type !== expected.type) {
        failures.push(`${name} ${pointer}：schema_version type 变化 锁定=${expected.type} 实际=${String(actual.type)}`);
      }
      if (actual.const !== expected.const) {
        failures.push(
          `${name} ${pointer}：schema_version 常量变化 锁定=${JSON.stringify(expected.const)} 实际=${JSON.stringify(actual.const)}`,
        );
      }
    }
  }

  return failures;
}

function cloneSchemas(schemas) {
  const copy = new Map();
  for (const [name, schema] of schemas) copy.set(name, structuredClone(schema));
  return copy;
}

/* ---------------- 漂移自测 ---------------- */

/** 每个变异都必须被 checkLock 咬住，否则锁无效。 */
const MUTATIONS = [
  {
    name: '删除顶层 required 字段（standard-access.generated_at）',
    apply: (schemas) => {
      const schema = schemas.get('standard-access.schema.json');
      schema.required = schema.required.filter((field) => field !== 'generated_at');
    },
  },
  {
    name: '重命名顶层 required 字段（standard-access.app_id → application_id）',
    apply: (schemas) => {
      const schema = schemas.get('standard-access.schema.json');
      schema.required = schema.required.map((field) => (field === 'app_id' ? 'application_id' : field));
    },
  },
  {
    name: '新增嵌套 required（standard-access.$defs.cookie 增加 domain）',
    apply: (schemas) => {
      schemas.get('standard-access.schema.json').$defs.cookie.required.push('domain');
    },
  },
  {
    name: '删除嵌套 required 声明（session-envelope.properties.target.required）',
    apply: (schemas) => {
      delete schemas.get('session-envelope.schema.json').properties.target.required;
    },
  },
  {
    name: '改动 schema_version 常量（1.0 → 1.1）',
    apply: (schemas) => {
      schemas.get('session-envelope.schema.json').properties.schema_version.const = '1.1';
    },
  },
  {
    name: '删除 schema_version 声明（standard-access.properties.schema_version）',
    apply: (schemas) => {
      delete schemas.get('standard-access.schema.json').properties.schema_version;
    },
  },
];

function runSelftest(schemas) {
  console.log('');
  console.log(`漂移自测（对内存副本注入变异，期望全部被检出）：`);
  const baselineFailures = checkLock(schemas);
  if (baselineFailures.length > 0) {
    console.log('  [FAIL] 基线本身不通过，自测无意义：');
    for (const failure of baselineFailures) console.log(`         - ${failure}`);
    return false;
  }
  console.log('  [PASS] 基线：0 漂移');

  let allDetected = true;
  for (const mutation of MUTATIONS) {
    const mutated = cloneSchemas(schemas);
    mutation.apply(mutated);
    const failures = checkLock(mutated);
    if (failures.length > 0) {
      console.log(`  [PASS] ${mutation.name} → 检出 ${failures.length} 项：${failures[0]}`);
    } else {
      allDetected = false;
      console.log(`  [FAIL] ${mutation.name} → 未被检出（锁存在盲区）`);
    }
  }
  return allDetected;
}

/* ---------------- 入口 ---------------- */

function main() {
  const selftestMode = process.argv.includes('--selftest');
  console.log('AI-Bridge 契约漂移锁（tests/contract-compat/check-compat.mjs）');
  console.log(`  契约目录：${CONTRACTS_DIR}`);
  console.log(`  锁定项：required 字段集合（顶层 + 嵌套）、schema_version 常量 "${SCHEMA_VERSION}"`);
  console.log('');

  const { schemas, problems } = loadSchemas();
  for (const name of SCHEMA_FILES) {
    const required = schemas.get(name) === undefined ? null : collectRequired(schemas.get(name));
    const decls = schemas.get(name) === undefined ? null : collectSchemaVersionDecls(schemas.get(name));
    const requiredCount = required === null ? 0 : required.size;
    const versionText = decls === null || decls.size === 0 ? '（无 schema_version 声明）' : `schema_version=${JSON.stringify([...decls.values()][0].const)}`;
    console.log(`  ${name}：required 声明 ${requiredCount} 处｜${versionText}`);
  }

  const failures = [...problems, ...checkLock(schemas)];
  console.log('');
  if (failures.length === 0) {
    console.log('[PASS] 三份 schema 的 required 集合与 schema_version 常量均未漂移');
  } else {
    console.log(`[FAIL] 检出 ${failures.length} 项漂移：`);
    for (const failure of failures) console.log(`  - ${failure}`);
  }

  let selftestPass = true;
  if (selftestMode) {
    selftestPass = runSelftest(schemas);
  }

  const finalPass = failures.length === 0 && selftestPass;
  console.log('');
  console.log(`结果：${finalPass ? 'PASS' : 'FAIL'}`);
  process.exitCode = finalPass ? 0 : 1;
}

main();
