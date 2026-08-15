<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { NButton, NInput, useMessage } from "naive-ui";
import ProfileIconEdit from "./ProfileIconEdit.vue";
import ProfileIconTile from "./ProfileIconTile.vue";
import { api } from "../api";
import type { ProfileDetail, ProfileSummary } from "../types";

const props = defineProps<{ profile: ProfileSummary }>();

const emit = defineEmits<{
  back: [];
  changed: [];
}>();

const message = useMessage();
const detail = ref<ProfileDetail | null>(null);
const loadError = ref("");
const saving = ref(false);
const pickingIcon = ref(false);
const name = ref(props.profile.name);
const baseUrl = ref("");
const apiKey = ref("");
const selectedIcon = ref(props.profile.icon);
const activeTab = ref<"config" | "auth" | "models">("config");

const tabs = computed(() => {
  const list: { id: "config" | "auth" | "models"; label: string }[] = [
    { id: "config", label: "config" },
  ];
  if (detail.value?.api_key) list.push({ id: "auth", label: "auth" });
  if (detail.value?.model_values.model_catalog_json)
    list.push({ id: "models", label: "models.json" });
  return list;
});

const catalogPath = computed(() =>
  detail.value?.model_values.model_catalog_json?.replace(/^"|"$/g, "") ?? "",
);

const liveConfigFragment = computed(() => {
  if (!detail.value) return "";
  const values: Record<string, string> = {
    base_url: baseUrl.value.trim(),
    experimental_bearer_token: apiKey.value.trim(),
  };
  return detail.value.config_fragment
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

onMounted(async () => {
  try {
    detail.value = await api.getProfile(props.profile.id);
    name.value = detail.value.name;
    baseUrl.value = detail.value.base_url ?? "";
    apiKey.value = detail.value.api_key ?? "";
    selectedIcon.value = detail.value.icon;
  } catch (error) {
    loadError.value = String(error);
  }
});

async function saveIcon(icon: string | null) {
  if (saving.value) return;
  saving.value = true;
  try {
    await api.setProfileIcon(props.profile.id, icon);
    selectedIcon.value = icon;
    if (detail.value) detail.value.icon = icon;
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
  saving.value = true;
  try {
    const hasProvider = Boolean(detail.value?.provider);
    await api.updateProfile(
      props.profile.id,
      name.value,
      hasProvider ? baseUrl.value : undefined,
      hasProvider ? apiKey.value : undefined,
    );
    message.success("配置档案已更新");
    emit("changed");
    emit("back");
  } catch (error) {
    message.error(String(error));
  } finally {
    saving.value = false;
  }
}

async function openActiveFile() {
  const relative =
    activeTab.value === "config"
      ? "config.toml"
      : activeTab.value === "auth"
        ? "auth.json"
        : catalogPath.value;
  if (!relative) {
    message.warning("没有可打开的文件");
    return;
  }
  try {
    await api.openCodexFile(relative);
  } catch (error) {
    message.error(String(error));
  }
}
</script>

<template>
  <ProfileIconEdit
    v-if="pickingIcon"
    :profile="profile"
    @back="pickingIcon = false"
    @save="saveIcon"
  />
  <section v-else class="mx-auto flex h-full w-full max-w-none flex-col">
    <button
      type="button"
      class="-ml-2 flex items-center gap-1.5 rounded-lg px-2 py-1 text-left transition-colors hover:bg-black/5 dark:hover:bg-white/8"
      aria-label="返回"
      @click="emit('back')"
    >
      <svg class="h-4 w-4 shrink-0 text-[#007aff]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
        <path d="M15 5.5 8.5 12l6.5 6.5" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
      <span class="apple-title">编辑档案</span>
    </button>

    <p v-if="loadError" class="muted mt-4 text-sm">{{ loadError }}</p>

    <div class="mt-6 flex justify-center">
      <button
        type="button"
        class="group relative grid h-[76px] w-[76px] place-items-center rounded-[22px] transition-opacity hover:opacity-80"
        title="点击更换图标"
        :aria-label="'更换图标'"
        @click="pickingIcon = true"
      >
        <ProfileIconTile :name="detail?.name ?? profile.name" :icon="selectedIcon" size="lg" />
        <span class="absolute -bottom-1.5 -right-1.5 grid h-6 w-6 place-items-center rounded-full bg-[#007aff] text-white shadow" aria-hidden="true">
          <svg class="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" aria-hidden="true">
            <path d="M17 3a2.8 2.8 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" stroke-linejoin="round" />
          </svg>
        </span>
      </button>
    </div>

    <div class="apple-group mt-6 p-5 sm:p-6">
      <div class="grid gap-4 sm:grid-cols-2">
        <div>
          <div class="muted mb-1.5 text-[13px]">名称</div>
          <n-input v-model:value="name" maxlength="50" placeholder="档案名称" />
        </div>
        <div>
          <div class="muted mb-1.5 text-[13px]">供应商</div>
          <n-input :value="detail?.provider ?? '官方'" readonly />
        </div>
        <div v-if="detail?.provider" class="sm:col-span-2">
          <div class="muted mb-1.5 text-[13px]">调用地址</div>
          <n-input v-model:value="baseUrl" placeholder="https://api.example.com/v1" />
        </div>
        <div v-if="detail?.provider" class="sm:col-span-2">
          <div class="muted mb-1.5 text-[13px]">密钥</div>
          <n-input v-model:value="apiKey" type="password" show-password-on="click" placeholder="供应商密钥" />
        </div>
      </div>
    </div>

    <div class="apple-group mt-4 flex min-h-[180px] flex-1 flex-col p-5 sm:p-6">
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
            <svg class="h-3.5 w-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true">
              <path d="M6 3h9l4 4v14H6z" stroke-linejoin="round" />
              <path d="M15 3v4h4" />
            </svg>
            {{ tab.label }}
          </button>
        </div>
        <n-button size="small" secondary title="用默认编辑器打开当前选中的文件" @click="openActiveFile">打开</n-button>
      </div>

      <div class="mt-4 min-h-0 flex-1 overflow-auto pr-1">
        <pre v-if="activeTab === 'config'" class="mono overflow-x-auto rounded-xl bg-black/4 p-3 text-xs leading-relaxed dark:bg-white/6">{{ liveConfigFragment }}</pre>
        <div v-else-if="activeTab === 'auth'" class="text-sm">
          <pre v-if="detail?.auth_content" class="mono overflow-x-auto rounded-xl bg-black/4 p-3 text-xs leading-relaxed dark:bg-white/6">{{ detail.auth_content }}</pre>
          <p v-else class="muted mt-2 text-xs">认证文件（~/.codex/auth.json）不存在或无法读取。</p>
        </div>
        <div v-else class="text-sm">
          <div class="flex justify-between gap-4 py-2">
            <span class="muted">模型目录</span>
            <span class="mono">{{ catalogPath }}</span>
          </div>
          <pre v-if="detail?.catalog_content" class="mono mt-2 overflow-x-auto rounded-xl bg-black/4 p-3 text-xs leading-relaxed dark:bg-white/6">{{ detail.catalog_content }}</pre>
          <p v-else class="muted mt-2 text-xs">模型目录文件不存在或无法读取，文件内容未显示。</p>
        </div>
      </div>
    </div>

    <div class="mt-5 flex items-center justify-end gap-2">
      <span class="muted mr-auto text-xs">保存仅更新档案快照，切换时才写回 config.toml</span>
      <n-button @click="emit('back')">取消</n-button>
      <n-button type="primary" :loading="saving" @click="save">保存</n-button>
    </div>
  </section>
</template>
