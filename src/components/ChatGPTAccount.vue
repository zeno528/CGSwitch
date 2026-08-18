<script setup lang="ts">
import { onMounted, ref } from "vue";
import { NButton, NTag, useDialog, useMessage } from "naive-ui";
import { api } from "../api";
import type { AuthStatus, DeviceCodeResponse } from "../types";
import {
  PhArrowSquareOut,
  PhCheckCircle,
  PhCopy,
  PhPlus,
  PhShieldCheck,
  PhUserCircle,
} from "@phosphor-icons/vue";

const message = useMessage();
const dialog = useDialog();
const status = ref<AuthStatus | null>(null);
const loadError = ref("");
const busy = ref(false);
const login = ref<DeviceCodeResponse | null>(null);

async function refreshStatus() {
  try {
    status.value = await api.authGetStatus();
    loadError.value = "";
  } catch (error) {
    loadError.value = String(error);
  }
}

async function startLogin() {
  if (busy.value) return;
  busy.value = true;
  login.value = null;
  try {
    login.value = await api.authStartLogin();
    await api.openUrl(login.value.verification_uri);
    poll();
  } catch (error) {
    const text = String(error);
    if (text.includes("unsupported_country_region_territory")) {
      message.error(
        "认证请求被地区限制拦截。请开启系统代理并确认节点位于 ChatGPT 支持的地区后重试。",
        { duration: 6000 }
      );
    } else {
      message.error(text);
    }
    busy.value = false;
  }
}

async function poll() {
  const current = login.value;
  if (!current) return;
  try {
    const deadline = Date.now() + current.expires_in * 1000;
    while (Date.now() < deadline) {
      const account = await api.authPollForAccount(current.device_code);
      if (account) {
        login.value = null;
        await refreshStatus();
        message.success("ChatGPT 账号已添加，可手动设为当前");
        break;
      }
      await new Promise((resolve) => setTimeout(resolve, current.interval * 1000));
    }
  } catch (error) {
    message.error(String(error));
    login.value = null;
  } finally {
    busy.value = false;
  }
}

function openVerification() {
  if (login.value) void api.openUrl(login.value.verification_uri);
}

async function copyUserCode() {
  const code = login.value?.user_code;
  if (!code) return;
  try {
    await navigator.clipboard.writeText(code);
    message.success("授权码已复制");
  } catch {
    message.error("复制失败，请手动选择复制");
  }
}

function cancelLogin() {
  login.value = null;
  busy.value = false;
}

async function removeAccount(accountId: string) {
  dialog.warning({
    title: "移除订阅账号",
    content: "确定移除该 ChatGPT 订阅账号吗？移除后本机将清除该账号的登录凭据。",
    positiveText: "移除",
    negativeText: "取消",
    positiveButtonProps: { type: "error" },
    onPositiveClick: async () => {
      try {
        await api.authRemoveAccount(accountId);
        message.success("账号已移除");
        await refreshStatus();
      } catch (error) {
        message.error(String(error));
      }
    },
  });
}

async function setDefault(accountId: string) {
  try {
    await api.authSetDefaultAccount(accountId);
    await api.authApplyToCodex(accountId);
    message.success("已切换当前订阅账号，Codex 将使用该账号");
    await refreshStatus();
  } catch (error) {
    message.error(String(error));
  }
}

onMounted(refreshStatus);
</script>

