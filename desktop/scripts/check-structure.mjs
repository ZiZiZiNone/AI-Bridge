#!/usr/bin/env node
/**
 * AI-Bridge 桌面端结构一致性检查（零依赖，Node ≥ 18）。
 *
 * 检查项：
 *   1. src-tauri 模块结构符合 docs/目录结构.md
 *   2. W1–W15 各一个视图且分组数量正确（托盘 2 / 候选条 1 / 面板 8 / 向导 4）
 *   3. 显示名一律 AI-Bridge，全文不得出现旧名
 *   4. 窗口标签表：src-tauri WINDOW_SPECS ↔ src/ui/window-registry.ts WINDOW_VIEWS（13 个，双向相等）
 *   5. 托盘菜单：src-tauri MENU_ITEMS ↔ src/ui/tray/TrayMenu.vue（id + label 全等）
 *   6. 契约字段：contracts/*.schema.json ↔ src/types/contract.ts 逐字段一致（含必填性）
 *   7. 依赖方向：native/ 与 bridge/ 不依赖 tauri 与 crate::ui；core 不反向依赖 desktop
 *   8. SFC 块顺序 template → script → style，且 script 为 lang="ts"
 */

import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const DESKTOP = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const REPO = resolve(DESKTOP, '..');
const UI_SRC = join(DESKTOP, 'src', 'ui');
const RUST_SRC = join(DESKTOP, 'src-tauri', 'src');
const CORE_DIR = join(REPO, 'core');

const SKIP_DIRS = new Set(['node_modules', '.npm-cache', 'dist', 'target', '.render-check', 'gen', 'icons']);

const results = [];

function check(name, fn) {
  try {
    const detail = fn();
    results.push({ ok: true, name, detail: detail ?? '' });
  } catch (error) {
    results.push({ ok: false, name, detail: error.message });
  }
}

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

const read = (path) => readFileSync(path, 'utf8');

function walk(dir) {
  const out = [];
  for (const entry of readdirSync(dir)) {
    if (SKIP_DIRS.has(entry)) continue;
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) out.push(...walk(full));
    else out.push(full);
  }
  return out;
}

const rel = (path) => relative(DESKTOP, path).replaceAll('\\', '/');
const relRepo = (path) => relative(REPO, path).replaceAll('\\', '/');

const VIEW_GROUPS = ['tray', 'candidate', 'panels', 'wizard'];
const viewFiles = () =>
  VIEW_GROUPS.flatMap((group) =>
    walk(join(UI_SRC, group)).filter((path) => path.endsWith('.vue')),
  );

const REQUIRED_RUST = [
  'ui/mod.rs',
  'ui/window.rs',
  'ui/tray/mod.rs',
  'ui/tray/menu.rs',
  'ui/candidate/mod.rs',
  'ui/panels/mod.rs',
  'ui/wizard/mod.rs',
  'native/mod.rs',
  'native/dpapi.rs',
  'native/hello.rs',
  'native/clipboard.rs',
  'native/uia_typing.rs',
  'native/protocol.rs',
  'bridge/mod.rs',
  'bridge/messaging_host.rs',
  'bridge/ext_protocol.rs',
];

check('1. src-tauri 模块结构符合 docs/目录结构.md', () => {
  const missing = REQUIRED_RUST.filter((p) => !existsSync(join(RUST_SRC, p)));
  assert(missing.length === 0, `缺少模块：${missing.join('、')}`);
  return `${REQUIRED_RUST.length} 个模块齐备`;
});

const GROUP_EXPECTED = { tray: 2, candidate: 1, panels: 8, wizard: 4 };

