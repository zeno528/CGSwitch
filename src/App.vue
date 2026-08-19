<script setup lang="ts">
import { computed, defineAsyncComponent, onBeforeUnmount, onMounted, ref, watch } from "vue";
import {
  NConfigProvider,
  NDialogProvider,
  NLayout,
  NMessageProvider,
  darkTheme,
} from "naive-ui";
import ProfilesView from "./views/ProfilesView.vue";
import { api, isTauri } from "./api";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useWindowActivation } from "./composables/useWindowActivation";
import { useModalEnterConfirm } from "./composables/useModalEnterConfirm";
import { darkThemeOverrides, themeOverrides } from "./theme";
import type { AppState, Settings } from "./types";
import { PhGearSix, PhMinus, PhSquare, PhStack, PhX } from "@phosphor-icons/vue";
import McpIcon from "./components/McpIcon.vue";

// 设置页/MCP 页按需加载：进入时才拉取，不拖累启动入口
const SettingsView = defineAsyncComponent(() => import("./views/SettingsView.vue"));
const McpView = defineAsyncComponent(() => import("./views/McpView.vue"));

type View = "profiles" | "mcp" | "settings";

const view = ref<View>("profiles");
const profilesNavReset = ref(0);
const mcpNavReset = ref(0);
// 侧边栏折叠状态：默认收缩，选择记忆在 localStorage（WebView2 应用专属存储），重启后还原
const sidebarCollapsed = ref(localStorage.getItem("cgswitch.sidebar-collapsed") !== "0");
const sidebarFlyoutArmed = ref(true);
const profilesNavBtn = ref<HTMLElement | null>(null);
const mcpNavBtn = ref<HTMLElement | null>(null);
const settingsNavBtn = ref<HTMLElement | null>(null);
const sidebarNavRef = ref<HTMLElement | null>(null);
const indicatorTop = ref(8);
const indicatorLeft = ref(0);
const state = ref<AppState | null>(null);
const loadError = ref("");
const systemDark = ref(window.matchMedia("(prefers-color-scheme: dark)").matches);
let codexPollTimer: number | undefined;
let codexPolling = false;

function startCodexPolling() {
  if (codexPollTimer !== undefined) return;
  codexPollTimer = window.setInterval(pollCodexStatus, 3000);
}

function stopCodexPolling() {
  if (codexPollTimer !== undefined) {
    window.clearInterval(codexPollTimer);
    codexPollTimer = undefined;
  }
}

function syncCodexPolling() {
  if (document.hidden) {
    stopCodexPolling();
  } else {
    startCodexPolling();
  }
}

const media = window.matchMedia("(prefers-color-scheme: dark)");
const mediaListener = (event: MediaQueryListEvent) => {
  systemDark.value = event.matches;
};

const isDark = computed(() => {
  const theme = state.value?.settings.theme ?? "system";
  return theme === "dark" || (theme === "system" && systemDark.value);
});
const naiveTheme = computed(() => (isDark.value ? darkTheme : null));
const isSidebarCollapsed = sidebarCollapsed;

async function syncWindowTitleBarTheme(dark: boolean) {
  if (!isTauri) return;
  await api.setWindowTheme(dark);
}

const appWindow = isTauri ? getCurrentWindow() : null;

function windowMinimize() {
  void appWindow?.minimize();
}

function windowToggleMaximize() {
  void appWindow?.toggleMaximize();
}

function windowClose() {
  void appWindow?.close();
}

function updateSidebarIndicator() {
  const target =
    view.value === "profiles"
      ? profilesNavBtn.value
      : view.value === "mcp"
        ? mcpNavBtn.value
        : settingsNavBtn.value;
  const nav = sidebarNavRef.value;
  if (target && nav) {
    indicatorTop.value = target.getBoundingClientRect().top - nav.getBoundingClientRect().top + 8;
    // 指示条贴着导航按钮左边缘
    indicatorLeft.value = target.offsetLeft;
  }
}

watch(view, updateSidebarIndicator);

function toggleSidebar() {
  sidebarCollapsed.value = !sidebarCollapsed.value;
  localStorage.setItem("cgswitch.sidebar-collapsed", sidebarCollapsed.value ? "1" : "0");
  if (sidebarCollapsed.value) sidebarFlyoutArmed.value = false;
  window.setTimeout(updateSidebarIndicator, 360);
}

// 点击侧边栏“供应商配置”始终回到首页列表（退出编辑等子视图）
function goProfiles() {
  profilesNavReset.value++;
  view.value = "profiles";
}

// 点击侧边栏“MCP 服务器”同样回到列表（退出添加/编辑子页）
function goMcp() {
  mcpNavReset.value++;
  view.value = "mcp";
}

async function refresh() {
  try {
    state.value = await api.getState();
    loadError.value = "";
  } catch (error) {
    loadError.value = String(error);
  }
}

