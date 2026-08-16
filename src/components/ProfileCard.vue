<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { NButton, NTag, useMessage } from "naive-ui";
import ProfileIconTile from "./ProfileIconTile.vue";
import { api } from "../api";
import type { ProfileSummary } from "../types";

const props = defineProps<{
  profile: ProfileSummary;
  active: boolean;
  busy: boolean;
  subscriptionAuthed?: boolean;
  subscriptionAccount?: string | null;
}>();

const emit = defineEmits<{
  apply: [];
  rename: [];
  remove: [];
  edit: [];
  duplicate: [];
}>();

const message = useMessage();
const testing = ref(false);
const connectionState = ref<"unknown" | "ok" | "fail">("unknown");

watch(
  () => props.profile.id,
  () => {
    connectionState.value = "unknown";
  },
);

const connectionDimmed = computed(() => {
  if (!props.profile.provider) return true;
  if (connectionState.value === "fail") return true;
  return !props.profile.has_key;
});

const connectionTitle = computed(() => {
  if (!props.profile.provider) return "该预设没有供应商配置，无法测试";
  if (!props.profile.has_key) return "缺少 API 密钥，点击查看提示";
  return "测试连通性";
});

async function openAdmin() {
  const url = props.profile.admin_url;
  if (!url) return;
  try {
    await api.openUrl(url);
  } catch (error) {
    message.error(String(error));
  }
}

async function testConnection() {
  if (testing.value || !props.profile.provider) return;
  if (!props.profile.has_key) {
    connectionState.value = "fail";
    message.warning(`「${props.profile.name}」还没有配置 API 密钥，请先填写后再测试`);
    return;
  }
  testing.value = true;
  try {
    const result = await api.testProfileConnection(props.profile.id);
    if (result.ok) {
      connectionState.value = "ok";
      const latency =
        result.latency_ms != null ? ` · ${result.latency_ms}ms` : "";
      message.success(`「${props.profile.name}」连接正常${latency}`);
    } else {
      connectionState.value = "fail";
      message.error(`「${props.profile.name}」连接失败：${result.error ?? "未知错误"}`);
    }
  } catch (error) {
    connectionState.value = "fail";
    message.error(`「${props.profile.name}」测试失败：${String(error)}`);
  } finally {
    testing.value = false;
  }
}
</script>

<template>
  <article class="flex flex-col gap-4 px-5 py-4 transition-colors sm:flex-row sm:items-center sm:justify-between" :class="active ? 'bg-[linear-gradient(90deg,var(--selection-bg),transparent_80%)]' : 'hover:bg-black/3 dark:hover:bg-white/4'" title="双击编辑" @dblclick="emit('edit')">
    <div class="flex min-w-0 flex-1 items-center gap-3">
      <ProfileIconTile :name="profile.name" :icon="profile.icon" />
      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-2">
          <h3 class="cursor-pointer truncate text-base font-semibold tracking-tight transition-colors hover:text-[#007aff]" title="点击重命名" @click="emit('rename')">{{ profile.name }}</h3>
          <n-tag v-if="active" type="success" size="small">当前生效</n-tag>
          <n-tag
            v-if="profile.provider === null"
            :type="subscriptionAuthed ? 'success' : 'warning'"
            size="small"
            :title="subscriptionAuthed ? (subscriptionAccount ? `当前订阅账号：${subscriptionAccount}` : 'ChatGPT 订阅已登录，Codex 使用订阅额度') : '尚未完成 ChatGPT 订阅登录，请到设置页认证'"
          >
            {{ subscriptionAuthed ? "订阅已认证" : "订阅未认证" }}
          </n-tag>
        </div>
        <div class="muted mt-1 flex flex-wrap items-center gap-1 text-[10px]">
          <span class="rounded-full border border-current/15 bg-black/4 px-1 py-px leading-none dark:bg-white/8">{{ profile.model ?? "未设置" }}</span>
          <span class="rounded-full border border-current/15 bg-black/4 px-1 py-px leading-none dark:bg-white/8">{{ profile.provider ?? "官方" }}</span>
          <span class="rounded-full border border-current/15 bg-black/4 px-1 py-px leading-none dark:bg-white/8">{{ profile.reasoning_effort ?? "默认" }}</span>
          <button
            v-if="profile.admin_url"
            type="button"
            class="grid h-4 w-4 place-items-center rounded-full text-[#007aff] transition-colors hover:bg-[#007aff]/10"
            title="打开官网"
            aria-label="打开官网"
            @click="openAdmin"
          >
            <svg class="h-3.5 w-3.5" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
              <path d="M14 3v2h3.59l-9.83 9.83 1.41 1.41L19 6.41V10h2V3h-7zM19 19H5V5h7V3H5c-1.11 0-2 .9-2 2v14c0 1.1.89 2 2 2h14c1.1 0 2-.9 2-2v-7h-2v7z" />
            </svg>
          </button>
        </div>
      </div>
    </div>
    <div class="flex shrink-0 items-center gap-2" @dblclick.stop>
      <n-button type="primary" size="small" :disabled="busy || active" @click="emit('apply')">应用</n-button>
      <button
        type="button"
        class="grid h-8 w-8 place-items-center rounded-lg text-zinc-400 transition-colors hover:bg-[#007aff]/10 hover:text-[#007aff] dark:text-zinc-500"
        title="复制预设"
        aria-label="复制预设"
        @click="emit('duplicate')"
      >
        <svg class="h-[18px] w-[18px]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <rect x="9" y="9" width="12" height="12" rx="2" />
          <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
        </svg>
      </button>
      <button
        type="button"
        class="grid h-8 w-8 place-items-center rounded-lg transition-colors hover:bg-[#007aff]/10 disabled:pointer-events-none disabled:opacity-40"
        :class="connectionDimmed ? 'text-zinc-400' : 'text-[#007aff]'"
        :disabled="!profile.provider || busy || testing"
        :title="connectionTitle"
        :aria-label="'测试连通性'"
        @click="testConnection"
      >
        <svg v-if="testing" class="h-4 w-4 animate-spin" viewBox="0 0 24 24" fill="none" aria-hidden="true">
          <circle cx="12" cy="12" r="9" stroke="currentColor" stroke-opacity="0.25" stroke-width="2.5" />
          <path d="M21 12a9 9 0 0 0-9-9" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" />
        </svg>
        <svg v-else class="h-[18px] w-[18px]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M4.5 11.5a13 13 0 0 1 15 0" />
          <path d="M7.5 15a8.5 8.5 0 0 1 9 0" />
          <path d="M10.5 18.5a4 4 0 0 1 3 0" />
        </svg>
      </button>
      <button
        type="button"
        class="grid h-8 w-8 place-items-center rounded-lg text-[#ff3b30]/60 transition-colors hover:bg-[#ff3b30]/10 hover:text-[#ff3b30] disabled:pointer-events-none disabled:opacity-40"
        :disabled="busy || active"
        title="删除"
        aria-label="删除"
        @click="emit('remove')"
      >
        <svg class="h-[18px] w-[18px]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M9.5 4.5a2.5 2.5 0 0 1 5 0" />
          <path d="M5 6.5h14" />
          <path d="M6 6.5V18a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V6.5" />
          <path d="M9.5 10v4.5" />
          <path d="M14.5 10v4.5" />
        </svg>
      </button>
    </div>
  </article>
</template>
