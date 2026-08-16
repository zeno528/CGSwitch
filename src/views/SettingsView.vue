<script setup lang="ts">
import { onMounted, reactive, ref, watch } from "vue";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import {
  NButton,
  NDivider,
  NForm,
  NFormItem,
  NInput,
  NInputNumber,
  NList,
  NListItem,
  NSwitch,
  useDialog,
  useMessage,
} from "naive-ui";
import { api, isTauri } from "../api";
import ChatGPTAccount from "../components/ChatGPTAccount.vue";
import type { AppState, DatabaseBackupInfo, PathInfo, Settings } from "../types";

const props = defineProps<{ state: AppState }>();
const emit = defineEmits<{ refresh: []; saved: [settings: Settings]; previewTheme: [theme: Settings["theme"]]; home: [] }>();
const message = useMessage();
const dialog = useDialog();

const form = reactive<Settings>({ ...props.state.settings });
const saving = ref(false);
const savingGeneral = ref(false);
const openingPath = ref<string | null>(null);
const section = ref<"general" | "codex" | "account" | "about" | "advanced">("general");
const tabBar = ref<HTMLElement | null>(null);
const indicatorLeft = ref("0px");
const indicatorWidth = ref("0px");
const backups = ref<DatabaseBackupInfo[]>([]);
const exporting = ref(false);
const importing = ref(false);
const themeOptions: { label: string; value: Settings["theme"] }[] = [
  { label: "浅色", value: "light" },
  { label: "深色", value: "dark" },
  { label: "跟随系统", value: "system" },
];

async function loadBackups() {
  try {
    backups.value = await api.listDatabaseBackups();
  } catch {
    backups.value = [];
  }
}

async function loadSettings() {
  try {
    const settings = await api.getSettings();
    Object.assign(form, settings);
    emit("refresh");
  } catch (error) {
    message.error(String(error));
  }
}

async function exportBackupToFile() {
  if (exporting.value) return;
  try {
    let target: string | null = null;
    if (isTauri) {
      const picked = await saveDialog({
        title: "导出数据库备份",
        defaultPath: `cgswitch-export-${Date.now()}.db`,
        filters: [{ name: "SQLite 数据库", extensions: ["db"] }],
      });
      target = typeof picked === "string" ? picked : null;
    }
    if (!target && isTauri) return;
    exporting.value = true;
    const path = isTauri
      ? await api.exportDatabaseTo(target!)
      : await api.exportDatabase();
    message.success(`数据库已导出：${path}`);
    await loadBackups();
  } catch (error) {
    message.error(String(error));
  } finally {
    exporting.value = false;
  }
}

async function importBackupFromFile() {
  if (importing.value) return;
  try {
    let picked: string | null = null;
    if (isTauri) {
      const result = await openDialog({
        title: "选择数据库备份",
        multiple: false,
        filters: [{ name: "SQLite 数据库", extensions: ["db"] }],
      });
      picked = typeof result === "string" ? result : null;
    }
    if (!picked && isTauri) return;
    importing.value = true;
    await api.importDatabase(picked ?? "mock-backup.db");
    message.success("数据库已导入并恢复");
    emit("refresh");
    await loadBackups();
  } catch (error) {
    message.error(String(error));
  } finally {
    importing.value = false;
  }
}

function restoreBackup(backup: DatabaseBackupInfo) {
  dialog.warning({
    title: "恢复数据库备份",
    content: `确定用「${backup.name}」覆盖当前所有供应商数据吗？恢复后无法撤销。`,
    positiveText: "恢复",
    negativeText: "取消",
    positiveButtonProps: { type: "error" },
    onPositiveClick: async () => {
      try {
        await api.restoreDatabase(backup.name);
        message.success("数据库已恢复");
        emit("refresh");
        await loadBackups();
      } catch (error) {
        message.error(String(error));
      }
    },
  });
}

