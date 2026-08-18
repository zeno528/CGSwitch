<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { NButton, NTag, useMessage } from "naive-ui";
import LoadingSpinner from "./LoadingSpinner.vue";
import ProfileIconTile from "./ProfileIconTile.vue";
import TrashIcon from "./TrashIcon.vue";
import { api } from "../api";
import { balanceChipClass, balanceQueryProviders } from "../presets";
import { useWindowActivation } from "../composables/useWindowActivation";
import type { ProfileBalanceInfo, ProfileSummary } from "../types";
import { PhArrowSquareOut, PhCopy, PhDotsSixVertical, PhKey, PhMonitor, PhWallet, PhWifiHigh } from "@phosphor-icons/vue";

// 模块级缓存：切换视图/窗口时数字立即可见，不等网络
const balanceInfoCache = new Map<string, ProfileBalanceInfo>();

const props = defineProps<{
  profile: ProfileSummary;
  active: boolean;
  busy: boolean;
  subscriptionAuthed?: boolean;
  subscriptionAccount?: string | null;
  subscriptionSource?: "desktop" | "oauth" | null;
  boundAccount?: string | null;
  balanceCache?: Record<string, ProfileBalanceInfo>;
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
const balanceInfo = ref<ProfileBalanceInfo | null>(null);
const balanceFetching = ref(false);
const balanceError = ref("");

// 是否支持余额查询由 presets.ts 的供应商表决定，新增供应商只需在那里加一行
const supportsBalance = computed(() =>
  balanceQueryProviders.has(props.profile.provider ?? ""),
);
const balanceTitle = computed(() => {
  if (!balanceError.value) {
    return balanceInfo.value?.usage_percent != null
      ? "用量，点击刷新"
      : "余额，点击刷新";
  }
  return balanceInfo.value
    ? `余额刷新失败：${balanceError.value}（显示上次余额，点击重试）`
    : `余额查询失败：${balanceError.value}（点击重试）`;
});

async function fetchBalance() {
  if (
    !supportsBalance.value ||
    !props.profile.show_balance ||
    !props.profile.has_key ||
    balanceFetching.value
  ) {
    return;
  }
  balanceFetching.value = true;
  try {
    const result = await api.getProfileBalance(props.profile.id);
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
  if (!supportsBalance.value) return;
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
  if (!props.profile.provider) return !props.subscriptionAuthed;
  if (connectionState.value === "fail") return true;
  return !props.profile.has_key;
});

const connectionTitle = computed(() => {
  if (!props.profile.provider) {
    return props.subscriptionAuthed ? "测试订阅认证连通性" : "尚未认证 ChatGPT 订阅";
  }
  if (!props.profile.has_key) return "缺少 API 密钥，点击查看提示";
  return "测试连通性";
});

const subscriptionTagLabel = computed(() => {
  if (!props.subscriptionAuthed) return "未认证";
  return props.boundAccount ?? props.subscriptionAccount ?? "";
});

const subscriptionSourceKind = computed(() => {
  if (!props.subscriptionAuthed) return null;
  return props.boundAccount ? "oauth" : props.subscriptionSource ?? "oauth";
});

const subscriptionTagTitle = computed(() => {
  if (!props.subscriptionAuthed) return "ChatGPT 尚未完成认证，请到设置页登录";
  if (props.boundAccount) {
    return `OAuth 认证账号：${props.boundAccount}`;
  }
  const source = subscriptionSourceKind.value === "desktop" ? "桌面端认证" : "OAuth 认证";
  return props.subscriptionAccount ? `${source}账号：${props.subscriptionAccount}` : source;
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
  if (testing.value) return;
  if (!props.profile.provider) {
    if (!props.subscriptionAuthed) {
      message.warning("尚未完成 ChatGPT 订阅认证，请先到设置页登录");
      return;
    }
  } else if (!props.profile.has_key) {
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
    class="flex cursor-pointer select-none flex-col gap-4 px-5 py-[var(--gap-card)] transition-colors sm:flex-row sm:items-center sm:justify-between"
    :class="active ? 'bg-[linear-gradient(90deg,var(--selection-bg),transparent_65%)]' : 'hover:bg-black/3 dark:hover:bg-white/4'"
    title="单击编辑"
    @click="emit('edit')"
  >
    <span
      class="drag-handle -ml-5 -mr-4 grid shrink-0 cursor-grab place-items-center self-center rounded-md py-1 pl-3 pr-3 text-zinc-400 transition-colors hover:text-zinc-600 active:cursor-grabbing dark:text-zinc-500 dark:hover:text-zinc-300"
      title="拖动排序"
      aria-label="拖动排序"
      @click.stop
    >
      <PhDotsSixVertical class="h-4 w-4" weight="bold" aria-hidden="true" />
    </span>
    <div class="flex min-w-0 flex-1 items-center gap-3">
      <ProfileIconTile :name="profile.name" :icon="profile.icon" />
      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-2">
          <h3 class="cursor-pointer truncate text-[16px] font-semibold tracking-tight transition-colors hover:text-accent" title="点击重命名" @click.stop="emit('rename')">{{ profile.name }}</h3>
          <span v-if="active" class="inline-flex items-center rounded-full bg-success px-2 py-0.5 text-xs font-semibold leading-none text-white">活动</span>
          <n-tag
            v-if="profile.provider === null"
            :type="subscriptionAuthed ? 'info' : 'warning'"
            size="small"
            class="max-w-[min(42vw,280px)]"
            :title="subscriptionTagTitle"
          >
            <span class="inline-flex min-w-0 max-w-full items-center gap-1.5 leading-4">
              <PhMonitor
                v-if="subscriptionSourceKind === 'desktop'"
                class="h-3.5 w-3.5 shrink-0"
                weight="bold"
                aria-hidden="true"
              />
              <PhKey
                v-else-if="subscriptionSourceKind === 'oauth'"
                class="h-3.5 w-3.5 shrink-0"
                weight="bold"
                aria-hidden="true"
              />
              <span class="min-w-0 truncate leading-4">{{ subscriptionTagLabel }}</span>
            </span>
          </n-tag>
        </div>
        <div class="muted mt-1 flex flex-wrap items-center gap-1">
          <span class="apple-chip">{{ profile.model ?? "未设置" }}</span>
          <span v-if="profile.provider" class="apple-chip">{{ profile.provider }}</span>
          <span class="apple-chip">{{ profile.reasoning_effort ?? "默认" }}</span>
          <button
            v-if="supportsBalance && profile.show_balance"
            type="button"
            class="apple-chip"
            :title="balanceTitle"
            :aria-label="'余额'"
            @click.stop="fetchBalance"
          >
            <PhWallet class="h-3 w-3" weight="bold" aria-hidden="true" />
            <template v-if="balanceInfo?.usage_percent != null">
              <span>5小时:</span>
              <span :class="balanceChipClass(balanceInfo.usage_percent, false)">{{ balanceInfo.usage_percent }}%</span>
              <span v-if="balanceInfo.usage_reset">{{ balanceInfo.usage_reset }}</span>
              <template v-if="balanceInfo.weekly_usage_percent != null">
                <span>· 7天:</span>
                <span :class="balanceChipClass(balanceInfo.weekly_usage_percent, false)">{{ balanceInfo.weekly_usage_percent }}%</span>
                <span v-if="balanceInfo.weekly_reset">{{ balanceInfo.weekly_reset }}</span>
              </template>
            </template>
            <template v-else-if="balanceInfo">
              <span>余额:</span>
              <span class="chip-success">
                {{ (balanceInfo.currency === "USD" ? "$" : "¥") + balanceInfo.total_balance + "  " + balanceInfo.currency }}
              </span>
            </template>
            <span v-else :class="balanceError ? 'chip-danger' : ''">
              {{ balanceError ? "查询失败" : "余额 --" }}
            </span>
          </button>
          <button
            v-if="profile.admin_url"
            type="button"
            class="grid h-4 w-4 place-items-center rounded-full text-accent transition-colors hover:bg-accent/10"
            title="打开官网"
            aria-label="打开官网"
            @click.stop="openAdmin"
          >
            <PhArrowSquareOut class="h-3.5 w-3.5" weight="bold" aria-hidden="true" />
          </button>
        </div>
      </div>
    </div>
    <div class="flex shrink-0 items-center gap-2" @click.stop>
      <n-button
        type="primary"
        size="small"
        :style="{ '--n-height': '32px', '--n-padding': '0 14px' }"
        :disabled="busy || active"
        @click="emit('apply')"
      >
        {{ active ? "已应用" : "应用" }}
      </n-button>
      <button
        type="button"
        class="grid h-8 w-8 place-items-center rounded-lg text-zinc-400 transition-colors hover:bg-accent/10 hover:text-accent dark:text-zinc-500"
        title="复制供应商"
        aria-label="复制供应商"
        @click="emit('duplicate')"
      >
        <PhCopy class="h-[18px] w-[18px]" weight="bold" aria-hidden="true" />
      </button>
      <button
        type="button"
        class="grid h-8 w-8 place-items-center rounded-lg transition-colors enabled:hover:bg-accent/10 disabled:cursor-not-allowed disabled:opacity-40"
        :class="connectionDimmed ? 'text-zinc-400' : 'text-accent'"
        :disabled="(!profile.provider && !subscriptionAuthed) || busy || testing"
        :title="connectionTitle"
        :aria-label="'测试连通性'"
        @click="testConnection"
      >
        <LoadingSpinner v-if="testing" size="md" />
        <PhWifiHigh v-else class="h-[18px] w-[18px]" weight="bold" aria-hidden="true" />
      </button>
      <button
        type="button"
        class="grid h-8 w-8 place-items-center rounded-lg text-[#ff3b30]/60 transition-colors enabled:hover:bg-[#ff3b30]/10 enabled:hover:text-[#ff3b30] disabled:cursor-not-allowed disabled:opacity-40"
        :disabled="busy || active"
        title="删除"
        aria-label="删除"
        @click="emit('remove')"
      >
        <TrashIcon />
      </button>
    </div>
  </article>
</template>
