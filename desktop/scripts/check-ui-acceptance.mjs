#!/usr/bin/env node
/**
 * AI-Bridge 桌面端 UI 验收检查（纯文本输出，不依赖视觉）
 *
 * 为什么存在：`check:structure` / `render-check` 只验证「能不能渲染出文案」，
 * 不验证「按钮点了有没有反应」。结果是骨架界面能拿满分，而按钮全是死的。
 * 本脚本把「点了没反应」变成可执行断言，输出 PASS/FAIL 文本行。
 *
 * 两类检查：
 *   A. 静态检查（不需要浏览器）
 *      A1. desktop/src/ 内 invoke( 调用点数 > 0
 *      A2. 每个 <button> 都有 @click / v-on:click（或 type="submit" 交给表单）
 *   B. 运行期检查（需要 dev server + Edge/Chrome）
 *      B1. 15 个窗口标签都能挂载，无一显示「未知窗口标签」
 *      B2. 逐个点击每个按钮，必须产生「反应」：DOM 变化 / 控制台报错 / 弹出对话框
 *          三者皆无 = 静默失效 = FAIL（这就是用户说的「点都点不了」）
 *
 * 用法：
 *   node scripts/check-ui-acceptance.mjs                    # 全量（需先 npm run dev）
 *   node scripts/check-ui-acceptance.mjs --static           # 只跑静态检查
 *   node scripts/check-ui-acceptance.mjs --json report.json # 另存 JSON
 *   node scripts/check-ui-acceptance.mjs --base http://localhost:1420
 *
 * 退出码：0 = 全部通过；1 = 有 FAIL。
 */

