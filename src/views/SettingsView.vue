<script setup lang="ts">
import { onMounted, reactive, ref, watch } from "vue";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  NButton,
  NDivider,
  NForm,
  NFormItem,
  NInput,
  NList,
  NListItem,
  NModal,
  NSwitch,
  useDialog,
  useMessage,
} from "naive-ui";
import { api, isTauri } from "../api";
import ChatGPTAccount from "../components/ChatGPTAccount.vue";
import type { AppState, DatabaseBackupInfo, PathInfo, Settings } from "../types";
import version from "../../VERSION?raw";

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
const renameTarget = ref<DatabaseBackupInfo | null>(null);
const renameText = ref("");
const renaming = ref(false);

function backupTitle(name: string) {
  return name.replace(/^cgswitch-export-/, "").replace(/\.db$/, "");
}

const backupActions = [
  {
    key: "import",
    label: "导入备份",
    desc: "从备份文件恢复供应商数据库",
    color: "#007aff",
    bgClass: "bg-[#007aff]/10 enabled:hover:bg-[#007aff]/[0.16]",
    disabled: () => importing.value,
    run: () => importBackupFromFile(),
  },
  {
    key: "export",
    label: "导出备份",
    desc: "导出全部供应商配置",
    color: "#34c759",
    bgClass: "bg-[#34c759]/10 enabled:hover:bg-[#34c759]/[0.16]",
    disabled: () => exporting.value,
    run: () => exportBackupToFile(),
  },
  {
    key: "folder",
    label: "打开备份文件夹",
    desc: "查看本地备份文件",
    color: "#ff9500",
    bgClass: "bg-[#ff9500]/10 hover:bg-[#ff9500]/[0.16]",
    disabled: () => false,
    run: () => openBackupFolder(),
  },
];
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
  exporting.value = true;
  try {
    // 导出到预设备份目录，不弹路径选择；导出后备份记录自动刷新可见
    const path = await api.exportDatabase();
    message.success(`数据库已导出：${path}`);
    await loadBackups();
  } catch (error) {
    message.error(String(error));
  } finally {
    exporting.value = false;
  }
}

function openRename(backup: DatabaseBackupInfo) {
  renameTarget.value = backup;
  renameText.value = backupTitle(backup.name);
}

async function submitRename() {
  const target = renameTarget.value;
  const text = renameText.value.trim();
  if (!target || renaming.value) return;
  if (!text || text === backupTitle(target.name)) {
    renameTarget.value = null;
    return;
  }
  renaming.value = true;
  try {
    await api.renameDatabaseBackup(target.name, text);
    message.success("备份已重命名");
    await loadBackups();
    renameTarget.value = null;
  } catch (error) {
    message.error(String(error));
  } finally {
    renaming.value = false;
  }
}

