package com.aibridge.mobile.core

import com.aibridge.mobile.ffi.Clock

/**
 * Clock trait 的宿主实现（core 不自取系统时间，TTL 判定经此注入）。
 * 实例由组合根构造并显式传递，不是单例。
 */
class SystemClock : Clock {
    override fun nowUnixSecs(): Long = System.currentTimeMillis() / 1000L
}