async function pollCodexStatus() {
  if (codexPolling || !state.value) return;
  codexPolling = true;
  try {
    const codex = await api.getCodexStatus();
    if (state.value) state.value = { ...state.value, codex };
  } catch {
    // 轮询失败时保留上次状态，不打扰用户
  } finally {
    codexPolling = false;
  }
}

async function saveSettings(settings: Settings) {
  if (!state.value) return;
  state.value = { ...state.value, settings };
}

async function previewTheme(theme: Settings["theme"]) {
  if (!state.value || state.value.settings.theme === theme) return;
  state.value = { ...state.value, settings: { ...state.value.settings, theme } };
}

watch(
  isDark,
  (dark) => {
    // 先同步切 `.dark`/colorScheme，与 naive-ui 主题同帧生效；
    // 标题栏 IPC 异步进行，不再阻塞（否则两轨不同步会闪一帧）
    const root = document.documentElement;
    // 切主题期间冻结全部过渡：侧边栏按钮等 transition-colors 会在主题翻转时
    // 产生 150ms 颜色渐变（文字/图标尾帧闪烁），冻结后整页同帧切换、无动画
    root.classList.add("theme-switching");
    root.classList.toggle("dark", dark);
    root.style.colorScheme = dark ? "dark" : "light";
    requestAnimationFrame(() => root.classList.remove("theme-switching"));
    void syncWindowTitleBarTheme(dark).catch(() => {
      // 标题栏同步失败不阻塞内容切换
    });
  },
  { immediate: true },
);

onMounted(async () => {
  media.addEventListener("change", mediaListener);
  updateSidebarIndicator();
  const main = document.querySelector("main");
  if (main) {
    document.documentElement.style.setProperty(
      "--scrollbar-size",
      `${main.offsetWidth - main.clientWidth}px`,
    );
  }
  await refresh();
  // 首帧渲染完成后再显示窗口，避免启动时出现空白/残影帧；静默启动保持不显示
  if (isTauri && !state.value?.settings.silent_start) {
    void appWindow?.show();
  }
  syncCodexPolling();
});

onBeforeUnmount(() => {
  stopCodexPolling();
  media.removeEventListener("change", mediaListener);
});

// 窗口激活（含从托盘唤出）即同步一次 live → 数据库/卡片；失焦/隐藏时暂停 Codex 状态轮询
useWindowActivation({
  onActive: () => {
    void refresh();
    syncCodexPolling();
  },
  onInactive: () => stopCodexPolling(),
});

// 全局弹窗快捷键：回车 = 确定（n-modal / n-dialog 通用）
useModalEnterConfirm();
</script>

