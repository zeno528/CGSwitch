<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { NButton, NTag, useMessage } from "naive-ui";
import LoadingSpinner from "./LoadingSpinner.vue";
import ProfileIconTile from "./ProfileIconTile.vue";
import { api } from "../api";
import { useWindowActivation } from "../composables/useWindowActivation";
import type { DeepSeekBalanceInfo, ProfileSummary } from "../types";

// 模块级缓存：切换视图/窗口时数字立即可见，不等网络
const balanceInfoCache = new Map<string, DeepSeekBalanceInfo>();

const props = defineProps<{
  profile: ProfileSummary;
  active: boolean;
  busy: boolean;
  subscriptionAuthed?: boolean;
  subscriptionAccount?: string | null;
  boundAccount?: string | null;
  balanceCache?: Record<string, DeepSeekBalanceInfo>;
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
// 最近一次成功拿到的余额数字；拿到过就常驻，刷新失败/返回不可用都不清空
const balanceInfo = ref<DeepSeekBalanceInfo | null>(null);
const balanceFetching = ref(false);
const balanceError = ref("");

const isDeepSeek = computed(() => props.profile.provider === "deepseek");
const balanceText = computed(() => {
  if (balanceInfo.value) {
    const symbol = balanceInfo.value.currency === "USD" ? "$" : "¥";
    return `余额 ${symbol}${balanceInfo.value.total_balance}`;
  }
  // 规矩：像模型/供应商胶囊一样始终有内容；失败文案只在获取失败时显示
  return balanceError.value ? "查询失败" : "余额 --";
});
const balanceTitle = computed(() => {
  if (!balanceError.value) return "DeepSeek 余额，点击刷新";
  return balanceInfo.value
    ? `余额刷新失败：${balanceError.value}（显示上次余额，点击重试）`
    : `余额查询失败：${balanceError.value}（点击重试）`;
});

async function fetchBalance() {
  if (
    !isDeepSeek.value ||
    !props.profile.show_balance ||
    !props.profile.has_key ||
    balanceFetching.value
  ) {
    return;
  }
  balanceFetching.value = true;
  try {
    const result = await api.getDeepseekBalance(props.profile.id);
    balanceError.value = "";
    const info = result.balance_infos[0];
    if (info) {
      balanceInfo.value = info;
      balanceInfoCache.set(props.profile.id, info);
      void api.setProfileBalance(props.profile.id, info);
    }
  } catch (error) {
    balanceError.value = String(error);
  } finally {
    balanceFetching.value = false;
  }
}

onMounted(() => {
  if (!isDeepSeek.value) return;
  // 先取缓存数字（模块缓存 > 应用状态缓存），保证首次渲染就有数字
  balanceInfo.value =
    balanceInfoCache.get(props.profile.id) ??
    props.balanceCache?.[props.profile.id] ??
    null;
  void fetchBalance();
});

// 看 APP 时刷新余额（窗口激活即拉取，不轮询）
useWindowActivation({ onActive: () => void fetchBalance() });

// 供应商级开关从关到开（编辑页保存后返回）时补拉一次余额
watch(
  () => props.profile.show_balance,
  (enabled) => {
    if (enabled) void fetchBalance();
  },
);

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
  if (!props.profile.provider) return "该供应商缺少配置，无法测试";
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
  <article
    class="flex cursor-pointer flex-col gap-4 px-5 py-4 transition-colors sm:flex-row sm:items-center sm:justify-between"
    :class="active ? 'bg-[linear-gradient(90deg,var(--selection-bg),transparent_80%)]' : 'hover:bg-black/3 dark:hover:bg-white/4'"
    title="单击编辑"
    @click="emit('edit')"
  >
    <div class="flex min-w-0 flex-1 items-center gap-3">
      <ProfileIconTile :name="profile.name" :icon="profile.icon" />
      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-2">
          <h3 class="cursor-pointer truncate text-base font-semibold tracking-tight transition-colors hover:text-[#007aff]" title="点击重命名" @click.stop="emit('rename')">{{ profile.name }}</h3>
          <n-tag v-if="active" type="success" size="small">当前生效</n-tag>
          <n-tag
            v-if="profile.provider === null"
            :type="subscriptionAuthed ? 'success' : 'warning'"
            size="small"
            :title="subscriptionAuthed ? (subscriptionAccount ? `当前订阅账号：${subscriptionAccount}` : 'ChatGPT 订阅已登录，Codex 使用订阅额度') : '尚未完成 ChatGPT 订阅登录，请到设置页认证'"
          >
            {{ subscriptionAuthed ? "订阅已认证" : "订阅未认证" }}
          </n-tag>
          <n-tag v-if="profile.provider === null && boundAccount" size="small" type="info">
            订阅账号：{{ boundAccount }}
          </n-tag>
        </div>
        <div class="muted mt-1 flex flex-wrap items-center gap-1 text-[10px]">
          <span class="rounded-full border border-current/15 bg-black/4 px-1 py-px leading-none dark:bg-white/8">{{ profile.model ?? "未设置" }}</span>
          <span class="rounded-full border border-current/15 bg-black/4 px-1 py-px leading-none dark:bg-white/8">{{ profile.provider ?? "官方" }}</span>
          <span class="rounded-full border border-current/15 bg-black/4 px-1 py-px leading-none dark:bg-white/8">{{ profile.reasoning_effort ?? "默认" }}</span>
          <button
            v-if="isDeepSeek && profile.show_balance"
            type="button"
            class="flex items-center gap-1 rounded-full border border-current/15 bg-black/4 px-1.5 py-px leading-none dark:bg-white/8"
            :class="balanceError && !balanceInfo ? 'text-[#ff3b30]/80' : 'text-[#007aff]'"
            :title="balanceTitle"
            :aria-label="'DeepSeek 余额'"
            @click.stop="fetchBalance"
          >
            <svg class="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <rect x="2" y="6" width="20" height="12" rx="2" />
              <path d="M2 10h20" />
            </svg>
            {{ balanceText }}
          </button>
          <button
            v-if="profile.admin_url"
            type="button"
            class="grid h-4 w-4 place-items-center rounded-full text-[#007aff] transition-colors hover:bg-[#007aff]/10"
            title="打开官网"
            aria-label="打开官网"
            @click.stop="openAdmin"
          >
            <svg class="h-3.5 w-3.5" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
              <path d="M14 3v2h3.59l-9.83 9.83 1.41 1.41L19 6.41V10h2V3h-7zM19 19H5V5h7V3H5c-1.11 0-2 .9-2 2v14c0 1.1.89 2 2 2h14c1.1 0 2-.9 2-2v-7h-2v7z" />
            </svg>
          </button>
        </div>
      </div>
    </div>
    <div class="flex shrink-0 items-center gap-2" @click.stop>
      <n-button type="primary" size="small" :disabled="busy || active" @click="emit('apply')">{{ active ? "已应用" : "应用" }}</n-button>
      <button
        type="button"
        class="grid h-8 w-8 place-items-center rounded-lg text-zinc-400 transition-colors hover:bg-[#007aff]/10 hover:text-[#007aff] dark:text-zinc-500"
        title="复制供应商"
        aria-label="复制供应商"
        @click="emit('duplicate')"
      >
        <svg class="h-[18px] w-[18px]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <rect x="9" y="9" width="12" height="12" rx="2" />
          <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
        </svg>
      </button>
      <button
        type="button"
        class="grid h-8 w-8 place-items-center rounded-lg transition-colors enabled:hover:bg-[#007aff]/10 disabled:cursor-not-allowed disabled:opacity-40"
        :class="connectionDimmed ? 'text-zinc-400' : 'text-[#007aff]'"
        :disabled="!profile.provider || busy || testing"
        :title="connectionTitle"
        :aria-label="'测试连通性'"
        @click="testConnection"
      >
        <LoadingSpinner v-if="testing" size="md" />
        <svg v-else class="h-[18px] w-[18px]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M4.5 11.5a13 13 0 0 1 15 0" />
          <path d="M7.5 15a8.5 8.5 0 0 1 9 0" />
          <path d="M10.5 18.5a4 4 0 0 1 3 0" />
        </svg>
      </button>
      <button
        type="button"
        class="grid h-8 w-8 place-items-center rounded-lg text-[#ff3b30]/60 transition-colors enabled:hover:bg-[#ff3b30]/10 enabled:hover:text-[#ff3b30] disabled:cursor-not-allowed disabled:opacity-40"
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
