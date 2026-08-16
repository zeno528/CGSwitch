<script setup lang="ts">
import { computed, defineAsyncComponent, onMounted, ref, watch } from "vue";
import { NButton, NInput, useMessage } from "naive-ui";
import ProfileIconEdit from "./ProfileIconEdit.vue";
import ProfileIconTile from "./ProfileIconTile.vue";
import { api } from "../api";
import { builtinPresets } from "../presets";
import type { ProfileDetail, ProfileSummary } from "../types";

// CodeMirror 编辑器按需加载：只在打开编辑弹窗时拉取，不影响应用启动
const ConfigTextEditor = defineAsyncComponent(() => import("./ConfigTextEditor.vue"));

const props = defineProps<{
  profile: ProfileSummary | null;
  create?: boolean;
}>();

const emit = defineEmits<{
  back: [];
  changed: [];
}>();

const message = useMessage();
const detail = ref<ProfileDetail | null>(null);
const loadError = ref("");
const saving = ref(false);
const pickingIcon = ref(false);
const name = ref(props.profile?.name ?? "");
const baseUrl = ref("");
const apiKey = ref("");
const adminUrl = ref("");
const selectedIcon = ref<string | null>(props.profile?.icon ?? null);
const activeTab = ref<"config" | "auth" | "models">("config");
const presetKind = ref("");
const configText = ref("");
const configTouched = ref(false);
const catalogTouched = ref(false);
let configBaselineSet = false;
const catalogText = ref("");
const authText = ref("");
const configInitial = ref("");
const catalogInitial = ref("");
const authInitial = ref("");

const creating = computed(() => props.create === true);
const selectedPreset = computed(
  () => builtinPresets.find((preset) => preset.kind === presetKind.value) ?? null,
);
const configDirty = computed(() => configText.value !== configInitial.value);
const catalogDirty = computed(() => catalogText.value !== catalogInitial.value);
const authDirty = computed(() => authText.value !== authInitial.value);
const showProviderFields = computed(() =>
  creating.value
    ? Boolean(selectedPreset.value?.base_url)
    : Boolean(detail.value?.provider),
);
// 官方订阅与带密钥的第三方都有认证文件组件
const hasAuthTab = computed(
  () =>
    !creating.value &&
    (detail.value?.provider === null || Boolean(detail.value?.api_key)),
);

const tabs = computed(() => {
  if (creating.value) {
    const list: { id: "config" | "auth" | "models"; label: string }[] = [
      { id: "config", label: "config.toml" },
    ];
    if (selectedPreset.value?.model_values.model_catalog_json)
      list.push({ id: "models", label: "models.json" });
    return list;
  }
  const list: { id: "config" | "auth" | "models"; label: string }[] = [
    { id: "config", label: "config.toml" },
  ];
  if (hasAuthTab.value) list.push({ id: "auth", label: "auth.json" });
  if (detail.value?.model_values.model_catalog_json)
    list.push({ id: "models", label: "models.json" });
  return list;
});

