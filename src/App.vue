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
import { themeOverrides } from "./theme";
import type { AppState, Settings } from "./types";
import version from "../VERSION?raw";

type View = "profiles" | "settings";

const view = ref<View>("profiles");
const sidebarCollapsed = ref(false);
const sidebarFlyoutArmed = ref(true);
const profilesNavBtn = ref<HTMLElement | null>(null);
const settingsNavBtn = ref<HTMLElement | null>(null);
const indicatorTop = ref(8);
const state = ref<AppState | null>(null);
const loadError = ref("");
const systemDark = ref(window.matchMedia("(prefers-color-scheme: dark)").matches);

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
  if (target) indicatorTop.value = target.offsetTop + 8;
}

watch(view, updateSidebarIndicator);

function toggleSidebar() {
  sidebarCollapsed.value = !sidebarCollapsed.value;
  if (sidebarCollapsed.value) sidebarFlyoutArmed.value = false;
}

async function refresh() {
  try {
    state.value = await api.getState();
    loadError.value = "";
  } catch (error) {
    loadError.value = String(error);
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
  await refresh();
});

onBeforeUnmount(() => {
  media.removeEventListener("change", mediaListener);
});
</script>

<template>
  <n-config-provider :theme="naiveTheme" :theme-overrides="themeOverrides" inline-theme-disabled>
    <n-dialog-provider>
      <n-message-provider>
        <n-layout class="h-full! rounded-none! bg-transparent!">
          <div class="flex min-h-screen">
            <aside class="apple-sidebar relative min-h-screen shrink-0" :class="isSidebarCollapsed ? ['w-[60px]', 'apple-sidebar--collapsed'] : 'w-[160px]'">
              <div class="apple-sidebar-brand mx-3 mt-3 flex items-center gap-3">
                <img src="/logo.png" alt="SwitchGPT" class="h-9 w-9 shrink-0" />
                <div class="apple-sidebar-label" :aria-hidden="isSidebarCollapsed">
                  <div class="text-sm font-bold">SwitchGPT</div>
                  <div class="app-version" :aria-label="`版本 ${version.trim()}`">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true">
                      <path d="M4.5 5.5h8.3l6.7 6.7-6.7 6.7H4.5z" stroke-linejoin="round" />
                      <circle cx="8.5" cy="9.5" r="1" fill="currentColor" stroke="none" />
                    </svg>
                    <span>v{{ version.trim() }}</span>
                  </div>
                </div>
              </div>

              <nav class="relative mx-2 mt-6 space-y-1">
                <span class="apple-sidebar-indicator" :style="{ top: `${indicatorTop}px` }" aria-hidden="true" />
                <button ref="profilesNavBtn" type="button" class="apple-sidebar-nav-button relative flex h-9 w-full items-center rounded-[10px] text-sm transition-colors" :class="view === 'profiles' ? 'bg-[var(--selection-bg)] font-semibold text-[#007aff]' : 'font-medium hover:bg-black/5 dark:hover:bg-white/8'" aria-label="配置档案" @click="view = 'profiles'" @mouseenter="sidebarFlyoutArmed = true">
                  <svg class="h-[18px] w-[18px] shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true">
                    <rect x="4.5" y="4.5" width="15" height="15" rx="3" />
                    <path d="M8.5 9h7M8.5 12h7M8.5 15h4" stroke-linecap="round" />
                  </svg>
                  <span class="apple-sidebar-label" :aria-hidden="isSidebarCollapsed">配置档案</span>
                  <span v-if="isSidebarCollapsed && sidebarFlyoutArmed" class="apple-sidebar-flyout" aria-hidden="true">配置档案</span>
                </button>
                <button ref="settingsNavBtn" type="button" class="apple-sidebar-nav-button relative flex h-9 w-full items-center rounded-[10px] text-sm transition-colors" :class="view === 'settings' ? 'bg-[var(--selection-bg)] font-semibold text-[#007aff]' : 'font-medium hover:bg-black/5 dark:hover:bg-white/8'" aria-label="设置" @click="view = 'settings'" @mouseenter="sidebarFlyoutArmed = true">
                  <svg class="h-[18px] w-[18px] shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true">
                    <path d="M5.5 7.5h13M5.5 12h13M5.5 16.5h13" stroke-linecap="round" />
                    <circle cx="9" cy="7.5" r="1.5" fill="var(--panel-bg)" />
                    <circle cx="15" cy="12" r="1.5" fill="var(--panel-bg)" />
                    <circle cx="11" cy="16.5" r="1.5" fill="var(--panel-bg)" />
                  </svg>
                  <span class="apple-sidebar-label" :aria-hidden="isSidebarCollapsed">设置</span>
                  <span v-if="isSidebarCollapsed && sidebarFlyoutArmed" class="apple-sidebar-flyout" aria-hidden="true">设置</span>
                </button>
              </nav>

              <div class="absolute inset-x-2 bottom-4">
                <button type="button" class="apple-sidebar-nav-button flex h-9 w-full items-center rounded-[10px] text-sm font-medium hover:bg-black/5 dark:hover:bg-white/8" :aria-label="isSidebarCollapsed ? '展开侧边栏' : '收缩侧边栏'" @click="toggleSidebar" @mouseenter="sidebarFlyoutArmed = true">
                  <svg class="h-[18px] w-[18px] shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true">
                    <rect x="4.5" y="5.5" width="15" height="13" rx="3" />
                    <path d="M12 5.5v13" stroke-linecap="round" />
                  </svg>
                  <span class="apple-sidebar-label" :aria-hidden="isSidebarCollapsed">收缩侧边栏</span>
                  <span v-if="isSidebarCollapsed && sidebarFlyoutArmed" class="apple-sidebar-flyout" aria-hidden="true">展开侧边栏</span>
                </button>
              </div>
            </aside>

            <main class="min-w-0 flex-1 overflow-auto bg-[var(--app-bg)] px-8 py-7">
              <template v-if="state">
                <ProfilesView v-if="view === 'profiles'" :state="state" @refresh="refresh" />
                <SettingsView v-else :state="state" @preview-theme="previewTheme" @refresh="refresh" @saved="saveSettings" />
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