check('2. W1–W15 各一个视图骨架且分组数量正确', () => {
  const found = new Map();
  const groups = new Map();

  for (const file of viewFiles()) {
    const folder = rel(file).split('/')[2];
    groups.set(folder, (groups.get(folder) ?? 0) + 1);

    const numbers = [...read(file).matchAll(/\bW(\d{1,2})\b/g)].map((m) => Number(m[1]));
    assert(numbers.length > 0, `${rel(file)} 未标注界面编号（W1–W15）`);
    for (const number of new Set(numbers)) {
      assert(!found.has(number), `W${number} 出现在多个视图：${found.get(number)} 与 ${rel(file)}`);
      found.set(number, rel(file));
    }
  }

  for (let w = 1; w <= 15; w += 1) {
    assert(found.has(w), `缺少 W${w} 的视图组件骨架`);
  }
  for (const [folder, expected] of Object.entries(GROUP_EXPECTED)) {
    assert(groups.get(folder) === expected, `${folder}/ 视图数为 ${groups.get(folder) ?? 0}，应为 ${expected}`);
  }
  return `15 个界面：托盘 2 + 候选条 1 + 面板 8 + 向导 4`;
});

check('3. 显示名一律 AI-Bridge，无作废名 SecureBox', () => {
  const config = JSON.parse(read(join(DESKTOP, 'src-tauri', 'tauri.conf.json')));
  assert(config.productName === 'AI-Bridge', `productName 应为 AI-Bridge，实际 ${config.productName}`);
  assert(
    /<title>AI-Bridge<\/title>/.test(read(join(DESKTOP, 'index.html'))),
    'index.html 的 <title> 应为 AI-Bridge',
  );

  const selfPath = fileURLToPath(import.meta.url);
  const offenders = walk(DESKTOP)
    .filter((path) => path !== selfPath)
    .filter((path) => !/\.(png|ico|woff2?|ttf)$/i.test(path))
    .filter((path) => read(path).includes('SecureBox'));
  assert(offenders.length === 0, `出现作废名 SecureBox：${offenders.map(rel).join('、')}`);
  return 'productName / index.html / 全部源码均符合';
});

function rustWindowLabels() {
  const source = read(join(RUST_SRC, 'ui', 'window.rs'));
  const consts = new Map(
    [...source.matchAll(/pub const ([A-Z_]{3,})\s*:[^=]*=\s*"([^"]+)";/g)].map((m) => [m[1], m[2]]),
  );

  const block = source.slice(source.indexOf('pub const WINDOW_SPECS'));
  const end = block.indexOf('\n];');
  const body = (end === -1 ? block : block.slice(0, end)).replace(/\b([A-Z_]{3,})\b/g, (name) => {
    const value = consts.get(name);
    return value === undefined ? name : `"${value}"`;
  });
  return [...body.matchAll(/label:\s*"([^"]+)"/g)].map((m) => m[1]);
}

