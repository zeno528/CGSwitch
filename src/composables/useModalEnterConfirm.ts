import { onBeforeUnmount, onMounted } from "vue";

/**
 * 弹窗回车即确认 + 打开时自动聚焦主操作：
 * - 弹窗（n-modal / n-dialog）打开时聚焦“确定/删除”按钮，回车直接生效；
 * - 输入框与弹窗内按钮不抢（各有回车语义），输入法组词中不触发。
 */
export function useModalEnterConfirm() {
  function blurFocusOutsideModal() {
    const active = document.activeElement;
    if (!(active instanceof HTMLElement) || active === document.body) return;
    const visibleModal = [...document.querySelectorAll<HTMLElement>(".n-modal-container")]
      .reverse()
      .find((element) => element.getClientRects().length > 0);
    if (!visibleModal || !visibleModal.contains(active)) active.blur();
  }

  function findConfirmButton(overlay: HTMLElement) {
    return (
      overlay.querySelector<HTMLButtonElement>(
        ".n-dialog__action button.n-button:not(.n-button--ghost):not(.n-button--default-type):not([disabled])",
      ) ??
      overlay.querySelector<HTMLButtonElement>(
        "button.n-button--primary-type:not([disabled])",
      )
    );
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key !== "Enter" || event.repeat || event.isComposing) return;
    const target = event.target as HTMLElement | null;
    if (!target || target.closest('input, textarea, select, [contenteditable="true"]')) return;

    const overlay = [...document.querySelectorAll<HTMLElement>(".n-modal-container")]
      .reverse()
      .find((el) => el.getClientRects().length > 0);
    if (!overlay || (target.closest("button") && overlay.contains(target))) return;

    const confirm = findConfirmButton(overlay);
    if (!confirm) return;

    event.preventDefault();
    confirm.click();
  }

  let observer: MutationObserver | null = null;

  onMounted(() => {
    window.addEventListener("keydown", onKeydown);
    // naive-ui 弹窗默认把焦点放在第一个可聚焦元素（取消按钮），
    // 这里改为弹窗出现后聚焦主操作按钮（删除/确定），回车才能直接生效
    observer = new MutationObserver((mutations) => {
      for (const mutation of mutations) {
        for (const node of mutation.addedNodes) {
          if (
            node instanceof HTMLElement &&
            node.classList.contains("n-modal-container")
          ) {
            requestAnimationFrame(() =>
              findConfirmButton(node)?.focus({ preventScroll: true }),
            );
          }
        }
        if (
          [...mutation.removedNodes].some(
            (node) => node instanceof HTMLElement && node.classList.contains("n-modal-container"),
          )
        ) {
          blurFocusOutsideModal();
        }
        if (
          mutation.type === "attributes" &&
          mutation.target instanceof HTMLElement &&
          mutation.target.closest(".n-modal-container")
        ) {
          // VFocusTrap 会在 leave 动画开始前恢复触发按钮焦点，必须同步清掉，避免闪一帧。
          blurFocusOutsideModal();
        }
      }
    });
    observer.observe(document.body, {
      attributes: true,
      attributeFilter: ["class", "style"],
      childList: true,
      subtree: true,
    });
  });

  onBeforeUnmount(() => {
    window.removeEventListener("keydown", onKeydown);
    observer?.disconnect();
  });
}