const catalogPath = computed(() => {
  const raw = creating.value
    ? selectedPreset.value?.model_values.model_catalog_json
    : detail.value?.model_values.model_catalog_json;
  return (raw ?? "").replace(/^["'`]+|["'`]+$/g, "");
});

const baseFragment = computed(() =>
  creating.value
    ? selectedPreset.value?.fragment ?? ""
    : detail.value?.config_fragment ?? "",
);

const liveConfigFragment = computed(() => {
  if (!baseFragment.value) return "";
  const values: Record<string, string> = {
    base_url: baseUrl.value.trim(),
    experimental_bearer_token: apiKey.value.trim(),
  };
  return baseFragment.value
    .split("\n")
    .map((line) => {
      const trimmed = line.trimStart();
      const match = /^(base_url|experimental_bearer_token)\s*=/.exec(trimmed);
      if (!match) return line;
      const field = match[1];
      const indent = line.slice(0, line.length - trimmed.length);
      const escaped = (values[field] ?? "").replace(/\\/g, "\\\\").replace(/"/g, '\\"');
      return `${indent}${field} = "${escaped}"`;
    })
    .join("\n");
});

const canSave = computed(() => {
  if (!creating.value) return true;
  const preset = selectedPreset.value;
  if (!preset) return false;
  return !preset.base_url || Boolean(apiKey.value.trim());
});

function selectPreset(kind: string) {
  const preset = builtinPresets.find((item) => item.kind === kind);
  if (!preset) return;
  configTouched.value = false;
  configBaselineSet = false;
  presetKind.value = kind;
  name.value = preset.name;
  baseUrl.value = preset.base_url;
  apiKey.value = "";
  selectedIcon.value = preset.icon;
  activeTab.value = "config";
}

watch(presetKind, async (kind) => {
  if (!creating.value || !kind) {
    catalogText.value = "";
    catalogTouched.value = false;
    return;
  }
  catalogTouched.value = false;
  const preset = builtinPresets.find((item) => item.kind === kind);
  if (!preset?.model_values.model_catalog_json) {
    catalogText.value = "";
    return;
  }
  try {
    catalogText.value = (await api.getBuiltinCatalog(kind)) ?? "";
    catalogInitial.value = catalogText.value;
  } catch {
    catalogText.value = "";
  }
});

// 创建模式：配置预览跟随表单字段刷新，用户手动改动后停止自动刷新
watch(liveConfigFragment, (fragment) => {
  if (!creating.value) return;
  if (!configTouched.value) configText.value = fragment;
  // 首次填充时建立“未保存”基准：此后任何改动（字段或编辑器）才显示圆点
  if (!configBaselineSet) {
    configBaselineSet = true;
    configInitial.value = fragment;
  }
});

watch(configText, (text) => {
  if (creating.value && text !== liveConfigFragment.value) configTouched.value = true;
});

onMounted(async () => {
  if (creating.value) return;
  try {
    if (!props.profile) throw new Error("缺少预设信息");
    detail.value = await api.getProfile(props.profile.id);
    name.value = detail.value.name;
    baseUrl.value = detail.value.base_url ?? "";
    apiKey.value = detail.value.api_key ?? "";
    adminUrl.value = detail.value.admin_url ?? "";
    selectedIcon.value = detail.value.icon;
    configText.value = detail.value.raw_config ?? detail.value.config_fragment;
    catalogText.value = detail.value.raw_catalog ?? detail.value.catalog_content ?? "";
    authText.value = detail.value.raw_auth ?? detail.value.auth_content ?? "";
    configInitial.value = configText.value;
    catalogInitial.value = catalogText.value;
    authInitial.value = authText.value;
  } catch (error) {
    loadError.value = String(error);
  }
});

async function saveIcon(icon: string | null) {
  if (saving.value) return;
  saving.value = true;
  try {
    if (creating.value) {
      selectedIcon.value = icon;
    } else {
      if (!props.profile) throw new Error("缺少预设信息");
      await api.setProfileIcon(props.profile.id, icon);
      selectedIcon.value = icon;
      if (detail.value) detail.value.icon = icon;
    }
    emit("changed");
    pickingIcon.value = false;
  } catch (error) {
    message.error(String(error));
  } finally {
    saving.value = false;
  }
}

async function save() {
  if (saving.value) return;
  if (creating.value) {
    if (!selectedPreset.value) {
      message.error("请先选择供应商");
      return;
    }
    if (selectedPreset.value.base_url && !apiKey.value.trim()) {
      message.error("请先填写 API 密钥");
      return;
    }
  }
  saving.value = true;
  try {
    if (creating.value) {
      const created = await api.addBuiltinProfile(
        presetKind.value,
        baseUrl.value.trim() || undefined,
        apiKey.value.trim() || undefined,
      );
      if (configTouched.value || catalogTouched.value) {
        await api.updateProfileConfig(
          created.id,
          configText.value,
          selectedPreset.value?.model_values.model_catalog_json ? catalogText.value || null : null,
          null,
        );
      }
      message.success("内置预设已添加");
    } else {
      if (!props.profile) throw new Error("缺少预设信息");
      const hasProvider = Boolean(detail.value?.provider);
      await api.updateProfile(
        props.profile.id,
        name.value,
        hasProvider ? baseUrl.value : undefined,
        hasProvider ? apiKey.value : undefined,
        adminUrl.value.trim() || undefined,
      );
      await api.updateProfileConfig(
        props.profile.id,
        configText.value,
        detail.value?.model_values.model_catalog_json && catalogDirty.value
          ? catalogText.value || null
          : null,
        hasAuthTab.value && authDirty.value ? authText.value : null,
      );
      message.success("配置预设已更新");
    }
    emit("changed");
    emit("back");
  } catch (error) {
    message.error(String(error));
  } finally {
    saving.value = false;
  }
}

</script>

<template>
  <ProfileIconEdit
    v-if="pickingIcon"
    :icon="selectedIcon"
    :name="name"
    @back="pickingIcon = false"
    @save="saveIcon"
  />
  <section v-else class="mx-auto flex h-[calc(100vh-2.75rem)] w-full max-w-none flex-col" @keydown.ctrl.enter="save">
    <div class="-mx-8 flex items-center bg-[var(--app-bg)] px-8 py-2">
      <button
        type="button"
        class="apple-page-header apple-back-button"
        aria-label="返回"
        @click="emit('back')"
      >
        <svg class="h-4 w-4 shrink-0 text-[#007aff]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
          <path d="M15 5.5 8.5 12l6.5 6.5" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
        <span class="apple-title">{{ creating ? "新建预设" : "编辑预设" }}</span>
      </button>
    </div>

    <div class="-mx-8 flex min-h-0 flex-1 flex-col overflow-auto px-8 pb-4 [scrollbar-gutter:stable]">
      <p v-if="loadError" class="muted mt-4 text-sm">{{ loadError }}</p>

      <div v-if="creating" class="apple-group mt-[var(--gap-page)] shrink-0 p-[var(--gap-card)]">
      <div class="field-subtitle">选择供应商</div>
      <div class="mt-3 grid gap-2 sm:grid-cols-3 lg:grid-cols-4">
        <button
          v-for="preset in builtinPresets"
          :key="preset.kind"
          type="button"
          class="flex items-center gap-2.5 rounded-xl border p-2.5 text-left transition-colors"
          :class="presetKind === preset.kind ? 'border-[#007aff] bg-[var(--selection-bg)]' : 'border-[var(--panel-border)] hover:bg-black/3 dark:hover:bg-white/4'"
          :aria-pressed="presetKind === preset.kind"
          @click="selectPreset(preset.kind)"
        >
          <ProfileIconTile :name="preset.name" :icon="preset.icon" size="xs" />
          <span class="min-w-0 flex-1">
            <span class="block truncate text-xs font-semibold tracking-tight">{{ preset.name }}</span>
            <span class="muted block truncate text-[11px]">{{ preset.model }}{{ preset.base_url ? "" : " · 无需密钥" }}</span>
          </span>
          <svg v-if="presetKind === preset.kind" class="h-4 w-4 shrink-0 text-[#007aff]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" aria-hidden="true">
            <path d="m6 12.5 4 4 8-9" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
        </button>
      </div>
    </div>

      <div class="apple-group shrink-0 p-[var(--gap-card)]" :class="creating ? 'mt-[var(--gap-section)]' : 'mt-[var(--gap-page)]'">
      <div class="flex items-center gap-4">
        <button
          type="button"
          class="relative grid h-[61px] w-[61px] shrink-0 place-items-center rounded-[16px] transition-opacity hover:opacity-80"
          title="点击更换图标"
          :aria-label="'更换图标'"
          @click="pickingIcon = true"
        >
          <span class="relative grid h-full w-full place-items-center">
            <ProfileIconTile :name="detail?.name ?? name" :icon="selectedIcon" size="fill" />
            <span class="absolute -bottom-1 -right-1 grid h-5 w-5 place-items-center rounded-full bg-[#007aff] text-white shadow" aria-hidden="true">
              <svg class="h-2.5 w-2.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" aria-hidden="true">
                <path d="M17 3a2.8 2.8 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" stroke-linejoin="round" />
              </svg>
            </span>
          </span>
        </button>
        <div class="min-w-0 flex-1">
          <div class="field-label mb-1.5">名称</div>
          <n-input v-model:value="name" :bordered="false" class="underline-input" maxlength="50" placeholder="预设名称" />
        </div>
      </div>
      <div v-if="showProviderFields" class="mt-4">
        <div class="field-label mb-1.5">调用地址</div>
        <n-input v-model:value="baseUrl" placeholder="https://api.example.com/v1" />
      </div>
      <div v-if="showProviderFields" class="mt-4">
        <div class="field-label mb-1.5">密钥</div>
        <n-input v-model:value="apiKey" type="password" show-password-on="click" placeholder="请输入 API 密钥" />
      </div>
      <div v-if="!creating" class="mt-4">
        <div class="field-label mb-1.5">官网地址</div>
        <n-input v-model:value="adminUrl" placeholder="https://console.example.com（可选）" />
      </div>
    </div>

      <div class="apple-group mt-[var(--gap-section)] flex shrink-0 flex-col p-[var(--gap-card)]">
      <div class="flex items-center justify-between gap-3">
        <div class="flex gap-1">
          <button
            v-for="tab in tabs"
            :key="tab.id"
            type="button"
            class="flex h-8 items-center gap-1.5 rounded-[10px] px-3 text-[13px] transition-colors"
            :class="activeTab === tab.id ? 'bg-[var(--selection-bg)] font-semibold text-[#007aff]' : 'muted hover:bg-black/5 dark:hover:bg-white/8'"
            :aria-pressed="activeTab === tab.id"
            @click="activeTab = tab.id"
          >
            <svg v-if="tab.id === 'config'" class="h-3.5 w-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true">
              <circle cx="12" cy="12" r="3" />
              <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
            </svg>
            <svg v-else class="h-3.5 w-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <path d="M8 3H7a2 2 0 0 0-2 2v5a2 2 0 0 1-2 2 2 2 0 0 1 2 2v5c0 1.1.9 2 2 2h1" />
              <path d="M16 21h1a2 2 0 0 0 2-2v-5c0-1.1.9-2 2-2a2 2 0 0 1-2-2V5a2 2 0 0 0-2-2h-1" />
            </svg>
            <span class="relative inline-grid">
              <span class="invisible font-semibold" aria-hidden="true">{{ tab.label }}</span>
              <span class="absolute inset-0 whitespace-nowrap">{{ tab.label }}</span>
            </span>
            <span
              v-if="(tab.id === 'config' && configDirty) || (tab.id === 'models' && catalogDirty) || (tab.id === 'auth' && authDirty)"
              class="absolute right-1.5 top-1.5 h-1.5 w-1.5 rounded-full bg-[#007aff]"
              aria-label="有未保存的改动"
            />
          </button>
        </div>
      </div>

      <div class="mt-4 flex flex-col pr-1">
        <div v-if="activeTab === 'config'">
          <ConfigTextEditor
            v-model="configText"
            language="toml"
            :placeholder="creating ? '选择供应商后显示配置预览' : '编辑 config.toml 内容，保存后仅写入预设；应用时才生效。'"
          />
        </div>
        <div v-else-if="activeTab === 'auth'">
          <ConfigTextEditor
            v-model="authText"
            language="json"
            placeholder="认证文件（~/.codex/auth.json）。保存后随预设生效；清空内容可移除预设自定义认证。"
          />
        </div>
        <div v-else class="flex flex-col text-sm">
          <div class="flex justify-between gap-4 py-2">
            <span class="field-label">模型目录</span>
            <span class="mono">{{ catalogPath }}</span>
          </div>
          <div>
            <ConfigTextEditor
              v-model="catalogText"
              language="json"
              placeholder="模型目录文件不存在或无法读取；保存后内容将随预设生效。"
            />
          </div>
        </div>
      </div>
      </div>
    </div>

    <div class="-mx-8 -mb-7 flex items-center justify-end gap-2 bg-[var(--app-bg)] pl-8 pr-[42px] pt-2 pb-4">
      <n-button secondary @click="emit('back')">取消</n-button>
      <n-button type="primary" :loading="saving" :disabled="!canSave" @click="save">
        <template #icon>
          <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z" />
            <path d="M17 21v-8H7v8" />
            <path d="M7 3v5h8" />
          </svg>
        </template>
        保存
      </n-button>
    </div>
  </section>
</template>
