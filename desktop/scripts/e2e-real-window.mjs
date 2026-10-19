#!/usr/bin/env node
/**
 * 桌面端真实窗口端到端验收（目标 ②）：建库 → 新建条目 → 重启后条目仍在。
 *
 * 为什么需要它：`check-ui-acceptance.mjs` 在 dev server 的裸浏览器里点按钮，
 * 只能证明「UI 会调用 invoke」，不能证明「数据真写进了 vault」。本脚本在
 * **真实 ab-desktop.exe 窗口**（WebView2）里走完整 UI 路径，再重启进程校验持久化。
 *
 * 做法：给 WebView2 加 `--remote-debugging-port`，经 CDP 操作真实 DOM。
 * 全程只点真实按钮、填真实表单，不直接调 invoke —— 走的就是用户路径。
 *
 * 用法：
 *   node scripts/e2e-real-window.mjs                    # 浅色默认：debug exe + 临时数据目录
 *   node scripts/e2e-real-window.mjs --exe <path>       # 指定 exe（如安装后的程序）
 *   node scripts/e2e-real-window.mjs --keep             # 保留数据目录，便于事后排查
 *
 * 退出码：0 = 全 PASS；1 = 有 FAIL（含 app 启动/渲染失败）。
 *
 * 前置：已在 desktop/ 下 npm 依赖就绪；exe 需先构建（cargo build 或 tools/build-installer.ps1）。
 */

import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const DESKTOP = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const REPO = resolve(DESKTOP, '..');

const argv = process.argv.slice(2);
const flag = (n) => argv.includes(n);
const opt = (n, fb) => {
  const i = argv.indexOf(n);
  return i >= 0 && argv[i + 1] ? argv[i + 1] : fb;
};

const PORT = Number(opt('--port', '9225'));
const EXE = resolve(opt('--exe', join(DESKTOP, 'src-tauri/target/debug/ab-desktop.exe')));
const KEEP = flag('--keep');

const TITLE = 'AI-Bridge E2E 条目';
const ACCOUNT = 'e2e@example.com';
const USERNAME = 'e2e-user';
const SECRET = 'e2e-secret-do-not-use';
const VAULT_NAME = 'E2E 库';
const VAULT_PASSWORD = 'e2e-master-password';

const results = [];
const pass = (n, d = '') => results.push({ ok: true, name: n, detail: d });
const fail = (n, d = '') => results.push({ ok: false, name: n, detail: d });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/* --------------------------------- CDP --------------------------------- */

class Cdp {
  constructor(ws) { this.ws = ws; this.id = 0; this.pending = new Map(); }
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
      }, 15000);
    });
  }
  async eval(expression) {
    const r = await this.send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
    if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? 'eval error');
    return r.result?.value;
  }
  close() { try { this.ws.close(); } catch { /* ignore */ } }
}

async function connect() {
  const deadline = Date.now() + 30000;
  let lastErr = 'unknown';
  while (Date.now() < deadline) {
    try {
      const list = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
      const page = list.find((t) => t.type === 'page' && /tauri\.localhost/.test(t.url));
      if (page) return await Cdp.open(page.webSocketDebuggerUrl);
      lastErr = `无应用页面（${list.length} 个 target）`;
    } catch (e) { lastErr = e.message; }
    await sleep(500);
  }
  throw new Error(`CDP ${PORT} 未就绪：${lastErr}`);
}

/* ------------------------------ DOM 操作助手 ------------------------------ */

const BODY_TEXT = `document.getElementById('app')?.innerText ?? ''`;

/** 按可见文案点按钮（找不到时回传当前全部按钮文案，便于定位）。 */
const CLICK_TEXT = (text) => `(() => {
  const btns = [...document.querySelectorAll('button')];
  const b = btns.find((x) => x.innerText.trim() === ${JSON.stringify(text)});
  if (!b) return 'NO_BUTTON: ' + btns.map((x) => x.innerText.trim()).join('|');
  if (b.disabled) return 'DISABLED';
  b.click();
  return 'OK';
})()`;

/** 让 Vue 的 v-model 感知：走原生 setter + 派发 input。 */
const SET_VALUE = (elementExpr, value) => `(() => {
  const el = ${elementExpr};
  if (!el) return 'NO_ELEMENT';
  const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  Object.getOwnPropertyDescriptor(proto, 'value').set.call(el, ${JSON.stringify(value)});
  el.dispatchEvent(new Event('input', { bubbles: true }));
  el.dispatchEvent(new Event('change', { bubbles: true }));
  return 'OK';
})()`;

const INPUT_BY_SELECTOR = (selector) => `document.querySelector(${JSON.stringify(selector)})`;
const INPUT_BY_LABEL = (label) => `(() => {
  const l = [...document.querySelectorAll('form label')]
    .find((x) => x.innerText.trim().startsWith(${JSON.stringify(label)}));
  return l ? l.querySelector('input') : null;
})()`;

