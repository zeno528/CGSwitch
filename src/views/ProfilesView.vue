<script setup lang="ts">
import { computed, defineAsyncComponent, h, onBeforeUnmount, onMounted, ref, watch } from "vue";
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
import ProfileCard from "../components/ProfileCard.vue";
import TrashIcon from "../components/TrashIcon.vue";
import draggable from "vuedraggable";
import { api } from "../api";
import type { AppState, ManagedAccount, ProfileSummary, RestartStage } from "../types";
import { PhArrowClockwise, PhCamera, PhPlus } from "@phosphor-icons/vue";

// 编辑页按需加载：只在打开编辑/新建时拉取，避免把 CodeMirror/预设数据带进启动入口
const ProfileEdit = defineAsyncComponent(() => import("../components/ProfileEdit.vue"));

const props = defineProps<{ state: AppState; navReset: number }>();
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
const subscriptionAuthed = ref(false);
const subscriptionAccount = ref<string | null>(null);
const subscriptionSource = ref<"desktop" | "oauth" | null>(null);

watch(
  () => props.navReset,
  () => {
    editingProfile.value = null;
    creatingProfile.value = false;
    modalVisible.value = false;
    modalProfile.value = null;
  },
);

// 手动排序：vuedraggable（SortableJS）实时重排，结束后持久化
function onDragStart() {
  document.body.classList.add("drag-active");
}

async function persistOrder() {
  try {
    await api.reorderProfiles(props.state.profiles.map((item) => item.id));
  } catch (error) {
    message.error(String(error));
    emit("refresh");
  } finally {
    document.body.classList.remove("drag-active");
  }
}
const authAccounts = ref<ManagedAccount[]>([]);

let unlisten: (() => void) | null = null;

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

let restartCollapseTimer: ReturnType<typeof setTimeout> | undefined;

// 重启进度卡入场/收起：rAF 驱动 + 每帧高度/外边距取整。
// 不用 CSS 高度过渡——它收尾会落到小数像素，列表在亚像素上重绘产生顿挫（实测收尾步进 3.4px→1.9px 全小数）；
// 每帧取整后列表始终落在整数像素上，收尾平滑。
// 内边距也必须一起动画：border-box 下盒子高度不能小于上下内边距之和（py-3=24px），
// 不压内边距的话高度会被浏览器钳制在 24px，移除瞬间列表再跳 24px（实测复现）。
const RESTART_CARD_DURATION = 360;

function animateRestartCard(el: Element, entering: boolean, done: () => void) {
  const node = el as HTMLElement;
  if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
    done();
    return;
  }
  const natural = node.scrollHeight;
  const margin = parseFloat(getComputedStyle(node).marginTop) || 24;
  const padTop = parseFloat(getComputedStyle(node).paddingTop) || 0;
  const padBottom = parseFloat(getComputedStyle(node).paddingBottom) || 0;
  const fromH = entering ? 0 : natural;
  const toH = entering ? natural : 0;
  const fromM = entering ? 0 : margin;
  const toM = entering ? margin : 0;
  node.style.overflow = "hidden";
  node.style.opacity = entering ? "0" : "1";
  const start = performance.now();
  const tick = (now: number) => {
    const progress = Math.min(1, (now - start) / RESTART_CARD_DURATION);
    // easeOutQuart：与全站曲线同属强 ease-out，收尾更快、尾部不留拖影
    const eased = 1 - (1 - progress) ** 4;
    node.style.height = `${Math.round(fromH + (toH - fromH) * eased)}px`;
    node.style.marginTop = `${Math.round(fromM + (toM - fromM) * eased)}px`;
    node.style.paddingTop = `${Math.round(padTop * (entering ? eased : 1 - eased))}px`;
    node.style.paddingBottom = `${Math.round(padBottom * (entering ? eased : 1 - eased))}px`;
    node.style.opacity = `${entering ? eased : 1 - eased}`;
    if (progress < 1) {
      requestAnimationFrame(tick);
      return;
    }
    // 进场结束后复位内联样式（卡片仍驻留）；出场不复位——元素随后即被移除，
    // 复位会把它弹回完整高度+外边距一帧，列表先下移再上弹，形成收尾顿挫（实测末帧跳 24px）
    if (entering) {
      node.style.cssText = "";
    }
    done();
  };
  requestAnimationFrame(tick);
}