import { spawn } from 'node:child_process';
import { existsSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';

const DESKTOP = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SRC = join(DESKTOP, 'src');

const argv = process.argv.slice(2);
const flag = (name) => argv.includes(name);
const opt = (name, fallback) => {
  const i = argv.indexOf(name);
  return i >= 0 && argv[i + 1] ? argv[i + 1] : fallback;
};

const STATIC_ONLY = flag('--static');
const BASE = opt('--base', 'http://localhost:1420');
const JSON_OUT = opt('--json', '');
const DEBUG_PORT = Number(opt('--port', '9333'));

/** 15 个窗口标签（权威定义在 src-tauri/src/ui/window.rs 的 WINDOW_SPECS + 2 个 dev 预览）。 */
const LABELS = [
  'candidate',
  'panel-library', 'panel-status', 'panel-sessions', 'panel-audit',
  'panel-backup', 'panel-converters', 'panel-pairing', 'panel-settings',
  'wizard-init', 'wizard-pairing', 'wizard-restore', 'wizard-drill',
  'preview-tray-menu', 'preview-notification-toast',
];

const results = [];
const pass = (name, detail = '') => results.push({ ok: true, name, detail });
const fail = (name, detail = '') => results.push({ ok: false, name, detail });

function walk(dir) {
  return readdirSync(dir).flatMap((e) => {
    const full = join(dir, e);
    return statSync(full).isDirectory() ? walk(full) : [full];
  });
}

/* ---------------- A. 静态检查 ---------------- */

function checkInvokeExists() {
  const files = walk(SRC).filter((f) => f.endsWith('.vue') || f.endsWith('.ts'));
  const hits = [];
  for (const f of files) {
    const text = readFileSync(f, 'utf8');
    // 只认真正的调用（排除注释里的示意）
    text.split('\n').forEach((line, i) => {
      if (/\binvoke\s*\(/.test(line) && !/^\s*(\/\/|\*|\/\*)/.test(line)) {
        hits.push(`${relative(DESKTOP, f).replaceAll('\\', '/')}:${i + 1}`);
      }
    });
  }
  if (hits.length === 0) {
    fail('A1 前端存在 invoke 调用', '0 处 —— 前端从未调用任何 tauri command，所有业务按钮必然静默失效');
  } else {
    pass('A1 前端存在 invoke 调用', `${hits.length} 处：${hits.slice(0, 6).join(', ')}${hits.length > 6 ? ' …' : ''}`);
  }
}

/** 收集 <button> 起止标签（允许属性跨行）。 */
function findButtons(text) {
  const out = [];
  const re = /<button\b[\s\S]*?>/g;
  let m;
  while ((m = re.exec(text)) !== null) {
    const line = text.slice(0, m.index).split('\n').length;
    out.push({ raw: m[0], line });
  }
  return out;
}

function checkDeadButtons() {
  const vueFiles = walk(SRC).filter((f) => f.endsWith('.vue'));
  const dead = [];
  let total = 0;
  for (const f of vueFiles) {
    const text = readFileSync(f, 'utf8');
    for (const b of findButtons(text)) {
      total += 1;
      const hasClick = /@click|v-on:click/.test(b.raw);
      // type="submit" 的表单按钮合法：动作由表单的 @submit 承载
      const isSubmit = /type\s*=\s*["']submit["']/.test(b.raw);
      if (!hasClick && !isSubmit) {
        dead.push(`${relative(DESKTOP, f).replaceAll('\\', '/')}:${b.line}`);
      }
    }
  }
  if (dead.length === 0) {
    pass('A2 所有 button 都绑定了 @click', `共 ${total} 个 button，无死按钮`);
  } else {
    fail('A2 所有 button 都绑定了 @click', `${dead.length}/${total} 个死按钮：${dead.join(', ')}`);
  }
}

/* ---------------- B. 运行期检查 ---------------- */

function findBrowser() {
  const candidates = [
    process.env.EDGE_PATH,
    process.env.CHROME_PATH,
    'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe',
    'C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe',
    'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
    'C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe',
  ].filter(Boolean);
  return candidates.find((p) => existsSync(p)) ?? null;
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function waitForCdp(port, timeoutMs = 20000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const r = await fetch(`http://127.0.0.1:${port}/json/version`);
      if (r.ok) return await r.json();
    } catch { /* 还没起来 */ }
    await sleep(400);
  }
  return null;
}

class Cdp {
  constructor(ws) { this.ws = ws; this.id = 0; this.pending = new Map(); this.events = []; }
  static async open(url) {
    const ws = new WebSocket(url);
    await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
    const c = new Cdp(ws);
    ws.onmessage = (ev) => {
      const m = JSON.parse(ev.data);
      if (m.id !== undefined && c.pending.has(m.id)) {
        const { res, rej } = c.pending.get(m.id);
        c.pending.delete(m.id);
        m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result);
      } else if (m.method) {
        c.events.push(m);
      }
    };
    return c;
  }
  send(method, params = {}) {
    const id = ++this.id;
    return new Promise((res, rej) => {
      this.pending.set(id, { res, rej });
      this.ws.send(JSON.stringify({ id, method, params }));
      setTimeout(() => {
        if (this.pending.has(id)) { this.pending.delete(id); rej(new Error(`timeout ${method}`)); }
      }, 20000);
    });
  }
  clear() { this.events = []; }
  errors() {
    const out = [];
    for (const e of this.events) {
      if (e.method === 'Runtime.exceptionThrown') {
        out.push(e.params.exceptionDetails?.exception?.description ?? e.params.exceptionDetails?.text ?? 'uncaught');
      }
      if (e.method === 'Runtime.consoleAPICalled' && e.params.type === 'error') {
        out.push((e.params.args ?? []).map((a) => a.value ?? a.description ?? a.type).join(' '));
      }
    }
    return [...new Set(out)];
  }
  close() { try { this.ws.close(); } catch { /* ignore */ } }
}

/**
 * 页面「活动签名」：用于判断点击后是否有反应。
 * 必须包含 innerHTML 的散列 —— 只比 innerText 会漏掉「仅切换 CSS 类」的反应
 * （如向导步骤高亮 `steps__item--active`），把正常功能误报成死按钮。
 */
const ACTIVITY_SIG = `(() => {
  const app = document.getElementById('app');
  if (!app) return '';
  const html = app.innerHTML;
  let h = 0;
  for (let i = 0; i < html.length; i++) { h = (h * 31 + html.charCodeAt(i)) | 0; }
  return JSON.stringify({
    h,
    len: html.length,
    text: app.innerText,
    nodes: app.querySelectorAll('*').length,
    dialogs: document.querySelectorAll('[role=dialog],dialog,.modal,.overlay').length,
    forms: document.querySelectorAll('#app form').length,
  });
})()`;

/** 按钮清单：带 disabled 状态，禁用的按钮不参与「无反应」判定。 */
const BUTTONS = `[...document.querySelectorAll('button')].map(b => ({ label: b.innerText.trim(), disabled: b.disabled }))`;

async function runtimeChecks() {
  // dev server 可达性
  try {
    const r = await fetch(BASE, { method: 'GET' });
    if (!r.ok) throw new Error(`HTTP ${r.status}`);
  } catch (e) {
    fail('B0 dev server 可达', `${BASE} 不可达（${e.message}）。先执行：cd desktop && npm run dev`);
    return;
  }
  pass('B0 dev server 可达', BASE);

  const browser = findBrowser();
  if (!browser) {
    fail('B0 找到浏览器', '未找到 Edge/Chrome；用 EDGE_PATH 环境变量指定');
    return;
  }

  // profile 放系统临时目录，不污染仓库（放 desktop/ 下会进 git status）
  const profile = join(tmpdir(), `ab-ui-acceptance-${process.pid}`);
  const child = spawn(browser, [
    '--headless=new', '--disable-gpu', '--no-first-run', '--no-default-browser-check',
    `--remote-debugging-port=${DEBUG_PORT}`, `--user-data-dir=${profile}`,
    '--window-size=1000,800', 'about:blank',
  ], { stdio: 'ignore', windowsHide: true });

  try {
    const ver = await waitForCdp(DEBUG_PORT);
    if (!ver) { fail('B0 启动浏览器调试端口', `端口 ${DEBUG_PORT} 未就绪`); return; }

    const list = await (await fetch(`http://127.0.0.1:${DEBUG_PORT}/json/list`)).json();
    const page = list.find((t) => t.type === 'page');
    if (!page) { fail('B0 取得页面目标', '无 page target'); return; }

    const cdp = await Cdp.open(page.webSocketDebuggerUrl);
    await cdp.send('Page.enable');
    await cdp.send('Runtime.enable');

    const mountBad = [];
    const unknownLabel = [];
    const silent = [];
    const clickErrors = [];
    const skippedDisabled = [];

    for (const windowLabel of LABELS) {
      cdp.clear();
      await cdp.send('Page.navigate', { url: `${BASE}/?window=${windowLabel}` });
      await sleep(1200);

      const sig0 = await cdp.send('Runtime.evaluate', { expression: ACTIVITY_SIG, returnByValue: true });
      const state = JSON.parse(sig0.result?.value ?? '{}');

      if (/未知窗口标签/.test(state.text ?? '')) { unknownLabel.push(windowLabel); continue; }
      if ((state.nodes ?? 0) === 0) { mountBad.push(windowLabel); continue; }

      const btns = await cdp.send('Runtime.evaluate', { expression: BUTTONS, returnByValue: true });
      const buttons = btns.result?.value ?? [];

      for (let i = 0; i < buttons.length; i++) {
        const btnLabel = buttons[i].label;
        // 禁用按钮点了本来就不该有反应，跳过（不算通过也不算失败）
        if (buttons[i].disabled) { skippedDisabled.push(`${windowLabel} / ${btnLabel}`); continue; }

        cdp.clear();
        // 关键：每次点击前重新抓基线。若只在循环外抓一次，
        // 一旦前面的按钮改变了 DOM（如「置顶」改名为「取消置顶」），
        // 后面所有死按钮都会被误判为「有变化」——gate 会给出假通过。
        const base = await cdp.send('Runtime.evaluate', { expression: ACTIVITY_SIG, returnByValue: true });
        await cdp.send('Runtime.evaluate', {
          expression: `(() => { const b = document.querySelectorAll('button')[${i}]; if (b) b.click(); })()`,
          returnByValue: true,
        });
        await sleep(320);
        const sig1 = await cdp.send('Runtime.evaluate', { expression: ACTIVITY_SIG, returnByValue: true });
        const errs = cdp.errors();
        const changed = (sig1.result?.value ?? '') !== (base.result?.value ?? '');
        if (errs.length > 0) {
          clickErrors.push(`${windowLabel} / ${btnLabel} → ${errs[0].split('\n')[0].slice(0, 120)}`);
        } else if (!changed) {
          silent.push(`${windowLabel} / ${btnLabel}`);
        }
      }
    }

    if (mountBad.length === 0) pass('B1 全部窗口可挂载', `${LABELS.length} 个标签`);
    else fail('B1 全部窗口可挂载', `未挂载：${mountBad.join(', ')}`);

    if (unknownLabel.length === 0) pass('B2 无未知窗口标签', '');
    else fail('B2 无未知窗口标签', `出现「未知窗口标签」：${unknownLabel.join(', ')}`);

    if (silent.length === 0) {
      pass('B3 无静默失效按钮', '每个按钮点击后都产生了 DOM 变化或报错');
    } else {
      fail('B3 无静默失效按钮', `${silent.length} 个按钮点了无任何反应：\n      ${silent.join('\n      ')}`);
    }

    // 报错不算失败（dev 模式下点窗口控制按钮必然因缺 __TAURI_INTERNALS__ 报错），
    // 但要列出来，便于区分「预期的 dev 环境报错」与「真实缺陷」。
    if (clickErrors.length === 0) pass('B4 点击无报错', '');
    else pass('B4 点击报错清单（dev 环境下窗口控制按钮属预期）', `${clickErrors.length} 条：\n      ${clickErrors.slice(0, 5).join('\n      ')}`);

    // 禁用按钮不参与判定，但列出来，避免「跳过」被误读成「通过」
    if (skippedDisabled.length === 0) pass('B5 无被跳过的禁用按钮', '');
    else pass('B5 跳过的禁用按钮（点了本就不该有反应）', `${skippedDisabled.length} 个：${[...new Set(skippedDisabled)].slice(0, 8).join(', ')}`);

    cdp.close();
  } finally {
    try { child.kill(); } catch { /* ignore */ }
  }
}

/* ---------------- 主流程 ---------------- */

console.log('AI-Bridge 桌面端 UI 验收检查（纯文本输出）\n');
console.log('静态检查');
checkInvokeExists();
checkDeadButtons();
for (const r of results) console.log(`  ${r.ok ? 'PASS' : 'FAIL'}  ${r.name}${r.detail ? `  — ${r.detail}` : ''}`);

if (!STATIC_ONLY) {
  console.log('\n运行期检查');
  const before = results.length;
  await runtimeChecks();
  for (const r of results.slice(before)) console.log(`  ${r.ok ? 'PASS' : 'FAIL'}  ${r.name}${r.detail ? `  — ${r.detail}` : ''}`);
}

const failed = results.filter((r) => !r.ok);
console.log(`\n结果：${results.length} 项，通过 ${results.length - failed.length}，失败 ${failed.length}`);
if (failed.length > 0) {
  console.log('失败项：');
  for (const f of failed) console.log(`  FAIL  ${f.name}`);
}

if (JSON_OUT) {
  writeFileSync(JSON_OUT, JSON.stringify({ base: BASE, results }, null, 2), 'utf8');
  console.log(`\nJSON 报告：${JSON_OUT}`);
}

process.exit(failed.length > 0 ? 1 : 0);
