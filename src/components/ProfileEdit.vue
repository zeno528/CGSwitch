<script setup lang="ts">
import { computed, defineAsyncComponent, nextTick, onMounted, ref, watch } from "vue";
import { NButton, NInput, NSelect, NSwitch, useMessage } from "naive-ui";
import LoadingSpinner from "./LoadingSpinner.vue";
import ProfileIconEdit from "./ProfileIconEdit.vue";
import ProfileIconTile from "./ProfileIconTile.vue";
import { api } from "../api";
import {
  builtinPresets,
  customAuthTemplate,
  customCatalogTemplate,
  customConfigTemplate,
} from "../presets";
import type { ManagedAccount, ProfileDetail, ProfileSummary } from "../types";

// CodeMirror 编辑器按需加载：只在打开编辑弹窗时拉取，不影响应用启动
const ConfigTextEditor = defineAsyncComponent(() => import("./ConfigTextEditor.vue"));

const props = defineProps<{
  profile: ProfileSummary | null;
  create?: boolean;
}>();

// 读取 [model_providers.*] 段里的 base_url / 密钥，供编辑器回填表单
function readProviderFields(text: string): { base_url: string; experimental_bearer_token: string } {
  const values = { base_url: "", experimental_bearer_token: "" };
  const lines = text.split("\n");
  let providerId: string | null = null;
  for (const line of lines) {
    const m = /^model_provider\s*=\s*"([^"]+)"/.exec(line.trim());
    if (m) {
      providerId = m[1];
      break;
    }
  }
  let inProvider = false;
  let done = false;
  for (const line of lines) {
    const trimmed = line.trim();
    if (/^\[.+\]$/.test(trimmed)) {
      const section = /^\[model_providers\.(.+)\]$/.exec(trimmed);
      if (section && !done) {
        // 只处理 model_provider 指向的段；无 model_provider 时退化为第一段
        inProvider = providerId === null || section[1] === providerId;
        if (inProvider) done = true;
      } else {
        inProvider = false;
      }
      continue;
    }
    if (!inProvider) continue;
    const m =
      /^(base_url|experimental_bearer_token)\s*=\s*(?:(['"])(.*?)\2|([^\s]+))/.exec(trimmed);
    if (!m) continue;
    const field = m[1] as "base_url" | "experimental_bearer_token";
    values[field] = m[3] ?? m[4] ?? "";
  }
  return values;
}

// 把表单里的地址/密钥写回编辑器 provider 段；缺失的行在段尾补上
function patchProviderFields(text: string, baseUrl: string, apiKey: string): string {
  const escape = (value: string, quote: string) =>
    value.replace(/\\/g, "\\\\").replace(new RegExp(quote, "g"), "\\" + quote);
  const base = baseUrl.trim();
  const key = apiKey.trim();
  const lines = text.split("\n");
  let providerId: string | null = null;
  for (const line of lines) {
    const m = /^model_provider\s*=\s*"([^"]+)"/.exec(line.trim());
    if (m) {
      providerId = m[1];
      break;
    }
  }
  let inProvider = false;
  let done = false;
  let replacedBase = false;
  let replacedKey = false;
  const out: string[] = [];
  const flushMissing = () => {
    if (!inProvider) return;
    if (base && !replacedBase) out.push(`base_url = "${escape(base, '"')}"`);
    if (key && !replacedKey) out.push(`experimental_bearer_token = "${escape(key, '"')}"`);
    inProvider = false;
  };
  for (const line of lines) {
    const trimmed = line.trim();
    if (/^\[.+\]$/.test(trimmed)) {
      flushMissing();
      const section = /^\[model_providers\.(.+)\]$/.exec(trimmed);
      if (section && !done) {
        inProvider = providerId === null || section[1] === providerId;
        if (inProvider) done = true;
      } else {
        inProvider = false;
      }
      replacedBase = false;
      replacedKey = false;
      out.push(line);
      continue;
    }
    if (!inProvider) {
      out.push(line);
      continue;
    }
    const m = /^(base_url|experimental_bearer_token)\s*=\s*(['"]?)(.*?)\2\s*$/.exec(trimmed);
    if (!m) {
      out.push(line);
      continue;
    }
    const field = m[1];
    const quote = m[2] || '"';
    const value = field === "base_url" ? base : key;
    const indent = line.slice(0, line.length - line.trimStart().length);
    if (field === "base_url") replacedBase = true;
    else replacedKey = true;
    out.push(`${indent}${field} = ${quote}${escape(value, quote)}${quote}`);
  }
  flushMissing();
  return out.join("\n");
}

const emit = defineEmits<{
  back: [];
  changed: [];
}>();

const message = useMessage();
const detail = ref<ProfileDetail | null>(null);
const loadError = ref("");
const saving = ref(false);
const testing = ref(false);
const pickingIcon = ref(false);
const name = ref(props.profile?.name ?? "");
const baseUrl = ref("");
const apiKey = ref("");
const adminUrl = ref("");
const authAccounts = ref<ManagedAccount[]>([]);
const boundAccountId = ref<string | null>(null);
const selectedIcon = ref<string | null>(props.profile?.icon ?? null);
const activeTab = ref<"config" | "auth" | "models">("config");
const presetKind = ref("");
const configText = ref("");
const configTouched = ref(false);
const catalogTouched = ref(false);
const catalogText = ref("");
const authText = ref("");
const configInitial = ref("");
const catalogInitial = ref("");
const authInitial = ref("");
const showBalance = ref(false);
const savingBalance = ref(false);
// 初始数据装载完成后才允许双向同步，避免装载时产生假差异
let initialized = false;

const creating = computed(() => props.create === true);
const selectedPreset = computed(
  () => builtinPresets.find((preset) => preset.kind === presetKind.value) ?? null,
);
// 编辑器会把 CRLF 规范成 LF，比较时统一换行避免误报“未保存”
const normalizeNewlines = (text: string) => text.replace(/\r\n/g, "\n");
const configDirty = computed(
  () => normalizeNewlines(configText.value) !== normalizeNewlines(configInitial.value),
);
const catalogDirty = computed(
  () => normalizeNewlines(catalogText.value) !== normalizeNewlines(catalogInitial.value),
);
const authDirty = computed(
  () => normalizeNewlines(authText.value) !== normalizeNewlines(authInitial.value),
);
const showProviderFields = computed(() =>
  creating.value
    ? Boolean(selectedPreset.value?.base_url)
    : Boolean(detail.value?.provider),
);
const isOfficial = computed(() =>
  creating.value
    ? presetKind.value === "chatgpt"
    : detail.value?.provider === null,
);
const isCustom = computed(() => creating.value && presetKind.value === "custom");
const accountOptions = computed(() => [
  { label: "跟随默认账号", value: "" },
  ...authAccounts.value.map((account) => ({
    label: account.login,
    value: account.id,
  })),
]);
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
    if (isCustom.value) {
      list.push({ id: "models", label: "models.json" });
      list.push({ id: "auth", label: "auth.json" });
    } else if (selectedPreset.value?.model_values.model_catalog_json) {
      list.push({ id: "models", label: "models.json" });
    }
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
  if (isCustom.value) return Boolean(configText.value.trim());
  const preset = selectedPreset.value;
  return Boolean(preset);
});