async function waitFor(cdp, predicate, timeoutMs = 25000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const text = await cdp.eval(BODY_TEXT);
    if (predicate(text)) return text;
    await sleep(300);
  }
  return null;
}

/* ------------------------------- 进程管理 ------------------------------- */

let app = null;

function launch(dataDir, wv2Dir) {
  const env = {
    ...process.env,
    AIBRIDGE_DATA_DIR: dataDir,
    WEBVIEW2_USER_DATA_FOLDER: wv2Dir,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}`,
  };
  const child = spawn(EXE, ['aibridge://panel/library'], { env, stdio: 'ignore', windowsHide: true });
  child.stderr?.on('data', () => {});
  return child;
}

function stopApp() {
  if (!app) return;
  try { app.kill('SIGKILL'); } catch { /* ignore */ }
  app = null;
}

async function waitPortFree(timeoutMs = 15000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      await fetch(`http://127.0.0.1:${PORT}/json/version`);
      await sleep(300);
    } catch { return true; }
  }
  return false;
}

/* -------------------------------- 断言流程 -------------------------------- */

async function phaseSetup(cdp) {
  const text0 = await cdp.eval(BODY_TEXT);
  if (/未知窗口标签/.test(text0)) { fail('E1 窗口加载应用', '显示「未知窗口标签」'); return false; }
  if (!/解锁已有库/.test(text0)) { fail('E1 窗口加载应用', `门屏未渲染：${text0.slice(0, 120)}`); return false; }
  pass('E1 窗口加载应用', '真实 Tauri 窗口（tauri.localhost）内渲染 W3 库管理门屏');

  const tab = await cdp.eval(CLICK_TEXT('新建库'));
  if (tab !== 'OK') { fail('E2 建库', `切「新建库」页签失败：${tab}`); return false; }
  await sleep(400);

  const nameSet = await cdp.eval(SET_VALUE(INPUT_BY_SELECTOR('form.gate__form input[type="text"]'), VAULT_NAME));
  const pwdSet = await cdp.eval(SET_VALUE(INPUT_BY_SELECTOR('form.gate__form input[type="password"]'), VAULT_PASSWORD));
  if (nameSet !== 'OK' || pwdSet !== 'OK') {
    fail('E2 建库', `填表失败 name=${nameSet} pwd=${pwdSet}`);
    return false;
  }

  const submit = await cdp.eval(CLICK_TEXT('建库并生成恢复密钥'));
  if (submit !== 'OK') { fail('E2 建库', `提交失败：${submit}`); return false; }

  const recoveryText = await waitFor(cdp, (t) => /恢复密钥/.test(t) && /我已离线保存/.test(t));
  if (recoveryText === null) {
    const now = await cdp.eval(BODY_TEXT);
    fail('E2 建库', `未出现恢复密钥屏（S3 唯一后门必须当场展示）。当前：${now.slice(0, 300)}`);
    return false;
  }
  const key = (recoveryText.match(/恢复密钥（只显示这一次）\s*\n?\s*(\S+)/) ?? [])[1] ?? '';
  pass('E2 建库', `恢复密钥已展示（${key.slice(0, 12)}… / ${key.length} 字符）`);

  const ack = await cdp.eval(CLICK_TEXT('我已离线保存'));
  if (ack !== 'OK') { fail('E2b 确认恢复密钥', ack); return false; }
  pass('E2b 确认恢复密钥', '点击「我已离线保存」后进入库面板');

  const listText = await waitFor(cdp, (t) => /新建条目/.test(t));
  if (listText === null) { fail('E2c 进入库面板', '未出现条目列表'); return false; }
  pass('E2c 进入库面板', '门屏 → 条目列表切换成功');

  const openForm = await cdp.eval(CLICK_TEXT('新建条目'));
  if (openForm !== 'OK') { fail('E3 新建条目', `打开表单失败：${openForm}`); return false; }

  const fills = {
    标题: await cdp.eval(SET_VALUE(INPUT_BY_LABEL('标题'), TITLE)),
    账号引用: await cdp.eval(SET_VALUE(INPUT_BY_LABEL('账号引用'), ACCOUNT)),
    用户名: await cdp.eval(SET_VALUE(INPUT_BY_LABEL('用户名'), USERNAME)),
    秘密: await cdp.eval(SET_VALUE(INPUT_BY_LABEL('秘密'), SECRET)),
  };
  const bad = Object.entries(fills).filter(([, v]) => v !== 'OK');
  if (bad.length > 0) { fail('E3 新建条目', `填表失败：${JSON.stringify(bad)}`); return false; }

  const submitEntry = await cdp.eval(CLICK_TEXT('提交'));
  if (submitEntry !== 'OK') { fail('E3 新建条目', `提交失败：${submitEntry}`); return false; }

  const afterAdd = await waitFor(cdp, (t) => t.includes(TITLE));
  if (afterAdd === null) {
    const now = await cdp.eval(BODY_TEXT);
    fail('E3 新建条目', `列表未出现条目。当前：${now.slice(0, 400)}`);
    return false;
  }
  pass('E3 新建条目', `列表出现「${TITLE}」`);

  const leaked = await cdp.eval(`document.body.innerText.includes(${JSON.stringify(SECRET)})`);
  if (leaked === true) fail('E3b 列表不泄露明文', '列表出现 secret 明文（违反 S6 明文纪律）');
  else pass('E3b 列表不泄露明文', 'EntryView 不含 secret（S6 纪律成立）');

  return true;
}

