/**
 * cookie.ts 实测：浏览器 cookie ↔ 契约 Cookie 双向映射。
 * 重点断言 http_only / secure / same_site / expires_at / domain / path 一个不丢，
 * 以及畸形输入一律报错而非静默丢属性。
 */
import test from "node:test";
import assert from "node:assert/strict";

import type { BrowserCookie, BrowserSameSite } from "../cookie.ts";
import { CookieMappingError, toBrowserCookie, toBrowserCookieSetDetails, toContractCookie } from "../cookie.ts";
import type { Cookie } from "../types.ts";

/** 2027-01-01T00:00:00Z 的秒级时间戳。 */
const EXPIRES_SECONDS = 1798761600;
const EXPIRES_ISO = new Date(EXPIRES_SECONDS * 1000).toISOString();

const chromiumCookie: BrowserCookie = {
  name: "sid",
  value: "abc123",
  domain: ".github.com",
  path: "/",
  secure: true,
  httpOnly: true,
  sameSite: "lax",
  expirationDate: EXPIRES_SECONDS,
  session: false,
};

test("浏览器 cookie → 契约 Cookie：全属性一个不丢", () => {
  const cookie = toContractCookie(chromiumCookie);
  assert.deepEqual(cookie, {
    name: "sid",
    value: "abc123",
    domain: ".github.com",
    path: "/",
    secure: true,
    http_only: true,
    same_site: "lax",
    expires_at: EXPIRES_ISO,
  });
  assert.equal(EXPIRES_ISO, "2027-01-01T00:00:00.000Z");
});

test("契约 Cookie → 浏览器 cookie：往返后四属性仍在", () => {
  const contract = toContractCookie(chromiumCookie);
  const back = toBrowserCookie(contract);
  assert.equal(back.httpOnly, true);
  assert.equal(back.secure, true);
  assert.equal(back.sameSite, "lax");
  assert.equal(back.expirationDate, EXPIRES_SECONDS);
  assert.equal(back.domain, ".github.com");
  assert.equal(back.path, "/");
  assert.deepEqual(toContractCookie(back), contract);
});

test("sameSite: none ↔ no_restriction（双向）", () => {
  const cookie = toContractCookie({ name: "a", value: "b", sameSite: "no_restriction" });
  assert.deepEqual(cookie, { name: "a", value: "b", same_site: "none" });
  assert.equal(toBrowserCookie(cookie).sameSite, "no_restriction");
  assert.deepEqual(toContractCookie({ name: "a", value: "b", sameSite: "strict" }), {
    name: "a",
    value: "b",
    same_site: "strict",
  });
});

test("session cookie：不产生 expires_at；unspecified：不产生 same_site", () => {
  const cookie = toContractCookie({
    name: "a",
    value: "b",
    sameSite: "unspecified",
    session: true,
    expirationDate: EXPIRES_SECONDS,
  });
  assert.deepEqual(cookie, { name: "a", value: "b" });

  const back = toBrowserCookie(cookie);
  assert.equal(back.sameSite, undefined);
  assert.equal(back.expirationDate, undefined);
  assert.equal(back.domain, undefined);
});

test("未知 sameSite 取值报错，不静默丢属性", () => {
  assert.throws(
    () => toContractCookie({ name: "a", value: "b", sameSite: "weird" as unknown as BrowserSameSite }),
    (error: unknown) => error instanceof CookieMappingError && error.field === "sameSite",
  );
});

test("畸形输入报错：缺 name / 非字符串 / NaN 过期时间 / 非法 expires_at", () => {
  assert.throws(() => toContractCookie({ value: "b" } as unknown as BrowserCookie), CookieMappingError);
  assert.throws(() => toContractCookie({ name: 1, value: "b" } as unknown as BrowserCookie), CookieMappingError);
  assert.throws(
    () => toContractCookie({ name: "a", value: "b", expirationDate: Number.NaN }),
    CookieMappingError,
  );
  assert.throws(
    () => toContractCookie({ name: "a", value: "b", expirationDate: Number.POSITIVE_INFINITY }),
    CookieMappingError,
  );
  assert.throws(
    () => toBrowserCookie({ name: "a", value: "b", expires_at: "2026-10-09 00:00:00" }),
    (error: unknown) => error instanceof CookieMappingError && error.field === "expires_at",
  );
  assert.throws(
    () =>
      toBrowserCookie({
        name: "a",
        value: "b",
        same_site: "weird" as unknown as NonNullable<Cookie["same_site"]>,
      }),
    (error: unknown) => error instanceof CookieMappingError && error.field === "same_site",
  );
});

test("写入参数：url 由 domain+path 推导，前导点在 URL 中去掉", () => {
  const details = toBrowserCookieSetDetails({
    name: "sid",
    value: "v",
    domain: ".github.com",
    path: "/settings",
    secure: true,
    http_only: true,
    same_site: "none",
    expires_at: EXPIRES_ISO,
  });
  assert.equal(details.url, "https://github.com/settings");
  assert.equal(details.domain, ".github.com");
  assert.equal(details.path, "/settings");
  assert.equal(details.secure, true);
  assert.equal(details.httpOnly, true);
  assert.equal(details.sameSite, "no_restriction");
  assert.equal(details.expirationDate, EXPIRES_SECONDS);
});

test("写入参数：无 domain 报错；无 expires_at 即会话 cookie", () => {
  assert.throws(
    () => toBrowserCookieSetDetails({ name: "a", value: "b" }),
    (error: unknown) => error instanceof CookieMappingError && error.field === "domain",
  );
  const details = toBrowserCookieSetDetails({ name: "a", value: "b", domain: "example.com" });
  assert.equal(details.url, "http://example.com/");
  assert.equal(details.path, "/");
  assert.equal(details.expirationDate, undefined);
  assert.equal(details.secure, undefined);
});

test("空字符串 domain / path 与「缺省」同义（浏览器 API 不接受空串）", () => {
  assert.deepEqual(toContractCookie({ name: "a", value: "b", domain: "", path: "" }), {
    name: "a",
    value: "b",
  });
});