function deleteBackup(backup: DatabaseBackupInfo) {
  dialog.warning({
    title: "删除数据库备份",
    content: `确定删除「${backup.name}」吗？删除后不可恢复。`,
    positiveText: "删除",
    negativeText: "取消",
    positiveButtonProps: { type: "error" },
    onPositiveClick: async () => {
      try {
        await api.deleteDatabaseBackup(backup.name);
        message.success("备份已删除");
        await loadBackups();
      } catch (error) {
        message.error(String(error));
      }
    },
  });
}

function formatSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(2)} MB`;
}

function openBackupFolder() {
  const item = props.state.paths.find((path) => path.label === "数据库备份");
  if (!item) {
    message.warning("找不到备份目录");
    return;
  }
  api.openPath(item.path).catch((error) => message.error(String(error)));
}

function updateTabIndicator() {
  const bar = tabBar.value;
  if (!bar) return;
  const button = bar.querySelector<HTMLElement>(`[data-section="${section.value}"]`);
  if (!button) return;
  indicatorLeft.value = `${button.offsetLeft}px`;
  indicatorWidth.value = `${button.offsetWidth}px`;
}

watch(section, updateTabIndicator);
onMounted(async () => {
  await loadSettings();
  loadBackups();
  updateTabIndicator();
});

async function save() {
  if (saving.value) return;
  saving.value = true;
  try {
    const settings = await api.saveSettings({ ...form });
    message.success("设置已保存");
    emit("saved", settings);
    emit("refresh");
  } catch (error) {
    message.error(String(error));
  } finally {
    saving.value = false;
  }
}

async function saveGeneral() {
  if (savingGeneral.value) return;
  const previous = props.state.settings;
  savingGeneral.value = true;
  try {
    const settings = await api.saveSettings({
      ...previous,
      theme: form.theme,
      auto_restart: form.auto_restart,
      autostart_enabled: form.autostart_enabled,
      silent_start: form.silent_start,
      minimize_to_tray: form.minimize_to_tray,
    });
    emit("saved", settings);
  } catch (error) {
    form.theme = previous.theme;
    form.auto_restart = previous.auto_restart;
    form.autostart_enabled = previous.autostart_enabled;
    form.silent_start = previous.silent_start;
    form.minimize_to_tray = previous.minimize_to_tray;
    emit("previewTheme", previous.theme);
    message.error(String(error));
  } finally {
    savingGeneral.value = false;
  }
}

function updateTheme(theme: Settings["theme"]) {
  form.theme = theme;
  emit("previewTheme", theme);
  void saveGeneral();
}

function updateAutoRestart(autoRestart: boolean) {
  form.auto_restart = autoRestart;
  void saveGeneral();
}

function updateStartupToggle(key: "autostart_enabled" | "silent_start" | "minimize_to_tray", value: boolean) {
  form[key] = value;
  void saveGeneral();
}

async function openPath(item: PathInfo) {
  if (openingPath.value) return;
  openingPath.value = item.path;
  try {
    await api.openPath(item.path);
  } catch (error) {
    message.error(String(error));
  } finally {
    openingPath.value = null;
  }
}
</script>

<template>
  <section class="mx-auto w-full max-w-none">
    <div class="apple-page-bar sticky top-[-16px] z-10">
      <button
        type="button"
        class="apple-page-header apple-back-button"
        aria-label="返回首页"
        @click="emit('home')"
      >
        <svg class="h-4 w-4 shrink-0 text-[#007aff]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
          <path d="M15 5.5 8.5 12l6.5 6.5" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
        <span class="apple-title">设置</span>
      </button>
    </div>

    <div ref="tabBar" class="relative mt-[var(--gap-page)] flex items-center gap-1 border-b border-[var(--panel-border)]" aria-label="设置分区">
      <span
        class="settings-tab-indicator absolute -bottom-px h-0.5 rounded-full bg-[#007aff]"
        :style="{ left: indicatorLeft, width: indicatorWidth }"
        aria-hidden="true"
      />
      <button
        type="button"
        data-section="general"
        class="relative flex h-10 items-center gap-1.5 rounded-md px-3 text-sm transition-colors"
        :class="section === 'general' ? 'font-semibold text-[#007aff]' : 'font-medium text-[var(--text-secondary)] hover:text-[#007aff]'"
        :aria-current="section === 'general' ? 'page' : undefined"
        @click="section = 'general'"
      >
        <svg class="h-4 w-4 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M21 4h-7M10 4H3M21 12h-9M8 12H3M21 20h-5M12 20H3" />
          <circle cx="14" cy="4" r="2" />
          <circle cx="8" cy="12" r="2" />
          <circle cx="16" cy="20" r="2" />
        </svg>
        <span>通用</span>
      </button>
      <button
        type="button"
        data-section="codex"
        class="relative flex h-10 items-center gap-1.5 rounded-md px-3 text-sm transition-colors"
        :class="section === 'codex' ? 'font-semibold text-[#007aff]' : 'font-medium text-[var(--text-secondary)] hover:text-[#007aff]'"
        :aria-current="section === 'codex' ? 'page' : undefined"
        @click="section = 'codex'"
      >
        <svg class="h-4 w-4 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="m5 8 4 4-4 4" />
          <path d="M11 16h8" />
        </svg>
        <span>应用</span>
      </button>
      <button
        type="button"
        data-section="account"
        class="relative flex h-10 items-center gap-1.5 rounded-md px-3 text-sm transition-colors"
        :class="section === 'account' ? 'font-semibold text-[#007aff]' : 'font-medium text-[var(--text-secondary)] hover:text-[#007aff]'"
        :aria-current="section === 'account' ? 'page' : undefined"
        @click="section = 'account'"
      >
        <svg class="h-4 w-4 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <circle cx="12" cy="8" r="4" />
          <path d="M4.5 20a7.5 7.5 0 0 1 15 0" />
        </svg>
        <span>账号</span>
      </button>
      <button
        type="button"
        data-section="advanced"
        class="relative flex h-10 items-center gap-1.5 rounded-md px-3 text-sm transition-colors"
        :class="section === 'advanced' ? 'font-semibold text-[#007aff]' : 'font-medium text-[var(--text-secondary)] hover:text-[#007aff]'"
        :aria-current="section === 'advanced' ? 'page' : undefined"
        @click="section = 'advanced'"
      >
        <svg class="h-4 w-4 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <ellipse cx="12" cy="5.5" rx="7.5" ry="3" />
          <path d="M4.5 5.5v13c0 1.66 3.36 3 7.5 3s7.5-1.34 7.5-3v-13" />
          <path d="M4.5 12c0 1.66 3.36 3 7.5 3s7.5-1.34 7.5-3" />
        </svg>
        <span>高级</span>
      </button>
      <button
        type="button"
        data-section="about"
        class="relative flex h-10 items-center gap-1.5 rounded-md px-3 text-sm transition-colors"
        :class="section === 'about' ? 'font-semibold text-[#007aff]' : 'font-medium text-[var(--text-secondary)] hover:text-[#007aff]'"
        :aria-current="section === 'about' ? 'page' : undefined"
        @click="section = 'about'"
      >
        <svg class="h-4 w-4 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <circle cx="12" cy="12" r="8.5" />
          <path d="M12 11.2v4.6" />
          <path d="M12 7.8h.01" />
        </svg>
        <span>关于</span>
      </button>
    </div>

    <div v-if="section === 'general'" class="apple-group mt-[var(--gap-section)] p-[var(--gap-card)]">
      <div class="field-subtitle mb-2">主题</div>
      <div class="apple-group inline-flex gap-1 p-1">
        <button
          v-for="option in themeOptions"
          :key="option.value"
          type="button"
          class="inline-flex h-9 w-28 items-center justify-center gap-1.5 rounded-xl text-sm transition-colors"
          :class="form.theme === option.value ? 'bg-[var(--selection-bg)] font-semibold text-[#007aff]' : 'font-medium hover:bg-black/5 dark:hover:bg-white/8'"
          :aria-pressed="form.theme === option.value"
          @click="updateTheme(option.value)"
        >
          <svg class="h-4 w-4 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true">
            <template v-if="option.value === 'system'">
              <rect x="3.5" y="5" width="17" height="11" rx="2" />
              <path d="M9.5 19.5h5M12 16v3.5" stroke-linecap="round" />
            </template>
            <template v-else-if="option.value === 'light'">
              <circle cx="12" cy="12" r="4" />
              <path d="M12 3.5v1.8M12 18.7v1.8M3.5 12h1.8M18.7 12h1.8M6 6l1.3 1.3M16.7 16.7 18 18M18 6l-1.3 1.3M7.3 16.7 6 18" stroke-linecap="round" />
            </template>
            <template v-else>
              <path d="M20 13.6A8.2 8.2 0 1 1 10.4 4a6.4 6.4 0 0 0 9.6 9.6Z" stroke-linejoin="round" />
            </template>
          </svg>
          <span>{{ option.label }}</span>
        </button>
      </div>
      <n-divider :style="{ marginTop: '16px' }" />
      <div class="flex flex-col gap-5">
        <div class="flex items-center justify-between gap-4">
          <div class="flex items-start gap-3">
            <svg class="mt-0.5 h-4 w-4 shrink-0 text-[#007aff]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true">
              <path d="M12 3.5v7" stroke-linecap="round" />
              <path d="M7.2 6.2a7.5 7.5 0 1 0 9.6 0" stroke-linecap="round" />
            </svg>
            <div>
              <div class="text-sm font-semibold">开机自启</div>
              <div class="muted mt-0.5 text-xs">登录系统后自动启动 CGSwitch</div>
            </div>
          </div>
          <n-switch
            v-model:value="form.autostart_enabled"
            @update:value="updateStartupToggle('autostart_enabled', $event)"
          />
        </div>
        <div class="flex items-center justify-between gap-4">
          <div class="flex items-start gap-3">
            <svg class="mt-0.5 h-4 w-4 shrink-0 text-[#007aff]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true">
              <path d="M20 13.6A8.2 8.2 0 1 1 10.4 4a6.4 6.4 0 0 0 9.6 9.6Z" stroke-linejoin="round" />
            </svg>
            <div>
              <div class="text-sm font-semibold">静默启动</div>
              <div class="muted mt-0.5 text-xs">启动时不显示主窗口，驻留系统托盘</div>
            </div>
          </div>
          <n-switch
            v-model:value="form.silent_start"
            @update:value="updateStartupToggle('silent_start', $event)"
          />
        </div>
        <div class="flex items-center justify-between gap-4">
          <div class="flex items-start gap-3">
            <svg class="mt-0.5 h-4 w-4 shrink-0 text-[#007aff]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true">
              <path d="M4 14v2a3 3 0 0 0 3 3h10a3 3 0 0 0 3-3v-2" stroke-linecap="round" />
              <path d="M12 3.5v8" stroke-linecap="round" />
              <path d="m8.5 8.5 3.5 3.5 3.5-3.5" stroke-linecap="round" stroke-linejoin="round" />
            </svg>
            <div>
              <div class="text-sm font-semibold">关闭时最小化到托盘</div>
              <div class="muted mt-0.5 text-xs">点击关闭按钮时隐藏到托盘而不是退出</div>
            </div>
          </div>
          <n-switch
            v-model:value="form.minimize_to_tray"
            @update:value="updateStartupToggle('minimize_to_tray', $event)"
          />
        </div>
      </div>
    </div>

    <div v-else-if="section === 'advanced'" class="apple-group mt-[var(--gap-section)] p-[var(--gap-card)]">
      <div class="flex items-center justify-between gap-4">
        <div>
          <div class="text-sm font-semibold">数据备份</div>
          <div class="muted mt-0.5 text-xs">导入/导出供应商数据库</div>
        </div>
        <div class="flex gap-2">
          <n-button size="small" secondary :loading="importing" @click="importBackupFromFile">导入备份</n-button>
          <n-button size="small" secondary :loading="exporting" @click="exportBackupToFile">导出备份</n-button>
          <n-button size="small" secondary @click="openBackupFolder">打开备份文件夹</n-button>
        </div>
      </div>
      <div v-if="backups.length" class="mt-3 space-y-2">
        <div
          v-for="backup in backups"
          :key="backup.name"
          class="flex items-center justify-between gap-3 rounded-lg border border-[var(--panel-border)] px-3 py-2"
        >
          <div class="min-w-0">
            <div class="mono truncate text-xs font-medium">{{ backup.name }}</div>
            <div class="muted text-xs">{{ formatSize(backup.size_bytes) }}</div>
          </div>
          <div class="flex shrink-0 gap-1.5">
            <n-button size="tiny" secondary type="primary" @click="restoreBackup(backup)">恢复</n-button>
            <n-button size="tiny" quaternary type="error" @click="deleteBackup(backup)">删除</n-button>
          </div>
        </div>
      </div>
      <p v-else class="muted mt-3 text-xs">还没有导出过备份。</p>
    </div>

    <div v-else-if="section === 'codex'" class="apple-group mt-[var(--gap-section)] p-[var(--gap-card)]">
      <n-form label-placement="top">
        <n-form-item label="Codex / ChatGPT 应用路径覆盖">
          <n-input v-model:value="form.codex_app_path" clearable placeholder="留空使用自动识别" />
        </n-form-item>
        <n-form-item label="应用配置后自动重启 Codex">
          <div class="flex items-center gap-3">
            <n-switch v-model:value="form.auto_restart" @update:value="updateAutoRestart" />
            <span class="muted text-sm">关闭时仅写入 config.toml，由你手动点击重启。</span>
          </div>
        </n-form-item>
        <n-form-item label="重启等待超时（毫秒）">
          <n-input-number v-model:value="form.restart_timeout_ms" :min="1000" :max="60000" :step="500" class="w-full" />
        </n-form-item>
        <div class="flex justify-end">
          <n-button type="primary" :loading="saving" @click="save">保存设置</n-button>
        </div>
      </n-form>
    </div>

    <div v-else-if="section === 'account'" class="apple-group mt-[var(--gap-section)] p-[var(--gap-card)]">
      <h2 class="text-[15px] font-semibold tracking-tight">ChatGPT 账号</h2>
      <div class="mt-4">
        <ChatGPTAccount />
      </div>
    </div>

    <div v-else class="apple-group mt-[var(--gap-section)] p-[var(--gap-card)]">
      <h2 class="text-[15px] font-semibold tracking-tight">数据与路径</h2>
      <p class="muted mt-2 text-sm">所有本机数据固定保存在用户 Home 目录，不会进入 Git。</p>
      <n-divider />
      <n-list class="bg-transparent" :show-divider="true">
        <n-list-item v-for="item in state.paths" :key="item.label">
          <div class="flex items-center justify-between gap-4">
            <div class="min-w-0">
              <div class="text-sm font-semibold">{{ item.label }}</div>
              <div class="mono muted mt-1 break-all text-xs">{{ item.path }}</div>
            </div>
            <n-button size="small" secondary :loading="openingPath === item.path" :disabled="Boolean(openingPath)" title="在资源管理器中打开" @click="openPath(item)">
              <template #icon>
                <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true">
                  <path d="M3.75 7.75A2.75 2.75 0 0 1 6.5 5h3l1.7 2h6.05A2.75 2.75 0 0 1 20 9.75v7.75a2.75 2.75 0 0 1-2.75 2.75h-10.5A2.75 2.75 0 0 1 4 17.5V9.75" stroke-linejoin="round" />
                </svg>
              </template>
              打开
            </n-button>
          </div>
        </n-list-item>
      </n-list>
    </div>
  </section>
</template>
