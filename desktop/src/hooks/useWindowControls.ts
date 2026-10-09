import { getCurrentWindow } from '@tauri-apps/api/window';

/**
 * 窗口控制（纯交互副作用，无业务判断）。
 * 位置按 frontend/frameworks/vue3/composable.md：`src/hooks/`。
 * 这里是 UI 侧唯一调用窗口 API 的地方（UI 不直接碰 Tauri 窗口对象）。
 */
export function useWindowControls() {
  async function close(): Promise<void> {
    await getCurrentWindow().close();
  }

  async function hide(): Promise<void> {
    await getCurrentWindow().hide();
  }

  /** 一次性窗口（向导 W11–W14）用完即焚：destroy 销毁而非 hide 复用。 */
  async function destroy(): Promise<void> {
    await getCurrentWindow().destroy();
  }

  async function setAlwaysOnTop(value: boolean): Promise<void> {
    await getCurrentWindow().setAlwaysOnTop(value);
  }

  return { close, hide, destroy, setAlwaysOnTop };
}