<template>
  <div>
    <div v-if="login" class="space-y-4">
      <div class="flex items-start justify-between gap-3">
        <div class="flex min-w-0 items-start gap-3">
          <span class="grid h-9 w-9 shrink-0 place-items-center rounded-xl bg-accent/10 text-accent">
            <PhShieldCheck class="h-[18px] w-[18px]" weight="bold" aria-hidden="true" />
          </span>
          <div class="min-w-0">
            <div class="text-sm font-semibold">ChatGPT 设备码登录</div>
            <p class="muted mt-0.5 text-xs">完成 ChatGPT 登录后，认证结果会自动回到这里。</p>
          </div>
        </div>
        <n-tag size="small" type="warning">等待授权</n-tag>
      </div>

      <div class="rounded-2xl bg-accent/6 p-4 shadow-[0_0_0_1px_var(--panel-ring)] dark:bg-accent/10">
        <div class="text-center">
          <div class="field-label">授权码：请在浏览器中输入此码</div>
          <div class="mt-1 flex items-center justify-center gap-2">
            <span class="mono whitespace-nowrap text-2xl font-bold tracking-[0.3em]">{{ login.user_code }}</span>
            <button
              type="button"
              class="grid h-8 w-8 shrink-0 place-items-center rounded-full text-accent transition-colors hover:bg-accent/10"
              title="复制授权码"
              aria-label="复制授权码"
              @click="copyUserCode"
            >
              <PhCopy class="h-4 w-4" weight="bold" aria-hidden="true" />
            </button>
          </div>
        </div>
        <div class="mt-3 border-t border-[var(--panel-border)] pt-3 text-center">
          <div class="muted text-xs">授权页面</div>
          <button
            type="button"
            class="mt-1 flex w-full min-w-0 items-center justify-center gap-1.5 text-sm font-medium text-accent hover:underline"
            :title="login.verification_uri"
            @click="openVerification"
          >
            <span class="truncate">{{ login.verification_uri }}</span>
            <PhArrowSquareOut class="h-4 w-4 shrink-0" weight="bold" aria-hidden="true" />
          </button>
        </div>
        <div class="mt-4 flex justify-center">
          <n-button size="small" quaternary @click="cancelLogin">取消登录</n-button>
        </div>
      </div>
    </div>

    <div v-else-if="status?.authenticated" class="space-y-4">
      <div class="flex items-start justify-between gap-3 rounded-2xl bg-success/10 p-3 shadow-[0_0_0_1px_rgba(52,199,89,0.16)]">
        <div class="flex min-w-0 items-start gap-3">
          <PhCheckCircle class="mt-2 h-6 w-6 shrink-0 text-success" weight="bold" aria-hidden="true" />
          <div class="min-w-0">
            <template v-if="status.external && status.accounts.length">
              <div class="text-sm font-semibold">ChatGPT 已认证</div>
              <p class="muted mt-0.5 text-xs">桌面端 Codex 与设备码登录均已连接。</p>
            </template>
            <template v-else-if="status.external">
              <div class="text-sm font-semibold">ChatGPT 桌面端已登录</div>
              <p class="muted mt-0.5 text-xs">来自 ChatGPT 桌面端的 Codex 登录状态。</p>
            </template>
            <template v-else>
              <div class="text-sm font-semibold">ChatGPT 设备码登录已生效</div>
              <p class="muted mt-0.5 text-xs">当前使用通过设备码登录的 ChatGPT 账号。</p>
            </template>
          </div>
        </div>
      </div>

      <div v-if="status.external" class="space-y-2">
        <div class="field-subtitle">ChatGPT 账号（桌面端 Codex）</div>
        <div
          class="flex items-center gap-3 rounded-xl bg-info/8 px-3 py-2.5 shadow-[0_0_0_1px_var(--panel-ring)] dark:bg-info/12"
        >
          <PhShieldCheck class="h-5 w-5 shrink-0 text-info" weight="bold" aria-hidden="true" />
          <div class="min-w-0 flex-1">
            <div class="text-sm font-semibold">ChatGPT 账号</div>
            <div class="mono muted truncate text-xs">{{ status.external.login }}</div>
          </div>
          <n-tag size="small" type="info">桌面端</n-tag>
        </div>
      </div>

      <div v-if="status.accounts.length" class="space-y-2">
        <div class="field-subtitle">ChatGPT 账号（设备码登录）</div>
        <p class="muted text-xs">通过设备码登录添加，可在 CGSwitch 中管理多个账号。</p>
        <div
          v-for="account in status.accounts"
          :key="account.id"
          class="flex items-center gap-3 rounded-xl px-3 py-2.5 shadow-[0_0_0_1px_var(--panel-ring)]"
          :class="account.is_default ? 'bg-[var(--selection-bg)]' : ''"
        >
          <PhUserCircle class="h-5 w-5 shrink-0 text-accent" weight="bold" aria-hidden="true" />
          <div class="min-w-0 flex-1">
            <div class="flex min-w-0 items-center gap-2">
              <span class="mono truncate text-sm font-medium">{{ account.login }}</span>
              <n-tag v-if="account.is_default" size="small" type="success">CGSwitch 默认</n-tag>
            </div>
          </div>
          <div class="flex shrink-0 gap-1.5">
            <n-button v-if="!account.is_default" size="small" secondary @click="setDefault(account.id)">设为当前</n-button>
            <n-button size="small" quaternary type="error" @click="removeAccount(account.id)">移除</n-button>
          </div>
        </div>
      </div>

      <n-button secondary :loading="busy" @click="startLogin">
        <template #icon>
          <PhPlus class="h-4 w-4" weight="bold" aria-hidden="true" />
        </template>
        添加其他账号
      </n-button>
    </div>

    <div v-else class="rounded-2xl border border-[var(--panel-border)] bg-black/2 p-4 dark:bg-white/4">
      <div class="flex items-start gap-3">
        <span class="grid h-9 w-9 shrink-0 place-items-center rounded-xl bg-accent/10 text-accent">
          <PhShieldCheck class="h-[18px] w-[18px]" weight="bold" aria-hidden="true" />
        </span>
        <div class="min-w-0">
          <div class="text-sm font-semibold">尚未连接 ChatGPT</div>
          <p class="muted mt-0.5 text-xs">登录后可管理多个 ChatGPT 账号。</p>
        </div>
      </div>
      <div class="mt-6">
        <n-button type="primary" :loading="busy" @click="startLogin">
          <template #icon>
            <PhArrowSquareOut class="h-4 w-4" weight="bold" aria-hidden="true" />
          </template>
          使用 ChatGPT 登录
        </n-button>
      </div>
    </div>

    <p v-if="loadError" class="muted mt-3 text-sm">{{ loadError }}</p>
  </div>
</template>
