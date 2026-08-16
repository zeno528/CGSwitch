<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import {
  NButton,
  NEmpty,
  NInput,
  NModal,
  NProgress,
  NTag,
  useDialog,
  useMessage,
} from "naive-ui";
import ProfileEdit from "../components/ProfileEdit.vue";
import ProfileCard from "../components/ProfileCard.vue";
import ProfileIconTile from "../components/ProfileIconTile.vue";
import { api } from "../api";
import type { AppState, ProfileSummary, RestartStage } from "../types";

const props = defineProps<{ state: AppState }>();
const emit = defineEmits<{ refresh: [] }>();

const message = useMessage();
const dialog = useDialog();
const busy = ref(false);
const restartStage = ref<RestartStage>("idle");
const restartMessage = ref("");
const modalVisible = ref(false);
const modalMode = ref<"capture" | "rename">("capture");
const profileName = ref("");
const editingProfile = ref<ProfileSummary | null>(null);
const creatingProfile = ref(false);
const modalProfile = ref<ProfileSummary | null>(null);

let unlisten: (() => void) | null = null;

const activeProfile = computed(() =>
  props.state.profiles.find((profile) => profile.id === props.state.active_profile_id) ?? null,
);

const progress = computed(() => {
  const values: Record<RestartStage, number> = {
    idle: 0,
    stopping: 18,
    waiting: 48,
    launching: 82,
    success: 100,
    error: 100,
  };
  return values[restartStage.value];
});

const stageText = computed(() => {
  const values: Record<RestartStage, string> = {
    idle: "空闲",
    stopping: "正在停止 Codex",
    waiting: "等待进程退出",
    launching: "正在启动 Codex",
    success: "重启成功",
    error: "重启失败",
  };
  return values[restartStage.value];
});

function openCapture() {
  modalMode.value = "capture";
  modalProfile.value = null;
  profileName.value = "";
  modalVisible.value = true;
}

function openRename(profile: ProfileSummary) {
  modalMode.value = "rename";
  modalProfile.value = profile;
  profileName.value = profile.name;
  modalVisible.value = true;
}

function blurActiveOnModalLeave() {
  (document.activeElement as HTMLElement | null)?.blur?.();
}

async function submitModal() {
  if (busy.value) return;
  busy.value = true;
  try {
    if (modalMode.value === "capture") {
      await api.captureProfile(profileName.value);
      message.success("配置档案已捕获");
    } else if (modalProfile.value) {
      await api.renameProfile(modalProfile.value.id, profileName.value);
      message.success("配置档案已重命名");
    }
    modalVisible.value = false;
    emit("refresh");
  } catch (error) {
    message.error(String(error));
  } finally {
    busy.value = false;
  }
}

async function applyProfile(profile: ProfileSummary) {
  if (busy.value) return;
  busy.value = true;
  try {
    await api.applyProfile(profile.id);
    message.success("模型配置已应用");
    if (props.state.settings.auto_restart) {
      await restart(true);
    }
    emit("refresh");
  } catch (error) {
    message.error(String(error));
  } finally {
    busy.value = false;
  }
}

async function removeProfile(profile: ProfileSummary) {
  dialog.error({
    title: "删除配置档案",
    content: `确定删除“${profile.name}”吗？删除后不可恢复。`,
    positiveText: "删除",
    negativeText: "取消",
    class: "delete-profile-dialog",
    positiveButtonProps: { type: "error" },
    onPositiveClick: async () => {
      try {
        await api.deleteProfile(profile.id);
        message.success("配置档案已删除");
        emit("refresh");
      } catch (error) {
        message.error(String(error));
      }
    },
  });
}

async function restart(force = false) {
  if (busy.value && !force) return;
  busy.value = true;
  restartStage.value = "stopping";
  restartMessage.value = "";
  try {
    await api.restartCodex();
    message.success("Codex 已重启");
    emit("refresh");
  } catch (error) {
    restartMessage.value = String(error);
    message.error(String(error));
  } finally {
    busy.value = false;
  }
}

onMounted(async () => {
  unlisten = await api.onRestartProgress((payload) => {
    restartStage.value = payload.stage;
    restartMessage.value = payload.message ?? "";
  });
});

onBeforeUnmount(() => {
  unlisten?.();
});
</script>

