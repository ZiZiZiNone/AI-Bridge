/**
 * 请求序号守卫（frontend/rules/async-operations.md 竞态条件处理·方案 2）：
 * 单调序号判定「已返回但已过期」的结果，防止旧响应覆盖新状态。
 *
 * 与规范示例的差异（如实声明）：tauri `invoke` 走本机 IPC，没有 AbortSignal 通道，
 * 无法中断在途请求，故只实现序号判定这一半；不建不生效的 AbortController
 * （frontend/examples/golden/anti-examples.md「建了 controller 但 signal 从未下传」）。
 * 同时累计尝试次数，供 attemptMessage 显示重试次数。
 */

import { ref } from 'vue';
import { onScopeDispose } from 'vue';

export function useRequestGuard() {
  const latestToken = ref(0);
  const attempts = ref(0);

  /** 发起一次请求前调用；返回的 isStale 在结果返回后判定是否已过期。 */
  function start() {
    latestToken.value += 1;
    attempts.value += 1;
    const token = latestToken.value;
    return {
      isStale: () => token !== latestToken.value,
    };
  }

  /** 使所有在途结果过期（切换数据源 / 卸载时用）。 */
  function invalidateAll(): void {
    latestToken.value += 1;
  }

  onScopeDispose(invalidateAll);

  return { start, invalidateAll, attempts };
}