function onRestartCardEnter(el: Element, done: () => void) {
  animateRestartCard(el, true, done);
}

function onRestartCardLeave(el: Element, done: () => void) {
  animateRestartCard(el, false, done);
}

watch(restartStage, (stage) => {
  if (restartCollapseTimer !== undefined) {
    clearTimeout(restartCollapseTimer);
    restartCollapseTimer = undefined;
  }
  if (stage === "success") {
    // 进度跑完停留片刻让用户看到“重启成功”，再自动收缩回空闲
    restartCollapseTimer = setTimeout(() => {
      restartStage.value = "idle";
      restartCollapseTimer = undefined;
    }, 1200);
  }
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

function closeEdit() {
  editingProfile.value = null;
  // 编辑页可能读取过外部修改后的 live 配置，返回列表时重新拉取状态
  emit("refresh");
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
      message.success("已捕获并设为使用中");
    } else if (modalProfile.value) {
      await api.renameProfile(modalProfile.value.id, profileName.value);
      message.success("供应商已重命名");
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
    title: "删除供应商",
    content: () =>
      h("span", [
        "确定删除“",
        h("strong", { class: "font-semibold" }, profile.name),
        "”吗？删除后不可恢复。",
      ]),
    positiveText: "删除",
    negativeText: "取消",
    class: "delete-profile-dialog",
    icon: () => h(TrashIcon),
    positiveButtonProps: { type: "error" },
    onPositiveClick: async () => {
      try {
        await api.deleteProfile(profile.id);
        message.success("供应商已删除");
        emit("refresh");
      } catch (error) {
        message.error(String(error));
      }
    },
  });
}

async function duplicateProfile(profile: ProfileSummary) {
  if (busy.value) return;
  busy.value = true;
  try {
    const copy = await api.duplicateProfile(profile.id);
    message.success(`已复制为「${copy.name}」`);
    emit("refresh");
  } catch (error) {
    message.error(String(error));
  } finally {
    busy.value = false;
  }
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
  try {
    const status = await api.authGetStatus();
    subscriptionAuthed.value = status.authenticated;
    authAccounts.value = status.accounts;
    subscriptionSource.value = status.external ? "desktop" : status.accounts.length ? "oauth" : null;
    // 桌面端当前认证才是实际生效来源；CGswitch 默认账号仅作为没有外部认证时的回退。
    subscriptionAccount.value =
      status.external?.login ??
      status.accounts.find((account) => account.id === status.default_account_id)?.login ??
      null;
  } catch {
    subscriptionAuthed.value = false;
    subscriptionSource.value = null;
  }
});

function boundAccountLogin(profile: ProfileSummary): string | null {
  return (
    authAccounts.value.find((account) => account.id === profile.account_id)?.login ?? null
  );
}

onBeforeUnmount(() => {
  unlisten?.();
  if (restartCollapseTimer !== undefined) clearTimeout(restartCollapseTimer);
});
</script>

