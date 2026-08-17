<script setup lang="ts">
import { onMounted, ref } from "vue";
import { NButton, NTag, useMessage } from "naive-ui";
import { api } from "../api";
import type { AuthStatus, DeviceCodeResponse } from "../types";

const message = useMessage();
const status = ref<AuthStatus | null>(null);
const loadError = ref("");
const busy = ref(false);
const login = ref<DeviceCodeResponse | null>(null);
const polling = ref(false);

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
    message.error(String(error));
    busy.value = false;
  }
}

async function poll() {
  const current = login.value;
  if (!current) return;
  polling.value = true;
  try {
    const deadline = Date.now() + current.expires_in * 1000;
    while (Date.now() < deadline) {
      const account = await api.authPollForAccount(current.device_code);
      if (account) {
        login.value = null;
        await refreshStatus();
        try {
          await api.authApplyToCodex(account.id);
          message.success("ChatGPT 账号已登录，Codex 将使用订阅额度");
        } catch (error) {
          message.error(`账号已登录，但写入认证失败：${String(error)}`);
        }
        break;
      }
      await new Promise((resolve) => setTimeout(resolve, current.interval * 1000));
    }
  } catch (error) {
    message.error(String(error));
    login.value = null;
  } finally {
    polling.value = false;
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
  polling.value = false;
  busy.value = false;
}

async function removeAccount(accountId: string) {
  try {
    await api.authRemoveAccount(accountId);
    message.success("账号已移除");
    await refreshStatus();
  } catch (error) {
    message.error(String(error));
  }
}

async function setDefault(accountId: string) {
  try {
    await api.authSetDefaultAccount(accountId);
    message.success("已切换当前订阅账号");
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
      <p class="muted text-sm">在浏览器打开下面的地址，输入验证码完成授权：</p>
      <div class="rounded-xl bg-black/4 p-4 dark:bg-white/6">
        <div class="flex items-center justify-center gap-3">
          <span class="mono whitespace-nowrap text-2xl font-bold tracking-[0.3em]">{{ login.user_code }}</span>
          <button
            type="button"
            class="grid h-8 w-8 shrink-0 place-items-center rounded-lg text-[#007aff] transition-colors hover:bg-[#007aff]/10"
            title="复制授权码"
            aria-label="复制授权码"
            @click="copyUserCode"
          >
            <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <rect x="9" y="9" width="12" height="12" rx="2" />
              <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
            </svg>
          </button>
        </div>
        <button
          type="button"
          class="mono mt-3 block w-full break-all text-center text-[#007aff] hover:underline"
          @click="openVerification"
        >
          {{ login.verification_uri }}
        </button>
      </div>
      <div class="flex items-center justify-between">
        <n-button size="small" secondary @click="openVerification">重新打开浏览器</n-button>
        <n-button size="small" quaternary @click="cancelLogin">取消</n-button>
      </div>
      <p class="muted text-xs">{{ polling ? "正在等待授权…" : "即将打开浏览器" }}</p>
    </div>

    <div v-else-if="status?.authenticated" class="space-y-3">
      <p class="muted text-sm">ChatGPT 官方订阅已认证，添加 ChatGPT 供应商时无需再输入密钥。</p>
      <div
        v-for="account in status.accounts"
        :key="account.id"
        class="flex items-center justify-between gap-3 rounded-xl shadow-[0_0_0_1px_var(--panel-ring)] px-3 py-2.5"
      >
        <div class="flex min-w-0 items-center gap-2">
          <span class="mono truncate">{{ account.login }}</span>
          <n-tag v-if="account.is_default" size="small" type="success">默认</n-tag>
        </div>
        <n-button v-if="!account.is_default" size="small" secondary @click="setDefault(account.id)">设为当前</n-button>
        <n-button size="small" quaternary type="error" @click="removeAccount(account.id)">移除</n-button>
      </div>
      <n-button secondary :loading="busy" @click="startLogin">
        <template #icon>
          <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" aria-hidden="true">
            <path d="M12 5v14M5 12h14" />
          </svg>
        </template>
        添加账号
      </n-button>
    </div>

    <div v-else class="space-y-3">
      <p class="muted text-sm">
        官方 ChatGPT 订阅使用浏览器登录认证，无需 API 密钥。认证一次后，所有 ChatGPT 供应商共用该账号。
      </p>
      <n-button type="primary" :loading="busy" @click="startLogin">登录 ChatGPT</n-button>
    </div>

    <p v-if="loadError" class="muted mt-3 text-sm">{{ loadError }}</p>
  </div>
</template>
