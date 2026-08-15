<script setup lang="ts">
import { reactive, ref } from "vue";
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
  useMessage,
} from "naive-ui";
import { api } from "../api";
import SegmentedControl from "../components/SegmentedControl.vue";
import type { AppState, PathInfo, Settings } from "../types";

const props = defineProps<{ state: AppState }>();
const emit = defineEmits<{ refresh: []; saved: [settings: Settings]; previewTheme: [theme: Settings["theme"]] }>();
const message = useMessage();

const form = reactive<Settings>({ ...props.state.settings });
const saving = ref(false);
const savingGeneral = ref(false);
const openingPath = ref<string | null>(null);
const section = ref<"general" | "codex" | "about">("general");
const themeOptions: { label: string; value: Settings["theme"] }[] = [
  { label: "浅色", value: "light" },
  { label: "深色", value: "dark" },
  { label: "跟随系统", value: "system" },
];

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
    });
    emit("saved", settings);
  } catch (error) {
    form.theme = previous.theme;
    form.auto_restart = previous.auto_restart;
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
    <h1 class="apple-title">设置</h1>
    <p class="muted mt-2 text-sm">控制外观、Codex 路径和重启行为。</p>

    <SegmentedControl
      v-model="section"
      class="mt-6"
      :options="[
        { value: 'general', label: '通用' },
        { value: 'codex', label: '应用' },
        { value: 'about', label: '关于' },
      ]"
    />

    <div v-if="section === 'general'" class="apple-group mt-4 p-5 sm:p-6">
      <n-form label-placement="top">
        <n-form-item label="主题">
          <div>
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
          </div>
        </n-form-item>
      </n-form>
    </div>

    <div v-else-if="section === 'codex'" class="apple-group mt-4 p-5 sm:p-6">
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

    <div v-else class="apple-group mt-4 p-5 sm:p-6">
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