async function phaseVerify(cdp) {
  const text0 = await cdp.eval(BODY_TEXT);
  if (!/解锁已有库/.test(text0)) {
    fail('E4 重启后回到锁定态', `重启后未显示解锁门屏：${text0.slice(0, 200)}`);
    return false;
  }
  pass('E4 重启后回到锁定态', '进程重启后库处于锁定态（进程内会话不持久，符合设计）');

  const pwdSet = await cdp.eval(SET_VALUE(INPUT_BY_SELECTOR('form.gate__form input[type="password"]'), VAULT_PASSWORD));
  if (pwdSet !== 'OK') { fail('E4 解锁', `填口令失败：${pwdSet}`); return false; }
  const unlock = await cdp.eval(CLICK_TEXT('解锁'));
  if (unlock !== 'OK') { fail('E4 解锁', `点击解锁失败：${unlock}`); return false; }

  const listText = await waitFor(cdp, (t) => t.includes(TITLE));
  if (listText === null) {
    const now = await cdp.eval(BODY_TEXT);
    fail('E4 重启后数据仍在', `解锁后列表未出现「${TITLE}」。当前：${now.slice(0, 400)}`);
    return false;
  }
  pass('E4 重启后数据仍在', `解锁后「${TITLE}」仍在（真写盘，非前端内存假象）`);

  if (listText.includes(ACCOUNT)) pass('E4b 字段完整', `account_ref=${ACCOUNT} 已回显`);
  else fail('E4b 字段完整', `列表缺 account_ref=${ACCOUNT}`);
  return true;
}

/* --------------------------------- 主流程 --------------------------------- */

console.log('AI-Bridge 真实窗口 E2E：建库 → 新建条目 → 重启后仍在\n');
console.log(`  exe  : ${EXE}`);
console.log(`  port : ${PORT}`);

if (!existsSync(EXE)) {
  console.log(`\nFAIL  找不到 exe：${EXE}\n      先构建：cargo build --manifest-path desktop/src-tauri/Cargo.toml --offline --features custom-protocol`);
  process.exit(1);
}

const workRoot = mkdtempSync(join(REPO, 'target', 'ab-e2e-'));
const dataDir = join(workRoot, 'data');
const wv2Dir = join(workRoot, 'wv2');
mkdirSync(dataDir, { recursive: true });
mkdirSync(wv2Dir, { recursive: true });

try {
  /* 阶段一：建库 + 建条目 */
  app = launch(dataDir, wv2Dir);
  let cdp = await connect();
  await cdp.send('Runtime.enable');
  await cdp.send('Page.enable');
  const setupOk = await phaseSetup(cdp);
  cdp.close();

  if (!setupOk) {
    /* setup 失败就不再进重启校验，但仍要清理进程 */
  } else {
    /* 校验数据真的落了盘（不只信 UI） */
    const vaultFile = join(dataDir, 'vault.aibvault');
    if (existsSync(vaultFile)) pass('E3c vault 文件已落盘', `${vaultFile}（${readFileSync(vaultFile).length} 字节）`);
    else fail('E3c vault 文件已落盘', `未找到 ${vaultFile}`);

    /* 阶段二：重启进程，校验持久化 */
    stopApp();
    const freed = await waitPortFree();
    if (!freed) {
      fail('E4 重启', '旧进程未释放调试端口');
    } else {
      app = launch(dataDir, wv2Dir);
      cdp = await connect();
      await cdp.send('Runtime.enable');
      await cdp.send('Page.enable');
      await phaseVerify(cdp);
      cdp.close();
    }
  }
} catch (e) {
  fail('E2E 执行', e.message);
} finally {
  stopApp();
  if (!KEEP) {
    await sleep(1200);
    try { rmSync(workRoot, { recursive: true, force: true }); } catch { /* Windows 句柄延迟，可忽略 */ }
  } else {
    console.log(`\n数据目录保留在：${workRoot}`);
  }
}

for (const r of results) {
  console.log(`  ${r.ok ? 'PASS' : 'FAIL'}  ${r.name}${r.detail ? `  — ${r.detail}` : ''}`);
}
const failed = results.filter((r) => !r.ok);
console.log(`\n结果：${results.length} 项，通过 ${results.length - failed.length}，失败 ${failed.length}`);
process.exit(failed.length > 0 ? 1 : 0);