function selectPreset(kind: string) {
  if (kind === "custom") {
    presetKind.value = kind;
    name.value = "自定义供应商";
    baseUrl.value = "https://api.example.com/v1";
    apiKey.value = "";
    adminUrl.value = "";
    selectedIcon.value = "custom";
    configText.value = customConfigTemplate;
    catalogText.value = customCatalogTemplate;
    authText.value = customAuthTemplate;
    configInitial.value = customConfigTemplate;
    catalogInitial.value = customCatalogTemplate;
    authInitial.value = customAuthTemplate;
    configTouched.value = false;
    catalogTouched.value = false;
    activeTab.value = "config";
    return;
  }
  const preset = builtinPresets.find((item) => item.kind === kind);
  if (!preset) return;
  configTouched.value = false;
  presetKind.value = kind;
  name.value = preset.name;
  baseUrl.value = preset.base_url;
  adminUrl.value = preset.admin_url ?? "";
  apiKey.value = "";
  configText.value = patchProviderFields(preset.fragment, baseUrl.value, apiKey.value);
  configInitial.value = configText.value;
  selectedIcon.value = preset.icon;
  activeTab.value = "config";
  if (kind === "chatgpt") loadAuthStatus();
}

async function loadAuthStatus() {
  try {
    const status = await api.authGetStatus();
    authAccounts.value = status.accounts;
  } catch {
    authAccounts.value = [];
  }
}

