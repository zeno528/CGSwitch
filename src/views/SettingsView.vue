<script setup lang="ts">
import { h, onMounted, reactive, ref, watch } from "vue";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  NButton,
  NDivider,
  NInput,
  NList,
  NListItem,
  NModal,
  NSelect,
  NSwitch,
  useDialog,
  useMessage,
} from "naive-ui";
import { api, isTauri } from "../api";
import ChatGPTAccount from "../components/ChatGPTAccount.vue";
import ProfileIconTile from "../components/ProfileIconTile.vue";
import TrashIcon from "../components/TrashIcon.vue";
import type { AppState, DatabaseBackupInfo, PathInfo, Settings } from "../types";
import {
  PhArrowLeft,
  PhArrowClockwise,
  PhDatabase,
  PhDownloadSimple,
  PhFloppyDisk,
  PhFolderOpen,
  PhInfo,
  PhMoon,
  PhMoonStars,
  PhPencilSimple,
  PhMonitor,
  PhPower,
  PhSlidersHorizontal,
  PhSun,
  PhTerminalWindow,
  PhTrayArrowDown,
  PhUploadSimple,
  PhUserCircle,
} from "@phosphor-icons/vue";
import version from "../../VERSION?raw";

const props = defineProps<{ state: AppState }>();
const emit = defineEmits<{ refresh: []; saved: [settings: Settings]; previewTheme: [theme: Settings["theme"]]; home: [] }>();
const message = useMessage();
const dialog = useDialog();

const form = reactive<Settings>({ ...props.state.settings });
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
  return name.replace(/^(?:cg-backup-|cgswitch-export-)/, "").replace(/\.db$/, "");
}

const backupFileActions = [
  {
    key: "import",
    label: "导入数据库",
    color: "var(--accent)",
    disabled: () => importing.value,
    run: () => importBackupFromFile(),
  },
  {
    key: "export",
    label: "导出数据库",
    color: "#34c759",
    disabled: () => exporting.value,
    run: () => exportBackupToFile(),
  },
];
const backupUtilityActions = [
  {
    key: "immediate",
    label: "立即备份",
    color: "var(--accent)",
    disabled: () => exporting.value,
    run: () => createImmediateBackup(),
  },
  {
    key: "folder",
    label: "备份文件夹",
    color: "#ff9500",
    disabled: () => false,
    run: () => openBackupFolder(),
  },
];
const themeOptions: { label: string; value: Settings["theme"] }[] = [
  { label: "浅色", value: "light" },
  { label: "深色", value: "dark" },
  { label: "跟随系统", value: "system" },
];
const autoBackupIntervalOptions = [
  { label: "关闭", value: 0 },
  { label: "6 小时", value: 6 },
  { label: "12 小时", value: 12 },
  { label: "24 小时", value: 24 },
  { label: "48 小时", value: 48 },
  { label: "7 天", value: 168 },
];
const backupKeepCountOptions = [3, 5, 10, 15, 20, 30].map((value) => ({
  label: `${value} 个`,
  value,
}));

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
  let directory: string | null = null;
  try {
    if (isTauri) {
      const result = await openDialog({
        title: "选择导出目录",
        directory: true,
        multiple: false,
      });
      directory = typeof result === "string" ? result : null;
      if (!directory) return;
    }
    const path = await api.exportDatabaseTo(directory ?? "mock-export");
    message.success(`数据文件已导出：${path}`);
  } catch (error) {
    message.error(String(error));
  } finally {
    exporting.value = false;
  }
}

