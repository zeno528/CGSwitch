import { onBeforeUnmount, onMounted } from "vue";

export interface WindowActivationHandlers {
  /** 窗口获得焦点或重新可见时触发（含从托盘唤出、最小化恢复） */
  onActive?: () => void;
  /** 窗口失去焦点或隐藏时触发 */
  onInactive?: () => void;
}

/**
 * 全局窗口激活逻辑：事件驱动、不轮询。
 * 约定是“用户看 APP 的瞬间才刷新”——focus / visibilitychange 就是那个瞬间。
 *
 * 使用方示例：
 * - 首页状态刷新（App.vue）
 * - DeepSeek 余额查询等“看的时候刷新”的场景
 *
 * 注意：本组合式函数只负责激活事件，首次加载由调用方在自己的 onMounted 里处理。
 */
export function useWindowActivation(handlers: WindowActivationHandlers) {
  const onFocus = () => handlers.onActive?.();
  const onBlur = () => handlers.onInactive?.();
  const onVisibilityChange = () => {
    if (document.hidden) {
      handlers.onInactive?.();
    } else {
      handlers.onActive?.();
    }
  };

  onMounted(() => {
    window.addEventListener("focus", onFocus);
    window.addEventListener("blur", onBlur);
    document.addEventListener("visibilitychange", onVisibilityChange);
  });

  onBeforeUnmount(() => {
    window.removeEventListener("focus", onFocus);
    window.removeEventListener("blur", onBlur);
    document.removeEventListener("visibilitychange", onVisibilityChange);
  });
}