async function openAdminUrl() {
  const url = adminUrl.value.trim();
  if (!url) return;
  try {
    await api.openUrl(url);
  } catch (error) {
    message.error(String(error));
  }
}

async function testConnection() {
  if (testing.value || creating.value || !props.profile) return;
  if (!baseUrl.value.trim()) {
    message.warning("请填写调用地址");
    return;
  }
  if (!apiKey.value.trim()) {
    message.warning("请先填写 API 密钥");
    return;
  }
  testing.value = true;
  try {
    const result = await api.testProfileConnection(
      props.profile.id,
      baseUrl.value.trim(),
      apiKey.value.trim(),
    );
    if (result.ok) {
      message.success(
        `连接正常${result.latency_ms != null ? ` · ${result.latency_ms}ms` : ""}`,
      );
    } else {
      message.error(`连接失败：${result.error ?? "未知错误"}`);
    }
  } catch (error) {
    message.error(`测试失败：${String(error)}`);
  } finally {
    testing.value = false;
  }
}

watch(presetKind, async (kind) => {
  if (isCustom.value) return;
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

// 表单地址/密钥 → 编辑器 provider 段（所见即所得，始终同步）
watch([baseUrl, apiKey], () => {
  if (!initialized) return;
  const next = patchProviderFields(configText.value, baseUrl.value, apiKey.value);
  if (next !== configText.value) configText.value = next;
});

// 编辑器 provider 段 → 表单地址/密钥（所见即所得，始终同步）
watch(configText, (text) => {
  if (!initialized) return;
  const fields = readProviderFields(text);
  if (fields.base_url !== baseUrl.value) baseUrl.value = fields.base_url;
  // 模板占位符（<你的 API Key> 等）不应当回填进输入框
  const key = /^<.*>$/.test(fields.experimental_bearer_token)
    ? ""
    : fields.experimental_bearer_token;
  if (key !== apiKey.value) apiKey.value = key;
  if (creating.value && text !== liveConfigFragment.value) {
    configTouched.value = true;
  }
});

onMounted(async () => {
  if (creating.value) {
    selectPreset("custom");
  } else {
    try {
      if (!props.profile) throw new Error("缺少供应商信息");
      detail.value = await api.getProfile(props.profile.id);
      name.value = detail.value.name;
      configText.value = detail.value.raw_config ?? detail.value.config_fragment;
      catalogText.value = detail.value.raw_catalog ?? detail.value.catalog_content ?? "";
      authText.value = detail.value.raw_auth ?? detail.value.auth_content ?? "";
      baseUrl.value = detail.value.base_url ?? "";
      apiKey.value = detail.value.api_key ?? "";
      adminUrl.value = detail.value.admin_url ?? "";
      selectedIcon.value = detail.value.icon;
      if (detail.value.provider === null) {
        boundAccountId.value = detail.value.account_id ?? "";
      }
      configInitial.value = configText.value;
      catalogInitial.value = catalogText.value;
      authInitial.value = authText.value;
      showBalance.value = detail.value.show_balance;
    } catch (error) {
      loadError.value = String(error);
    }
  }
  await loadAuthStatus();
  await nextTick();
  initialized = true;
});

async function saveIcon(icon: string | null) {
  if (saving.value) return;
  saving.value = true;
  try {
    if (creating.value) {
      selectedIcon.value = icon;
    } else {
      if (!props.profile) throw new Error("缺少供应商信息");
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

async function toggleBalance(enabled: boolean) {
  if (savingBalance.value || !props.profile) return;
  savingBalance.value = true;
  try {
    await api.setProfileShowBalance(props.profile.id, enabled);
  } catch (error) {
    showBalance.value = !enabled;
    message.error(String(error));
  } finally {
    savingBalance.value = false;
  }
}

async function save() {
  if (saving.value) return;
  if (creating.value) {
    if (isCustom.value && !configText.value.trim()) {
      message.error("请填写 config.toml 内容");
      return;
    }
    if (!selectedPreset.value) {
      message.error("请先选择供应商");
      return;
    }
  }
  saving.value = true;
  try {
    if (creating.value) {
      if (isCustom.value) {
        await api.addCustomProfile(
          name.value.trim() || "自定义供应商",
          configText.value,
          baseUrl.value.trim() || undefined,
          apiKey.value.trim() || undefined,
          adminUrl.value.trim() || undefined,
          catalogText.value.trim() ? catalogText.value : null,
          authText.value.trim() ? authText.value : null,
        );
        message.success("自定义供应商已添加");
      } else {
        const created = await api.addBuiltinProfile(
          presetKind.value,
          baseUrl.value.trim() || undefined,
          apiKey.value.trim() || undefined,
          adminUrl.value.trim() || undefined,
          isOfficial.value ? boundAccountId.value || undefined : undefined,
        );
        if (configTouched.value || catalogTouched.value) {
          await api.updateProfileConfig(
            created.id,
            configText.value,
            selectedPreset.value?.model_values.model_catalog_json ? catalogText.value || null : null,
            null,
          );
        }
        message.success("内置供应商已添加");
      }
    } else {
      if (!props.profile) throw new Error("缺少供应商信息");
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
      if (isOfficial.value) {
        await api.setProfileAccount(props.profile.id, boundAccountId.value || null);
      }
      message.success("供应商已更新");
    }
    // 新建供应商：back 前先通知父级刷新列表，让首页立即显示新卡片（编辑路径由 closeEdit 刷新）
    if (creating.value) emit("changed");
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
    <div class="apple-page-bar">
      <button
        type="button"
        class="apple-page-header apple-back-button"
        aria-label="返回"
        @click="emit('back')"
      >
        <svg class="h-4 w-4 shrink-0 text-[#007aff]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
          <path d="M15 5.5 8.5 12l6.5 6.5" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
        <span class="apple-title">{{ creating ? "新建供应商" : "编辑供应商" }}</span>
      </button>
    </div>

    <div class="-mx-8 flex min-h-0 flex-1 flex-col overflow-auto pl-[var(--gap-main)] pr-[calc(var(--gap-main)-var(--scrollbar-size))] pb-4 [scrollbar-gutter:stable]">
      <p v-if="loadError" class="muted mt-4 text-sm">{{ loadError }}</p>

      <div v-if="creating" class="apple-group mt-[var(--gap-page)] shrink-0 p-[var(--gap-card)]">
      <div class="field-subtitle">选择供应商</div>
      <div class="mt-3 grid gap-2 sm:grid-cols-3 lg:grid-cols-4">
        <button
          v-for="preset in builtinPresets"
          :key="preset.kind"
          type="button"
          class="flex items-center gap-2.5 rounded-xl p-2.5 text-left transition-colors"
          :class="presetKind === preset.kind ? 'shadow-[0_0_0_1px_#007aff] bg-[var(--selection-bg)]' : 'shadow-[0_0_0_1px_var(--panel-ring)] hover:bg-black/3 dark:hover:bg-white/4'"
          :aria-pressed="presetKind === preset.kind"
          @click="selectPreset(preset.kind)"
        >
          <ProfileIconTile :name="preset.name" :icon="preset.icon" size="xs" />
          <span class="min-w-0 flex-1">
            <span class="block truncate text-xs font-semibold tracking-tight">{{ preset.name }}</span>
            <span class="muted block truncate text-[11px]">{{ preset.model }}{{ preset.base_url ? "" : (preset.kind === "chatgpt" ? " · 认证登录" : " · 无需密钥") }}</span>
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
          <n-input v-model:value="name" :bordered="false" class="underline-input" maxlength="50" placeholder="供应商名称" />
        </div>
      </div>
      <div v-if="showProviderFields" class="mt-4">
        <div class="field-label mb-1.5">请求地址</div>
        <n-input v-model:value="baseUrl" placeholder="https://api.example.com/v1" />
      </div>
      <div v-if="showProviderFields" class="mt-4">
        <div class="mb-1.5 flex items-center justify-between gap-2">
          <span class="field-label">API 密钥</span>
          <button
            v-if="!creating"
            type="button"
            class="flex h-6 shrink-0 items-center gap-1 rounded-md border border-[var(--panel-border)] px-2 text-[11px] font-medium text-[#007aff] transition-colors enabled:hover:bg-[#007aff]/10 disabled:cursor-not-allowed disabled:opacity-40"
            :disabled="!apiKey.trim() || !baseUrl.trim()"
            @click="testConnection"
          >
            <LoadingSpinner v-if="testing" />
            <svg v-else class="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <path d="M3 12h3l2-7 4 14 2-7h3" />
            </svg>
            测试连通
          </button>
        </div>
        <n-input v-model:value="apiKey" type="password" show-password-on="click" placeholder="请输入 API 密钥" />
      </div>
      <div v-if="isOfficial" class="mt-4">
        <div class="field-subtitle mb-1.5">订阅账号</div>
        <n-select
          v-model:value="boundAccountId"
          :options="accountOptions"
          placeholder="跟随默认账号"
        />
      </div>
      <div v-if="!creating || selectedPreset?.base_url" class="mt-4">
        <div class="mb-1.5 flex items-center gap-1">
          <span class="field-label">官网地址</span>
          <button
            type="button"
            class="grid h-4 w-4 cursor-pointer place-items-center rounded-full text-[#007aff] transition-colors hover:bg-[#007aff]/10 disabled:cursor-default disabled:opacity-40"
            title="打开官网"
            aria-label="打开官网"
            :disabled="!adminUrl.trim()"
            @click="openAdminUrl"
          >
            <svg class="h-3.5 w-3.5" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
              <path d="M14 3v2h3.59l-9.83 9.83 1.41 1.41L19 6.41V10h2V3h-7zM19 19H5V5h7V3H5c-1.11 0-2 .9-2 2v14c0 1.1.89 2 2 2h14c1.1 0 2-.9 2-2v-7h-2v7z" />
            </svg>
          </button>
        </div>
        <n-input v-model:value="adminUrl" placeholder="https://console.example.com（可选）" />
      </div>
      <div v-if="!creating && detail?.provider === 'deepseek'" class="mt-4 flex items-center justify-between gap-3">
        <div class="min-w-0">
          <div class="text-sm font-semibold">余额查询</div>
          <div class="muted mt-0.5 text-xs">窗口激活时自动刷新，点击余额手动刷新</div>
        </div>
        <n-switch v-model:value="showBalance" :disabled="savingBalance" @update:value="toggleBalance" />
      </div>
    </div>

      <div class="apple-group mt-[var(--gap-section)] flex shrink-0 flex-col p-[var(--gap-card)]">
      <div class="flex items-center justify-between gap-3">
        <div class="flex gap-1">
          <button
            v-for="tab in tabs"
            :key="tab.id"
            type="button"
            class="relative flex h-8 items-center gap-1.5 rounded-[10px] px-3 text-[13px] transition-colors"
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
            :placeholder="creating ? '选择供应商后显示配置预览' : '编辑 config.toml 内容，保存后仅写入该供应商；应用时才生效。'"
          />
        </div>
        <div v-else-if="activeTab === 'auth'">
          <ConfigTextEditor
            v-model="authText"
            language="json"
            placeholder="认证文件（~/.codex/auth.json）。保存后随该供应商生效；清空内容可移除该供应商的自定义认证。"
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
              placeholder="模型目录文件不存在或无法读取；保存后内容将随该供应商生效。"
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