async function createImmediateBackup() {
  if (exporting.value) return;
  exporting.value = true;
  try {
    await api.exportDatabase();
    message.success("已创建内部备份");
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
        title: "选择数据库文件",
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
    content: () =>
      h("span", [
        "确定删除「",
        h("strong", { class: "font-semibold" }, backup.name),
        "」吗？删除后不可恢复。",
      ]),
    positiveText: "删除",
    negativeText: "取消",
    icon: () => h(TrashIcon),
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
      auto_backup_interval_hours: form.auto_backup_interval_hours,
      database_backup_keep_count: form.database_backup_keep_count,
    });
    emit("saved", settings);
  } catch (error) {
    form.theme = previous.theme;
    form.auto_restart = previous.auto_restart;
    form.autostart_enabled = previous.autostart_enabled;
    form.silent_start = previous.silent_start;
    form.minimize_to_tray = previous.minimize_to_tray;
    form.auto_backup_interval_hours = previous.auto_backup_interval_hours;
    form.database_backup_keep_count = previous.database_backup_keep_count;
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
  <section class="settings-page mx-auto w-full max-w-none">
    <div class="apple-page-bar sticky top-[-16px] z-10">
      <button
        type="button"
        class="apple-page-header apple-back-button"
        aria-label="返回首页"
        @click="emit('home')"
      >
        <PhArrowLeft class="h-4 w-4 shrink-0 text-accent" weight="bold" aria-hidden="true" />
        <span class="apple-title">设置</span>
      </button>
    </div>

    <div ref="tabBar" class="relative mt-2 flex items-center gap-1 border-b border-[var(--panel-border)]" aria-label="设置分区">
      <span
        class="settings-tab-indicator absolute -bottom-px h-0.5 rounded-full bg-accent"
        :style="{ left: indicatorLeft, width: indicatorWidth }"
        aria-hidden="true"
      />
      <button
        type="button"
        data-section="general"
        class="settings-tab relative flex h-10 items-center gap-1.5 rounded-md px-3 transition-colors"
        :class="section === 'general' ? 'text-accent' : 'text-[var(--text-secondary)] hover:text-accent'"
        :aria-current="section === 'general' ? 'page' : undefined"
        @click="section = 'general'"
      >
        <PhSlidersHorizontal class="h-4 w-4 shrink-0" weight="bold" aria-hidden="true" />
        <span>通用</span>
      </button>
      <button
        type="button"
        data-section="codex"
        class="settings-tab relative flex h-10 items-center gap-1.5 rounded-md px-3 transition-colors"
        :class="section === 'codex' ? 'text-accent' : 'text-[var(--text-secondary)] hover:text-accent'"
        :aria-current="section === 'codex' ? 'page' : undefined"
        @click="section = 'codex'"
      >
        <PhTerminalWindow class="h-4 w-4 shrink-0" weight="bold" aria-hidden="true" />
        <span>应用</span>
      </button>
      <button
        type="button"
        data-section="account"
        class="settings-tab relative flex h-10 items-center gap-1.5 rounded-md px-3 transition-colors"
        :class="section === 'account' ? 'text-accent' : 'text-[var(--text-secondary)] hover:text-accent'"
        :aria-current="section === 'account' ? 'page' : undefined"
        @click="section = 'account'"
      >
        <PhUserCircle class="h-4 w-4 shrink-0" weight="bold" aria-hidden="true" />
        <span>账号</span>
      </button>
      <button
        type="button"
        data-section="advanced"
        class="settings-tab relative flex h-10 items-center gap-1.5 rounded-md px-3 transition-colors"
        :class="section === 'advanced' ? 'text-accent' : 'text-[var(--text-secondary)] hover:text-accent'"
        :aria-current="section === 'advanced' ? 'page' : undefined"
        @click="section = 'advanced'"
      >
        <PhDatabase class="h-4 w-4 shrink-0" weight="bold" aria-hidden="true" />
        <span>高级</span>
      </button>
      <button
        type="button"
        data-section="about"
        class="settings-tab relative flex h-10 items-center gap-1.5 rounded-md px-3 transition-colors"
        :class="section === 'about' ? 'text-accent' : 'text-[var(--text-secondary)] hover:text-accent'"
        :aria-current="section === 'about' ? 'page' : undefined"
        @click="section = 'about'"
      >
        <PhInfo class="h-4 w-4 shrink-0" weight="bold" aria-hidden="true" />
        <span>关于</span>
      </button>
    </div>

    <div v-if="section === 'general'" class="apple-group mt-[var(--gap-section)] p-[var(--gap-card)]">
      <div class="setting-title mb-2">主题</div>
      <div class="apple-group inline-flex gap-1 p-1">
        <button
          v-for="option in themeOptions"
          :key="option.value"
          type="button"
          class="inline-flex h-9 w-28 items-center justify-center gap-1.5 rounded-xl text-sm transition-colors"
          :class="form.theme === option.value ? 'bg-[var(--selection-bg)] font-semibold text-accent' : 'font-medium hover:bg-black/5 dark:hover:bg-white/8'"
          :aria-pressed="form.theme === option.value"
          @click="updateTheme(option.value)"
        >
          <PhMonitor v-if="option.value === 'system'" class="h-4 w-4 shrink-0" weight="bold" aria-hidden="true" />
          <PhSun v-else-if="option.value === 'light'" class="h-4 w-4 shrink-0" weight="bold" aria-hidden="true" />
          <PhMoon v-else class="h-4 w-4 shrink-0" weight="bold" aria-hidden="true" />
          <span>{{ option.label }}</span>
        </button>
      </div>
      <n-divider :style="{ marginTop: '16px' }" />
      <div class="flex flex-col gap-5">
        <div class="flex items-center justify-between gap-4">
          <div class="flex items-start gap-3">
            <span class="settings-icon-tile grid h-9 w-9 shrink-0 place-items-center rounded-xl text-[#007aff]">
              <PhPower class="h-[18px] w-[18px]" weight="bold" aria-hidden="true" />
            </span>
            <div>
              <div class="setting-title">开机自启</div>
              <div class="setting-description mt-0.5">登录系统后自动启动 CGswitch</div>
            </div>
          </div>
          <n-switch
            v-model:value="form.autostart_enabled"
            @update:value="updateStartupToggle('autostart_enabled', $event)"
          />
        </div>
        <div class="flex items-center justify-between gap-4">
          <div class="flex items-start gap-3">
            <span class="settings-icon-tile grid h-9 w-9 shrink-0 place-items-center rounded-xl text-[#af52de]">
              <PhMoonStars class="h-[18px] w-[18px]" weight="bold" aria-hidden="true" />
            </span>
            <div>
              <div class="setting-title">静默启动</div>
              <div class="setting-description mt-0.5">启动时不显示主窗口，驻留系统托盘</div>
            </div>
          </div>
          <n-switch
            v-model:value="form.silent_start"
            @update:value="updateStartupToggle('silent_start', $event)"
          />
        </div>
        <div class="flex items-center justify-between gap-4">
          <div class="flex items-start gap-3">
            <span class="settings-icon-tile grid h-9 w-9 shrink-0 place-items-center rounded-xl text-[#ff9500]">
              <PhTrayArrowDown class="h-[18px] w-[18px]" weight="bold" aria-hidden="true" />
            </span>
            <div>
              <div class="setting-title">关闭时最小化到托盘</div>
              <div class="setting-description mt-0.5">点击关闭按钮时隐藏到托盘而不是退出</div>
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
        <span class="settings-icon-tile grid h-9 w-9 shrink-0 place-items-center rounded-xl text-accent">
          <PhDatabase class="h-[18px] w-[18px]" weight="bold" aria-hidden="true" />
        </span>
        <div>
          <div class="setting-title">数据备份</div>
          <div class="setting-description mt-0.5">管理本地数据库备份，支持导入、导出和自动备份</div>
        </div>
      </div>

      <div class="mt-4 grid grid-cols-2 gap-2">
        <button
          v-for="action in backupFileActions"
          :key="action.key"
          type="button"
          class="inline-flex h-9 w-full items-center justify-center gap-2 rounded-xl border border-[var(--panel-ring)] px-3 text-sm font-medium transition-colors hover:bg-[var(--sidebar-bg)] disabled:cursor-not-allowed disabled:opacity-50"
          :disabled="action.disabled()"
          @click="action.run()"
        >
          <PhDownloadSimple v-if="action.key === 'import'" class="h-4 w-4" :style="{ color: action.color }" weight="bold" aria-hidden="true" />
          <PhUploadSimple v-else class="h-4 w-4" :style="{ color: action.color }" weight="bold" aria-hidden="true" />
          {{ action.label }}
        </button>
      </div>

      <div class="mt-[var(--gap-section)] rounded-2xl bg-[color-mix(in_srgb,var(--sidebar-bg)_30%,var(--panel-bg))] p-3.5 shadow-[0_0_0_1px_var(--panel-ring)]">
        <div class="text-[15px] font-semibold tracking-tight">自动备份</div>
        <div class="mt-3 grid gap-3 sm:grid-cols-2">
          <div>
            <div class="field-label muted mb-1.5">备份间隔</div>
            <n-select
              v-model:value="form.auto_backup_interval_hours"
              :options="autoBackupIntervalOptions"
              @update:value="saveGeneral"
            />
          </div>
          <div>
            <div class="field-label muted mb-1.5">最多保留</div>
            <n-select
              v-model:value="form.database_backup_keep_count"
              :options="backupKeepCountOptions"
              @update:value="saveGeneral"
            />
          </div>
        </div>
        <div class="mt-3 grid grid-cols-2 gap-2">
          <button
            v-for="action in backupUtilityActions"
            :key="action.key"
            type="button"
            class="inline-flex h-9 w-full items-center justify-center gap-2 rounded-xl border border-[var(--panel-ring)] px-3 text-sm font-medium transition-colors hover:bg-[var(--sidebar-bg)] disabled:cursor-not-allowed disabled:opacity-50"
            :disabled="action.disabled()"
            @click="action.run()"
          >
            <PhFloppyDisk v-if="action.key === 'immediate'" class="h-4 w-4" :style="{ color: action.color }" weight="bold" aria-hidden="true" />
            <PhFolderOpen v-else class="h-4 w-4" :style="{ color: action.color }" weight="bold" aria-hidden="true" />
            {{ action.label }}
          </button>
        </div>
      </div>

      <n-divider :style="{ marginTop: '16px' }" />
      <div class="setting-title mb-2">备份记录</div>
      <div v-if="backups.length" class="space-y-2">
        <div
          v-for="backup in backups"
          :key="backup.name"
          class="flex items-center justify-between gap-3 rounded-xl shadow-[0_0_0_1px_var(--panel-ring)] px-3 py-2.5"
        >
          <div class="flex min-w-0 items-center gap-2.5">
            <span class="settings-icon-tile grid h-8 w-8 shrink-0 place-items-center rounded-lg text-accent">
              <PhDatabase class="h-4 w-4" weight="bold" aria-hidden="true" />
            </span>
            <div class="min-w-0">
              <div class="mono truncate text-xs font-medium">{{ backup.name }}</div>
              <div class="muted text-[11px]">{{ formatTimestamp(backup.created_at) }} · {{ formatSize(backup.size_bytes) }}</div>
            </div>
          </div>
          <div class="flex shrink-0 gap-1.5">
            <button
              type="button"
              class="grid h-8 w-8 place-items-center rounded-lg text-zinc-500 transition-colors hover:bg-[var(--sidebar-bg)] hover:text-accent"
              title="编辑备份名称"
              aria-label="编辑备份名称"
              @click="openRename(backup)"
            >
              <PhPencilSimple class="h-4 w-4" weight="bold" aria-hidden="true" />
            </button>
            <button
              type="button"
              class="grid h-8 w-8 place-items-center rounded-lg text-accent transition-colors hover:bg-accent/10"
              title="恢复数据库"
              aria-label="恢复数据库"
              @click="restoreBackup(backup)"
            >
              <PhArrowClockwise class="h-4 w-4" weight="bold" aria-hidden="true" />
            </button>
            <button
              type="button"
              class="grid h-8 w-8 place-items-center rounded-lg text-[#ff3b30]/70 transition-colors hover:bg-[#ff3b30]/10 hover:text-[#ff3b30]"
              title="删除备份"
              aria-label="删除备份"
              @click="deleteBackup(backup)"
            >
              <TrashIcon />
            </button>
          </div>
        </div>
      </div>
      <div v-else class="setting-description flex items-center gap-2">
        <PhDatabase class="h-4 w-4 shrink-0" weight="bold" aria-hidden="true" />
        还没有导出过备份。
      </div>
    </div>

    <div v-else-if="section === 'codex'" class="apple-group mt-[var(--gap-section)] p-[var(--gap-card)]">
      <div class="flex items-center justify-between gap-4">
        <div>
          <div class="setting-title">应用配置后自动重启 Codex</div>
          <div class="setting-description mt-0.5">开启后应用配置会自动重启 Codex 生效；关闭则只保存配置，稍后可手动重启。</div>
        </div>
        <n-switch v-model:value="form.auto_restart" @update:value="updateAutoRestart" />
      </div>
    </div>

    <div v-else-if="section === 'account'" class="apple-group mt-[var(--gap-section)] p-[var(--gap-card)]">
      <div class="flex items-center gap-3">
        <ProfileIconTile name="ChatGPT" icon="openai-chatgpt" size="sm" />
        <h2 class="text-[15px] font-semibold tracking-tight">ChatGPT 账号</h2>
      </div>
      <div class="mt-4">
        <ChatGPTAccount />
      </div>
    </div>

    <div v-else class="apple-group mt-[var(--gap-section)] p-[var(--gap-card)]">
      <div class="flex items-center gap-3">
              <img src="/logo.svg" alt="CGswitch" class="h-12 w-12 shrink-0 dark:invert" />
        <div>
          <div class="apple-wordmark">CGswitch</div>
          <div class="app-version mt-1.5" :aria-label="`版本 ${version.trim()}`">
            <span>版本 {{ version.trim() }}</span>
          </div>
        </div>
      </div>
      <n-divider :style="{ marginTop: '16px' }" />
      <h2 class="setting-title">数据与路径</h2>
      <p class="setting-description mt-2">应用数据与配置文件位置。</p>
      <n-divider />
      <n-list class="bg-transparent" :show-divider="true">
        <n-list-item v-for="item in state.paths" :key="item.label">
          <div class="flex items-center justify-between gap-4">
            <div class="min-w-0">
              <div class="setting-title">{{ item.label }}</div>
              <div class="mono muted mt-1 break-all text-xs">{{ item.path }}</div>
            </div>
            <n-button size="small" secondary :loading="openingPath === item.path" :disabled="Boolean(openingPath)" title="在资源管理器中打开" @click="openPath(item)">
              <template #icon>
                <PhFolderOpen class="h-4 w-4" weight="bold" aria-hidden="true" />
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
        <div class="dialog-actions flex justify-end gap-2">
          <n-button @click="renameTarget = null">取消</n-button>
          <n-button type="primary" :loading="renaming" :disabled="!renameText.trim()" @click="submitRename">
            保存
          </n-button>
        </div>
      </div>
    </n-modal>
  </section>
</template>