function onRenameShowChange(show: boolean) {
  if (!show) renameTarget.value = null;
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

function formatTimestamp(seconds: number) {
  const date = new Date(seconds * 1000);
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
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
      <div class="flex items-center gap-3">
        <span class="grid h-9 w-9 shrink-0 place-items-center rounded-xl bg-[#007aff]/10 text-[#007aff]">
          <svg class="h-[18px] w-[18px]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" aria-hidden="true">
            <ellipse cx="12" cy="5" rx="8" ry="3" />
            <path d="M4 5v14c0 1.7 3.6 3 8 3s8-1.3 8-3V5" />
            <path d="M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3" />
          </svg>
        </span>
        <div>
          <div class="field-subtitle">数据备份</div>
          <div class="muted mt-0.5 text-xs">导入/导出供应商数据库</div>
        </div>
      </div>

      <div class="mt-4 flex flex-col gap-2">
        <button
          v-for="action in backupActions"
          :key="action.key"
          type="button"
          class="flex w-full items-center gap-3 rounded-xl p-3 text-left transition-colors disabled:cursor-not-allowed disabled:opacity-50"
          :class="action.bgClass"
          :disabled="action.disabled()"
          @click="action.run()"
        >
          <span class="grid h-9 w-9 shrink-0 place-items-center rounded-[10px] text-white" :style="{ backgroundColor: action.color }">
            <svg v-if="action.key === 'import'" class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <path d="M12 3v12" />
              <path d="m7 10 5 5 5-5" />
              <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
            </svg>
            <svg v-else-if="action.key === 'export'" class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <path d="M12 15V3" />
              <path d="m7 8 5-5 5 5" />
              <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
            </svg>
            <svg v-else class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z" />
            </svg>
          </span>
          <span class="min-w-0 flex-1">
            <span class="block text-sm font-semibold">{{ action.label }}</span>
            <span class="muted block truncate text-xs">{{ action.desc }}</span>
          </span>
          <svg class="h-4 w-4 shrink-0" :style="{ color: action.color }" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="m9 6 6 6-6 6" />
          </svg>
        </button>
      </div>

      <n-divider :style="{ marginTop: '16px' }" />
      <div class="field-subtitle mb-2">备份记录</div>
      <div v-if="backups.length" class="space-y-2">
        <div
          v-for="backup in backups"
          :key="backup.name"
          class="flex items-center justify-between gap-3 rounded-xl shadow-[0_0_0_1px_var(--panel-ring)] px-3 py-2.5"
        >
          <div class="flex min-w-0 items-center gap-2.5">
            <span class="grid h-8 w-8 shrink-0 place-items-center rounded-lg bg-[#007aff]/10 text-[#007aff]">
              <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                <rect x="3" y="4" width="18" height="4" rx="1" />
                <path d="M5 8v10a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8" />
                <path d="M10 12h4" />
              </svg>
            </span>
            <div class="min-w-0">
              <div class="mono truncate text-xs font-medium">{{ backup.name }}</div>
              <div class="muted text-[11px]">{{ formatTimestamp(backup.created_at) }} · {{ formatSize(backup.size_bytes) }}</div>
            </div>
          </div>
          <div class="flex shrink-0 gap-1.5">
            <n-button size="tiny" quaternary @click="openRename(backup)">编辑</n-button>
            <n-button size="tiny" secondary type="primary" @click="restoreBackup(backup)">恢复</n-button>
            <n-button size="tiny" quaternary type="error" @click="deleteBackup(backup)">删除</n-button>
          </div>
        </div>
      </div>
      <div v-else class="muted flex items-center gap-2 text-xs">
        <svg class="h-4 w-4 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <rect x="3" y="4" width="18" height="4" rx="1" />
          <path d="M5 8v10a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8" />
          <path d="M10 12h4" />
        </svg>
        还没有导出过备份。
      </div>
    </div>

    <div v-else-if="section === 'codex'" class="apple-group mt-[var(--gap-section)] p-[var(--gap-card)]">
      <n-form label-placement="top">
        <n-form-item label="Codex / ChatGPT 应用路径覆盖">
          <n-input v-model:value="form.codex_app_path" clearable placeholder="留空使用自动识别" />
        </n-form-item>
        <div class="flex items-center justify-between gap-4">
          <div>
            <div class="text-sm font-semibold">应用配置后自动重启 Codex</div>
            <div class="muted mt-0.5 text-xs">开启后应用配置会自动重启 Codex 生效；关闭则只保存配置，稍后可手动重启。</div>
          </div>
          <n-switch v-model:value="form.auto_restart" @update:value="updateAutoRestart" />
        </div>
        <div class="mt-3 flex justify-end">
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
      <div class="flex items-center gap-3">
        <img src="/logo.png" alt="CGSwitch" class="h-12 w-12 shrink-0 rounded-2xl" />
        <div>
          <div class="text-[15px] font-bold tracking-tight">CGSwitch</div>
          <div class="app-version mt-1.5" :aria-label="`版本 ${version.trim()}`">
            <span>v{{ version.trim() }}</span>
          </div>
        </div>
      </div>
      <n-divider :style="{ marginTop: '16px' }" />
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

    <n-modal
      :show="renameTarget !== null"
      preset="card"
      class="max-w-[380px]"
      title="重命名备份"
      @update:show="onRenameShowChange"
    >
      <div class="space-y-4">
        <n-input
          v-model:value="renameText"
          maxlength="80"
          show-count
          placeholder="输入新的备份标题"
          @keyup.enter="submitRename"
        />
        <div class="flex justify-end gap-2">
          <n-button @click="renameTarget = null">取消</n-button>
          <n-button type="primary" :loading="renaming" :disabled="!renameText.trim()" @click="submitRename">
            保存
          </n-button>
        </div>
      </div>
    </n-modal>
  </section>
</template>
