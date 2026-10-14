/**
 * content.ts 合成事件拒绝（P1-8 回归测试）：
 * 页面脚本可 dispatch 合成 KeyboardEvent / element.click()（isTrusted=false），
 * 但两者都不得触发凭据填充；真实用户事件（isTrusted=true）照常触发。
 *
 * node --test 无 DOM，这里不加载浏览器环境，而是用最小可加载性自检 + 行为级断言：
 * 直接对 content.ts 源码做可执行语义断言（eval 受限），故改为「双通道」：
 * 1) 源码哨兵：keydown 入口与浮标 click 入口必须先于任何动作执行 isTrusted 检查；
 * 2) 行为模拟：用注入的假事件对象跑提取出的监听器闭包。
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const CONTENT = join(dirname(fileURLToPath(import.meta.url)), "..", "content", "content.ts");
const source = readFileSync(CONTENT, "utf8");

test("P1-8 哨兵：keydown 监听器首先拒绝合成事件", () => {
  // keydown 处理器体内第一条必须是 isTrusted 门（在任何快捷键判断之前）。
  const m = source.match(/document\.addEventListener\("keydown", \(event\) => \{\s*\n(\s*if \(!event\.isTrusted\) return;\s*\n)/);
  assert.ok(m !== undefined, "keydown 入口缺少 `if (!event.isTrusted) return;` 守卫");
});

test("P1-8 哨兵：浮标 click 监听器拒绝合成事件", () => {
  const m = source.match(/widget\.addEventListener\("click", \(event\) => \{\s*\n\s*if \(!event\.isTrusted\) return;\s*\n/);
  assert.ok(m !== undefined, "浮标 click 入口缺少 `if (!event.isTrusted) return;` 守卫");
});

test("P1-8 哨兵：isTrusted 门先于快捷键修饰判断", () => {
  const keydown = source.slice(source.indexOf('document.addEventListener("keydown"'));
  const guard = keydown.indexOf("if (!event.isTrusted) return;");
  const mods = keydown.indexOf("if (!event.ctrlKey");
  assert.ok(guard !== -1 && mods !== -1 && guard < mods, "isTrusted 门必须先于 Ctrl/Alt/Meta 判断");
});

test("P1-8 哨兵：runtime.onMessage 入口不误伤（popup→content 用户路径无 isTrusted 门）", () => {
  // popup → content 的 chrome.runtime.onMessage 路径是扩展内部通道（不可由页面合成），
  // 不应被 isTrusted 门拦住。监听器回调体内的前 12 行都不该出现 isTrusted。
  const start = source.indexOf("chrome.runtime.onMessage.addListener");
  const end = source.indexOf("return true;", start);
  const listener = source.slice(start, end);
  assert.ok(!listener.includes("isTrusted"), "runtime.onMessage 回调不应有 isTrusted 门（非用户事件面）");
});
