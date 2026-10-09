#!/usr/bin/env node
/**
 * AI-Bridge 桌面端视图渲染检查（Node 内跑，不依赖 esbuild / vite）。
 *
 * 目的：`vite build` 在本沙盒内不可执行（esbuild 需要以管道型 stdio 派生服务进程，被沙盒拒绝），
 * 因此这里用纯 JS 链路做等效的「能不能渲染」验证：
 *   @vue/compiler-sfc（compileScript + 内联模板）→ typescript 去类型 → Node 动态 import
 *   → @vue/server-renderer 渲染成 HTML → 断言非空且关键文案存在。
 *
 * 这是**运行期**验证：模板语法、<script setup> 宏（defineProps / withDefaults / defineEmits）、
 * props 默认值、状态容器（StateBlock）分支都真正执行一遍。
 *
 * 测试替身：`@tauri-apps/api/window` 被替换为 __shim__（宿主窗口 API 在 Node 中不存在）。
 *
 * 用法：node scripts/render-check.mjs（结束时删除 .render-check/ 临时目录）
 */

import { mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

import { parse, compileScript } from '@vue/compiler-sfc';
import ts from 'typescript';

const DESKTOP = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SRC = join(DESKTOP, 'src');
const OUT = join(DESKTOP, '.render-check');
const SHIM = join(OUT, '__shim__', 'tauri-window.mjs');
const TARGET_DEP = '@tauri-apps/api/window';

const warnings = [];

function walk(dir) {
  return readdirSync(dir).flatMap((entry) => {
    const full = join(dir, entry);
    return statSync(full).isDirectory() ? walk(full) : [full];
  });
}

function rewriteSpecifiers(code, outFile) {
  return code.replace(/(\bfrom\s*)(['"])([^'"]+)\2/g, (match, prefix, quote, specifier) => {
    let next = specifier;

    if (specifier === TARGET_DEP) {
      next = relative(dirname(outFile), SHIM).replaceAll('\\', '/');
      if (!next.startsWith('.')) next = `./${next}`;
    } else if (specifier.startsWith('.')) {
      next = specifier.replace(/\.vue$/, '').replace(/\.ts$/, '') + '.mjs';
    }

    return `${prefix}${quote}${next}${quote}`;
  });
}

function toJs(code) {
  return ts.transpileModule(code, {
    compilerOptions: {
      target: ts.ScriptTarget.ES2022,
      module: ts.ModuleKind.ESNext,
      isolatedModules: true,
      sourceMap: false,
    },
    fileName: 'sfc.ts',
  }).outputText;
}

/** .vue → 去类型的 ESM（模板内联进组件） */
function compileSfc(file) {
  const source = readFileSync(file, 'utf8');
  const { descriptor, errors } = parse(source, { filename: file });
  if (errors.length > 0) {
    throw new Error(`${relative(DESKTOP, file)} SFC 解析失败：${errors[0].message}`);
  }

  const id = relative(DESKTOP, file).replace(/[^a-z0-9]/gi, '');
  const result = compileScript(descriptor, {
    id,
    inlineTemplate: true,
    expressionPlugins: ['typescript'],
    templateOptions: { compilerOptions: { mode: 'module' } },
  });

  return toJs(result.content);
}

/** .ts → 去类型的 ESM */
function compileTs(file) {
  return toJs(readFileSync(file, 'utf8'));
}

function write(files) {
  for (const [outFile, code] of files) {
    mkdirSync(dirname(outFile), { recursive: true });
    writeFileSync(outFile, rewriteSpecifiers(code, outFile), 'utf8');
  }
}

async function main() {
  rmSync(OUT, { recursive: true, force: true });

  const sfcFiles = walk(SRC).filter((file) => file.endsWith('.vue'));
  const tsFiles = walk(SRC).filter((file) => file.endsWith('.ts'));

  // 宿主窗口 API 测试替身（仅 Node 渲染检查使用，不进入产物）
  const shim = `// 测试替身：Node 内不存在 Tauri 宿主窗口 API
const stub = {
  close: async () => {}, hide: async () => {}, destroy: async () => {},
  show: async () => {}, setAlwaysOnTop: async () => {}, setFocus: async () => {},
  label: 'render-check',
};
export function getCurrentWindow() { return stub; }
export default stub;
`;

  const compiled = sfcFiles.map((file) => [
    join(OUT, relative(SRC, file)).replace(/\.vue$/, '.mjs'),
    compileSfc(file),
  ]);
  compiled.push(...tsFiles.map((file) => [join(OUT, relative(SRC, file)).replace(/\.ts$/, '.mjs'), compileTs(file)]));
  compiled.push([SHIM, shim]);
  write(compiled);

  const { createSSRApp } = await import('vue');
  const { renderToString } = await import('@vue/server-renderer');

  const cases = [
    ['src/ui/tray/TrayMenu.vue', 'W1 托盘菜单', ['解锁 / 锁定', '退出']],
    ['src/ui/candidate/CandidateBar.vue', 'W2 候选条', ['Ctrl+Alt+P', 'TOTP']],
    ['src/ui/panels/LibraryPanel.vue', 'W3 库管理', ['AI-Bridge', '还没有条目']],
    ['src/ui/panels/StatusPanel.vue', 'W4 状态清单（空态）', ['暂无会话记录']],
    ['src/ui/panels/TemporarySessionPanel.vue', 'W5 临时会话（空态）', ['暂无临时会话']],
    ['src/ui/panels/AuditPanel.vue', 'W6 审计', ['暂无审计记录']],
    ['src/ui/panels/BackupPanel.vue', 'W7 备份', ['最后验证时间', '立即备份']],
    ['src/ui/panels/ConverterPanel.vue', 'W8 转换器（空态）', ['暂无转换器']],
    ['src/ui/panels/PairingPanel.vue', 'W9 配对', ['指纹比对']],
    ['src/ui/panels/SettingsPanel.vue', 'W10 设置', ['Ctrl+Alt+P', '30 秒']],
    ['src/ui/wizard/InitWizard.vue', 'W11 初始化向导', ['创建加密库', '保存恢复密钥']],
    ['src/ui/wizard/PairingWizard.vue', 'W12 配对向导', ['展示本机 QR']],
    ['src/ui/wizard/RestoreWizard.vue', 'W13 恢复向导', ['过期即拒解']],
    ['src/ui/wizard/RecoveryDrillWizard.vue', 'W14 恢复演练', ['比对条目数与完整性']],
    ['src/ui/tray/NotificationToast.vue', 'W15 通知 toast', ['备份已完成']],
  ];

  const failures = [];
  console.log('界面渲染检查（@vue/compiler-sfc + @vue/server-renderer，Node 内实跑）\n');

  for (const [file, name, expected] of cases) {
    const outFile = join(OUT, relative(SRC, file)).replace(/\.vue$/, '.mjs');
    try {
      const module = await import(pathToFileURL(outFile).href);
      const html = await renderToString(createSSRApp(module.default));

      if (html.length === 0) throw new Error('渲染结果为空');
      for (const text of expected) {
        if (!html.includes(text)) throw new Error(`渲染结果缺少文案「${text}」`);
      }
      console.log(`PASS  ${name.padEnd(20)} ${file}  → ${html.length} 字节 HTML，文案齐全`);
    } catch (error) {
      failures.push(`${name}（${file}）：${error.message}`);
      console.error(`FAIL  ${name.padEnd(20)} ${file}\n        ${error.message}`);
    }
  }

  if (warnings.length > 0) {
    console.log(`\nVue 运行期告警 ${warnings.length} 条：`);
    for (const warning of warnings) console.log(`  - ${warning}`);
  }

  console.log(`\n渲染检查：${cases.length - failures.length}/${cases.length} 通过`);
  return failures.length;
}

process.on('warning', (warning) => warnings.push(warning.message));

let exitCode = 1;
try {
  exitCode = await main();
} finally {
  rmSync(OUT, { recursive: true, force: true });
}
process.exit(exitCode);