<template>
  <ProfileEdit
    v-if="editingProfile"
    :profile="editingProfile"
    @back="editingProfile = null"
    @changed="emit('refresh')"
  />
  <ProfileEdit
    v-else-if="creatingProfile"
    :profile="null"
    create
    @back="creatingProfile = false"
    @changed="emit('refresh')"
  />
  <section v-else class="mx-auto w-full max-w-none">
    <header class="flex flex-wrap items-end justify-between gap-4">
      <div>
        <h1 class="apple-title">配置档案</h1>
      </div>
      <div class="flex gap-2">
        <n-button @click="emit('refresh')">刷新</n-button>
        <n-button secondary :disabled="busy" @click="openCapture">捕获当前配置</n-button>
        <n-button type="primary" :disabled="busy" @click="creatingProfile = true">添加档案</n-button>
        <n-button secondary :disabled="busy" :loading="restartStage !== 'idle' && restartStage !== 'success' && restartStage !== 'error'" @click="restart(false)">重启 Codex</n-button>
      </div>
    </header>

    <div class="apple-group mt-7 px-5 py-4">
      <div class="flex flex-wrap items-center justify-between gap-5">
        <div class="flex min-w-0 items-center gap-3">
          <ProfileIconTile :name="activeProfile?.name ?? '未匹配'" :icon="activeProfile?.icon ?? null" />
          <div class="min-w-0">
            <div class="field-label">当前使用</div>
            <div class="mt-1 truncate text-lg font-semibold tracking-tight">
              {{ activeProfile?.name ?? "未匹配" }}
            </div>
          </div>
        </div>
        <div class="flex flex-wrap items-center gap-x-4 gap-y-2 text-sm">
          <span class="inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-xs font-medium transition-colors" :class="state.codex.running ? 'bg-[#34c759]/10 text-[#248a3d] dark:text-[#6ee7a0]' : 'bg-zinc-500/10 text-zinc-500'">
            <span class="relative flex h-2 w-2">
              <span v-if="state.codex.running" class="absolute inline-flex h-full w-full animate-ping rounded-full bg-[#34c759] opacity-60" />
              <span class="relative inline-flex h-2 w-2 rounded-full" :class="state.codex.running ? 'bg-[#34c759]' : 'bg-zinc-400'" />
            </span>
            Codex {{ state.codex.running ? "运行中" : "未运行" }}
          </span>
          <span v-if="state.settings.auto_restart" class="ml-1 border-l border-[var(--panel-border)] pl-3" title="应用配置后自动重启已开启" aria-label="应用配置后自动重启已开启">
            <span class="flex h-5 w-9 items-center rounded-full bg-[#007aff] p-[2px]" aria-hidden="true">
              <span class="ml-auto grid h-4 w-4 place-items-center rounded-full bg-white text-[#007aff]">
                <svg class="h-2.5 w-2.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" aria-hidden="true">
                  <path d="m7 12 3 3 7-7" stroke-linecap="round" stroke-linejoin="round" />
                </svg>
              </span>
            </span>
          </span>
        </div>
      </div>

      <transition name="apple-reveal">
        <div v-if="restartStage !== 'idle'" class="mt-4 border-t border-[var(--panel-border)] pt-4">
          <div class="flex items-center justify-between gap-3">
            <div class="font-semibold">重启进度</div>
            <n-tag size="small" :type="restartStage === 'error' ? 'error' : restartStage === 'success' ? 'success' : 'default'">
              {{ stageText }}
            </n-tag>
          </div>
          <n-progress class="mt-3" type="line" :percentage="progress" :status="restartStage === 'error' ? 'error' : restartStage === 'success' ? 'success' : 'default'" :show-indicator="false" />
          <p v-if="restartMessage" class="muted mt-3 text-sm">{{ restartMessage }}</p>
        </div>
      </transition>
    </div>

    <div class="mt-8">
      <div class="flex items-center justify-between">
        <h2 class="text-[15px] font-semibold tracking-tight">我的档案</h2>
      </div>
      <n-empty v-if="state.profiles.length === 0" description="还没有配置档案。可以添加内置官方档案，或先把 ~/.codex/config.toml 调整到目标状态，再点击“捕获当前配置”。" class="apple-group mt-3 py-14" />
      <template v-else>
        <div class="apple-group mt-3 divide-y divide-[var(--panel-border)]">
          <ProfileCard
            v-for="profile in state.profiles"
            :key="profile.id"
            :profile="profile"
            :active="profile.id === state.active_profile_id"
            :busy="busy"
            @apply="applyProfile(profile)"
            @rename="openRename(profile)"
            @edit="editingProfile = profile"
            @remove="removeProfile(profile)"
          />
        </div>
      </template>
    </div>

    <n-modal v-model:show="modalVisible" preset="card" class="max-w-[460px]" title="配置档案" @after-leave="blurActiveOnModalLeave">
      <div class="space-y-4">
        <p class="muted text-sm">
          {{ modalMode === "capture" ? "为当前 Codex 配置创建一个可回滚的档案。" : "输入新的配置档案名称。" }}
        </p>
        <n-input v-model:value="profileName" maxlength="50" show-count placeholder="例如：ZAI GLM 高推理" @keyup.enter="submitModal" />
        <div class="flex justify-end gap-2">
          <n-button @click="modalVisible = false">取消</n-button>
          <n-button type="primary" :loading="busy" @click="submitModal">保存</n-button>
        </div>
      </div>
    </n-modal>
  </section>
</template>