function tsWindowLabels() {
  const source = read(join(DESKTOP, 'src', 'ui', 'window-registry.ts'));
  const start = source.indexOf('export const WINDOW_VIEWS');
  const body = source.slice(source.indexOf('{', start), source.indexOf('\n};', start));
  return [...body.matchAll(/^\s{2}(?:'([^']+)'|([A-Za-z][\w-]*))\s*:/gm)].map((m) => m[1] ?? m[2]);
}

check('4. 窗口标签表 Rust WINDOW_SPECS ↔ TS WINDOW_VIEWS', () => {
  const rust = rustWindowLabels();
  const ts = tsWindowLabels();
  assert(rust.length === 13, `WINDOW_SPECS 窗口数为 ${rust.length}，应为 13`);
  assert(ts.length === 13, `WINDOW_VIEWS 窗口数为 ${ts.length}，应为 13`);
  const onlyRust = rust.filter((label) => !ts.includes(label));
  const onlyTs = ts.filter((label) => !rust.includes(label));
  assert(onlyRust.length === 0, `仅 Rust 声明：${onlyRust.join('、')}`);
  assert(onlyTs.length === 0, `仅 TS 声明：${onlyTs.join('、')}`);
  return `13 个标签双向一致（${rust.join(', ')}）`;
});

function rustMenuItems() {
  const source = read(join(RUST_SRC, 'ui', 'tray', 'menu.rs'));
  const block = source.slice(source.indexOf('pub const MENU_ITEMS'));
  return [...block.matchAll(/id:\s*"([^"]+)",\s*label:\s*"([^"]+)"/gs)].map((m) => [m[1], m[2]]);
}

function tsMenuItems() {
  const source = read(join(UI_SRC, 'tray', 'TrayMenu.vue'));
  return [...source.matchAll(/id:\s*'([^']+)',\s*label:\s*'([^']+)'/g)].map((m) => [m[1], m[2]]);
}

check('5. 托盘菜单 Rust MENU_ITEMS ↔ TrayMenu.vue', () => {
  const rust = rustMenuItems();
  const ts = tsMenuItems();
  assert(rust.length === 11, `MENU_ITEMS 项数为 ${rust.length}，应为 11`);
  assert(ts.length === 11, `TrayMenu.vue 项数为 ${ts.length}，应为 11`);
  rust.forEach(([id, label], index) => {
    assert(ts[index] !== undefined, `TrayMenu.vue 缺少第 ${index + 1} 项（${id}）`);
    assert(ts[index][0] === id, `第 ${index + 1} 项 id 不一致：Rust ${id} / TS ${ts[index][0]}`);
    assert(ts[index][1] === label, `第 ${index + 1} 项 label 不一致：Rust ${label} / TS ${ts[index][1]}`);
  });
  return `11 项 id + label 全等`;
});

const CONTRACT_MAP = [
  ['standard-access.schema.json', 'StandardAccess', ['properties']],
  ['standard-access.schema.json', 'CredentialRecord', ['$defs', 'credential', 'properties']],
  ['standard-access.schema.json', 'Cookie', ['$defs', 'cookie', 'properties']],
  ['standard-access.schema.json', 'SessionRecord', ['$defs', 'session', 'properties']],
  ['standard-access.schema.json', 'Target', ['$defs', 'target', 'properties']],
  ['standard-access.schema.json', 'Payload', ['$defs', 'payload', 'properties']],
  [
    'standard-access.schema.json',
    'PlainCredential',
    ['$defs', 'payload', 'properties', 'credential', 'properties'],
  ],
  ['standard-access.schema.json', 'Source', ['$defs', 'source', 'properties']],
  ['session-envelope.schema.json', 'SessionEnvelope', ['properties']],
  ['converter-manifest.schema.json', 'ConverterManifest', ['properties']],
  ['converter-manifest.schema.json', 'Signature', ['properties', 'signature', 'properties']],
];

function schemaNode(schema, path) {
  return path.reduce((node, key) => node[key], schema);
}

function tsInterfaceFields(source, name) {
  const match = source.match(new RegExp(`export interface ${name} \\{([\\s\\S]*?)\\n\\}`));
  assert(match !== null, `契约类型源码缺少 interface ${name}`);
  const body = match[1];
  return [...body.matchAll(/^\s{2}([A-Za-z_][A-Za-z0-9_]*)(\?)?:/gm)].map((m) => ({
    name: m[1],
    optional: m[2] === '?',
  }));
}

function loadContractTsSource() {
  const contractTs = read(join(DESKTOP, 'src', 'types', 'contract.ts'));
  if (/export\s+\*\s+from\s+['"]\.\.\/\.\.\/\.\.\/extensions\/shared\/types(?:\.ts)?['"]/.test(contractTs)) {
    return read(join(REPO, 'extensions', 'shared', 'types.ts'));
  }
  return contractTs;
}

check('6. 契约字段逐字段一致（contracts/*.schema.json ↔ src/types/contract.ts）', () => {
  const tsSource = loadContractTsSource();
  let fieldCount = 0;

  for (const [schemaFile, interfaceName, path] of CONTRACT_MAP) {
    const schema = JSON.parse(read(join(REPO, 'contracts', schemaFile)));
    const node = schemaNode(schema, path);
    const schemaFields = Object.keys(node);
    const required = new Set(schemaNode(schema, path.slice(0, -1)).required ?? []);
    const tsFields = tsInterfaceFields(tsSource, interfaceName);

    const tsNames = tsFields.map((field) => field.name);
    const missing = schemaFields.filter((field) => !tsNames.includes(field));
    const extra = tsNames.filter((field) => !schemaFields.includes(field));
    assert(missing.length === 0, `${interfaceName} 缺少字段：${missing.join('、')}`);
    assert(extra.length === 0, `${interfaceName} 多出字段：${extra.join('、')}`);

    for (const field of tsFields) {
      assert(
        /^[a-z][a-z0-9_]*$/.test(field.name),
        `${interfaceName}.${field.name} 不是 snake_case`,
      );
      if (required.has(field.name)) {
        assert(!field.optional, `${interfaceName}.${field.name} 在 schema 中必填，TS 侧不应可选`);
      }
    }
    fieldCount += tsFields.length;
  }
  return `${CONTRACT_MAP.length} 个类型 / ${fieldCount} 个字段一致（含必填性）`;
});

check('7. 依赖方向：native/ 与 bridge/ 不碰 tauri 与 ui；core 不反向依赖 desktop', () => {
  const nativeAndBridge = walk(RUST_SRC).filter(
    (path) => /^src-tauri\/src\/(native|bridge)\//.test(rel(path)) && path.endsWith('.rs'),
  );

  for (const path of nativeAndBridge) {
    const source = read(path);
    assert(!/^\s*use\s+tauri::/m.test(source), `${rel(path)} 依赖了 tauri（应只用 std / serde）`);
    assert(!/^\s*use\s+crate::ui/m.test(source), `${rel(path)} 反向依赖了 crate::ui`);
  }

  const coreFiles = existsSync(CORE_DIR)
    ? walk(CORE_DIR).filter((path) => path.endsWith('.rs') || path.endsWith('Cargo.toml'))
    : [];
  for (const path of coreFiles) {
    const source = read(path);
    assert(!/\bab[_-]desktop\b/.test(source), `${relRepo(path)} 反向依赖了 desktop`);
    assert(!/^\s*use\s+tauri::/m.test(source), `${relRepo(path)} 依赖了宿主框架 tauri`);
  }

  return `${nativeAndBridge.length} 个 native/bridge 文件 + ${coreFiles.length} 个 core 文件方向正确`;
});

check('8. SFC 块顺序 template → script → style，script 为 lang="ts"', () => {
  const files = viewFiles().concat([
    join(UI_SRC, 'components', 'WindowShell.vue'),
    join(UI_SRC, 'components', 'StateBlock.vue'),
  ]);

  for (const path of files) {
    const source = read(path);
    const template = source.indexOf('<template>');
    const script = source.indexOf('<script setup lang="ts">');
    const style = source.indexOf('<style');

    assert(template !== -1, `${rel(path)} 缺少 <template>`);
    assert(script !== -1, `${rel(path)} 缺少 <script setup lang="ts">`);
    assert(template < script, `${rel(path)} 块顺序错误：template 应在 script 之前`);
    if (style !== -1) {
      assert(script < style, `${rel(path)} 块顺序错误：style 应在 script 之后`);
    }
  }
  return `${files.length} 个 SFC 块顺序正确`;
});

let failed = 0;
for (const result of results) {
  if (result.ok) {
    console.log(`PASS  ${result.name} —— ${result.detail}`);
  } else {
    failed += 1;
    console.error(`FAIL  ${result.name}\n        ${result.detail}`);
  }
}

console.log(`\n结构一致性检查：${results.length - failed}/${results.length} 通过`);
process.exit(failed === 0 ? 0 : 1);