<template>
  <n-config-provider :theme="naiveTheme" :theme-overrides="isDark ? darkThemeOverrides : themeOverrides" inline-theme-disabled>
    <n-dialog-provider>
      <n-message-provider :container-style="{ top: '44px' }">
        <n-layout class="h-full! rounded-none! bg-transparent!">
          <div class="flex h-screen flex-col">
            <div class="flex h-8 shrink-0 items-center bg-[var(--app-bg)]">
              <div data-tauri-drag-region class="relative flex h-full shrink-0 items-center bg-[var(--sidebar-bg)] transition-[width] duration-[360ms] ease-[cubic-bezier(0.22,1,0.36,1)]" :class="isSidebarCollapsed ? ['w-12', 'apple-sidebar--collapsed'] : 'w-[128px]'">
                <div
                  class="apple-sidebar-brand flex h-full w-fit cursor-pointer items-center pt-2"
                  role="button"
                  tabindex="0"
                  aria-label="CGswitch"
                  @click="toggleSidebar"
                  @keyup.enter="toggleSidebar"
                  @mouseenter="sidebarFlyoutArmed = true"
                  @mouseleave="sidebarFlyoutArmed = false"
                >
                  <img src="/logo.svg" alt="CGswitch" class="h-6 w-6 shrink-0 dark:invert" draggable="false" />
                  <span class="apple-sidebar-label apple-wordmark whitespace-nowrap">CGswitch</span>
                </div>
                <span v-if="sidebarFlyoutArmed" class="apple-sidebar-flyout" aria-hidden="true">{{ isSidebarCollapsed ? "展开侧边栏" : "收缩侧边栏" }}</span>
              </div>
              <div data-tauri-drag-region class="min-w-0 flex-1 self-stretch" />
              <div class="flex h-full items-center">
                <button type="button" class="grid h-8 w-10 place-items-center text-[var(--text-secondary)] transition-colors hover:bg-black/6 dark:hover:bg-white/10" aria-label="最小化" @click="windowMinimize">
                  <PhMinus class="h-4 w-4" weight="bold" aria-hidden="true" />
                </button>
                <button type="button" class="grid h-8 w-10 place-items-center text-[var(--text-secondary)] transition-colors hover:bg-black/6 dark:hover:bg-white/10" aria-label="最大化" @click="windowToggleMaximize">
                  <PhSquare class="h-4 w-4" weight="bold" aria-hidden="true" />
                </button>
                <button type="button" class="grid h-8 w-10 place-items-center text-[var(--text-secondary)] transition-colors hover:bg-[#e81123] hover:text-white" aria-label="关闭" @click="windowClose">
                  <PhX class="h-4 w-4" weight="bold" aria-hidden="true" />
                </button>
              </div>
            </div>
            <div class="flex min-h-0 flex-1">
            <aside class="apple-sidebar relative h-full shrink-0" :class="isSidebarCollapsed ? ['w-12', 'apple-sidebar--collapsed'] : 'w-[128px]'">
              <nav ref="sidebarNavRef" class="relative mx-1.5 mt-3 space-y-1">
                <span class="apple-sidebar-indicator" :style="{ top: `${indicatorTop}px`, left: `${indicatorLeft}px` }" aria-hidden="true" />
                <button ref="profilesNavBtn" type="button" class="apple-sidebar-nav-button relative flex h-9 w-full items-center rounded-[10px] text-sm transition-colors" :class="view === 'profiles' ? 'bg-[var(--selection-bg)] font-semibold text-accent' : 'font-normal hover:bg-black/5 dark:hover:bg-white/8'" aria-label="供应商配置" @click="goProfiles" @mouseenter="sidebarFlyoutArmed = true">
                  <PhStack class="h-[18px] w-[18px] shrink-0" weight="bold" aria-hidden="true" />
                  <span class="apple-sidebar-label" :aria-hidden="isSidebarCollapsed">供应商配置</span>
                  <span v-if="isSidebarCollapsed && sidebarFlyoutArmed" class="apple-sidebar-flyout" aria-hidden="true">供应商配置</span>
                </button>
                <button ref="mcpNavBtn" type="button" class="apple-sidebar-nav-button relative flex h-9 w-full items-center rounded-[10px] text-sm transition-colors" :class="view === 'mcp' ? 'bg-[var(--selection-bg)] font-semibold text-accent' : 'font-normal hover:bg-black/5 dark:hover:bg-white/8'" aria-label="MCP 管理" @click="goMcp" @mouseenter="sidebarFlyoutArmed = true">
                  <McpIcon class="h-[18px] w-[18px] shrink-0" />
                  <span class="apple-sidebar-label" :aria-hidden="isSidebarCollapsed">MCP 管理</span>
                  <span v-if="isSidebarCollapsed && sidebarFlyoutArmed" class="apple-sidebar-flyout" aria-hidden="true">MCP 管理</span>
                </button>
              </nav>

              <div class="absolute inset-x-1.5 bottom-4">
                <button ref="settingsNavBtn" type="button" class="apple-sidebar-nav-button relative flex h-9 w-full items-center rounded-[10px] text-sm transition-colors" :class="view === 'settings' ? 'bg-[var(--selection-bg)] font-semibold text-accent' : 'font-normal hover:bg-black/5 dark:hover:bg-white/8'" aria-label="设置" @click="view = 'settings'" @mouseenter="sidebarFlyoutArmed = true">
                  <PhGearSix class="h-[18px] w-[18px] shrink-0" weight="bold" aria-hidden="true" />
                  <span class="apple-sidebar-label" :aria-hidden="isSidebarCollapsed">设置</span>
                  <span v-if="isSidebarCollapsed && sidebarFlyoutArmed" class="apple-sidebar-flyout" aria-hidden="true">设置</span>
                </button>
              </div>
            </aside>

            <main class="min-w-0 flex-1 overflow-y-auto overflow-x-hidden bg-[var(--app-bg)] pt-4 pb-7">
              <template v-if="state">
                <KeepAlive>
                  <ProfilesView v-if="view === 'profiles'" :state="state" :nav-reset="profilesNavReset" @refresh="refresh" />
                  <McpView v-else-if="view === 'mcp'" :nav-reset="mcpNavReset" />
                  <SettingsView v-else :state="state" @preview-theme="previewTheme" @refresh="refresh" @saved="saveSettings" @home="goProfiles" />
                </KeepAlive>
              </template>
              <div v-else class="startup-skeleton" aria-busy="true">
                <div class="startup-skeleton__title" />
                <div class="startup-skeleton__subtitle" />
                <div class="startup-skeleton__panel" />
                <div class="startup-skeleton__heading" />
                <div class="startup-skeleton__list" />
                <p v-if="loadError" class="muted mt-4 text-sm">{{ loadError }}</p>
              </div>
            </main>
            </div>
          </div>
        </n-layout>
      </n-message-provider>
    </n-dialog-provider>
  </n-config-provider>
</template>
