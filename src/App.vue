<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import {
  NConfigProvider,
  NDialogProvider,
  NLayout,
  NMessageProvider,
  darkTheme,
} from "naive-ui";
import ProfilesView from "./views/ProfilesView.vue";
import SettingsView from "./views/SettingsView.vue";
import { api, isTauri } from "./api";
import { useWindowActivation } from "./composables/useWindowActivation";
import { themeOverrides } from "./theme";
import type { AppState, Settings } from "./types";
import version from "../VERSION?raw";

type View = "profiles" | "settings";

const view = ref<View>("profiles");
const profilesNavReset = ref(0);
const sidebarCollapsed = ref(false);
const sidebarFlyoutArmed = ref(true);
const profilesNavBtn = ref<HTMLElement | null>(null);
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

function updateSidebarIndicator() {
  const target = view.value === "profiles" ? profilesNavBtn.value : settingsNavBtn.value;
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
  if (sidebarCollapsed.value) sidebarFlyoutArmed.value = false;
  window.setTimeout(updateSidebarIndicator, 360);
}

// 点击侧边栏“供应商配置”始终回到首页列表（退出编辑等子视图）
function goProfiles() {
  profilesNavReset.value++;
  view.value = "profiles";
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
  async (dark) => {
    try {
      await syncWindowTitleBarTheme(dark);
    } catch {
      // 标题栏同步失败不阻塞内容切换
    }
    document.documentElement.classList.toggle("dark", dark);
    document.documentElement.style.colorScheme = dark ? "dark" : "light";
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
</script>

<template>
  <n-config-provider :theme="naiveTheme" :theme-overrides="themeOverrides" inline-theme-disabled>
    <n-dialog-provider>
      <n-message-provider>
        <n-layout class="h-full! rounded-none! bg-transparent!">
          <div class="flex h-screen">
            <aside class="apple-sidebar relative h-full shrink-0" :class="isSidebarCollapsed ? ['w-[60px]', 'apple-sidebar--collapsed'] : 'w-[144px]'">
              <div
                class="apple-sidebar-brand mx-3 mt-3 flex items-center gap-3"
                role="button"
                tabindex="0"
                @click="toggleSidebar"
                @keyup.enter="toggleSidebar"
              >
                <img src="/logo.png" alt="CGSwitch" class="h-9 w-9 shrink-0" />
                <div class="apple-sidebar-label" :aria-hidden="isSidebarCollapsed">
                  <div class="text-sm font-bold">CGSwitch</div>
                  <div class="app-version" :aria-label="`版本 ${version.trim()}`">
                    <span>v{{ version.trim() }}</span>
                  </div>
                </div>
              </div>

              <nav ref="sidebarNavRef" class="relative mx-2 mt-6 space-y-1">
                <span class="apple-sidebar-indicator" :style="{ top: `${indicatorTop}px`, left: `${indicatorLeft}px` }" aria-hidden="true" />
                <button ref="profilesNavBtn" type="button" class="apple-sidebar-nav-button relative flex h-9 w-full items-center rounded-[10px] text-sm transition-colors" :class="view === 'profiles' ? 'bg-[var(--selection-bg)] font-semibold text-[#007aff]' : 'font-medium hover:bg-black/5 dark:hover:bg-white/8'" aria-label="供应商配置" @click="goProfiles" @mouseenter="sidebarFlyoutArmed = true">
                  <svg class="h-[18px] w-[18px] shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                    <rect x="2" y="3" width="20" height="7" rx="2" />
                    <rect x="2" y="14" width="20" height="7" rx="2" />
                    <path d="M6 6.5h.01M6 17.5h.01" />
                  </svg>
                  <span class="apple-sidebar-label" :aria-hidden="isSidebarCollapsed">供应商配置</span>
                  <span v-if="isSidebarCollapsed && sidebarFlyoutArmed" class="apple-sidebar-flyout" aria-hidden="true">供应商配置</span>
                </button>
              </nav>

              <div class="absolute inset-x-2 bottom-4">
                <button ref="settingsNavBtn" type="button" class="apple-sidebar-nav-button relative flex h-9 w-full items-center rounded-[10px] text-sm transition-colors" :class="view === 'settings' ? 'bg-[var(--selection-bg)] font-semibold text-[#007aff]' : 'font-medium hover:bg-black/5 dark:hover:bg-white/8'" aria-label="设置" @click="view = 'settings'" @mouseenter="sidebarFlyoutArmed = true">
                  <svg class="h-[18px] w-[18px] shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true">
                    <circle cx="12" cy="12" r="3" />
                    <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
                  </svg>
                  <span class="apple-sidebar-label" :aria-hidden="isSidebarCollapsed">设置</span>
                  <span v-if="isSidebarCollapsed && sidebarFlyoutArmed" class="apple-sidebar-flyout" aria-hidden="true">设置</span>
                </button>
              </div>
            </aside>

            <main class="min-w-0 flex-1 overflow-auto bg-[var(--app-bg)] pt-4 pb-7">
              <template v-if="state">
                <ProfilesView v-if="view === 'profiles'" :key="profilesNavReset" :state="state" @refresh="refresh" />
                <SettingsView v-else :state="state" @preview-theme="previewTheme" @refresh="refresh" @saved="saveSettings" @home="goProfiles" />
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
        </n-layout>
      </n-message-provider>
    </n-dialog-provider>
  </n-config-provider>
</template>