<template>
  <ProfileEdit
    v-if="editingProfile"
    :profile="editingProfile"
    @back="closeEdit"
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
    <header class="flex flex-wrap items-center justify-between gap-4 pb-2">
      <div class="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-2 text-sm">
        <span class="inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs font-medium transition-colors" :class="state.codex.running ? 'border-success/25 bg-success/10 text-[#248a3d] dark:border-success/30 dark:bg-success/10 dark:text-[#6ee7a0]' : 'border-[var(--panel-border)] bg-black/4 text-zinc-500 dark:bg-white/6'">
          <span class="relative flex h-2 w-2">
            <span v-if="state.codex.running" class="absolute inline-flex h-full w-full animate-ping rounded-full bg-success opacity-50" />
            <span class="relative inline-flex h-2 w-2 rounded-full shadow-[0_0_6px_1px_rgba(52,199,89,0.45)]" :class="state.codex.running ? 'bg-success' : 'bg-zinc-400 shadow-none'" />
          </span>
          Codex {{ state.codex.running ? "运行中" : "未运行" }}
        </span>
      </div>
      <div class="flex flex-wrap items-center gap-2">
        <n-button quaternary :disabled="busy" :loading="restartStage !== 'idle' && restartStage !== 'success' && restartStage !== 'error'" title="重启 Codex" @click="restart(false)">
          <template #icon>
            <PhArrowClockwise class="h-4 w-4" weight="bold" aria-hidden="true" />
          </template>
          重启 Codex
        </n-button>
        <n-button quaternary :disabled="busy" title="捕获当前配置" aria-label="捕获当前配置" @click="openCapture">
          <template #icon>
            <PhCamera class="h-4 w-4" weight="bold" aria-hidden="true" />
          </template>
        </n-button>
        <n-button type="primary" :disabled="busy" @click="creatingProfile = true">
          <template #icon>
            <PhPlus class="h-4 w-4" weight="bold" aria-hidden="true" />
          </template>
          添加供应商
        </n-button>
      </div>
    </header>

    <transition :css="false" @enter="onRestartCardEnter" @leave="onRestartCardLeave">
      <div v-if="restartStage !== 'idle'" class="apple-group mt-[var(--gap-page)] px-4 py-3">
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

    <div class="mt-[var(--gap-page)]">
      <n-empty v-if="state.profiles.length === 0" description="还没有供应商配置。可以添加内置官方供应商，或先把 ~/.codex/config.toml 调整到目标状态，再点击“捕获当前配置”。" class="apple-group py-14" />
      <template v-else>
        <draggable
          tag="div"
          class="profile-list apple-group relative will-change-transform"
          :component-data="{ name: 'profile-list' }"
          :list="state.profiles"
          item-key="id"
          handle=".drag-handle"
          :animation="250"
          :force-fallback="true"
          fallback-on-body="true"
          :fallback-tolerance="5"
          ghost-class="drag-ghost"
          drag-class="drag-dragging"
          @start="onDragStart"
          @end="persistOrder"
        >
          <template #item="{ element: profile }">
            <ProfileCard
              :profile="profile"
              :active="profile.id === state.active_profile_id"
              :busy="busy"
              :subscription-authed="subscriptionAuthed"
              :subscription-account="subscriptionAccount"
              :subscription-source="subscriptionSource"
              :bound-account="boundAccountLogin(profile)"
              :balance-cache="state.balance_cache"
              @apply="applyProfile(profile)"
              @rename="openRename(profile)"
              @edit="editingProfile = profile"
              @remove="removeProfile(profile)"
              @duplicate="duplicateProfile(profile)"
            />
          </template>
        </draggable>
      </template>
    </div>

    <n-modal v-model:show="modalVisible" preset="card" class="max-w-[460px]" :title="modalMode === 'capture' ? '保存当前配置快照' : '重命名供应商'" @after-leave="blurActiveOnModalLeave">
      <div class="space-y-4">
        <p class="muted text-sm">
          {{ modalMode === "capture" ? "为当前 Codex 配置创建快照，切换供应商后可一键恢复。" : "输入新的供应商名称。" }}
        </p>
        <n-input v-model:value="profileName" maxlength="50" show-count placeholder="例如：DeepSeek 日常" @keyup.enter="submitModal" />
        <div class="dialog-actions flex justify-end gap-2">
          <n-button @click="modalVisible = false">取消</n-button>
          <n-button type="primary" :loading="busy" @click="submitModal">保存</n-button>
        </div>
      </div>
    </n-modal>
  </section>
</template>
