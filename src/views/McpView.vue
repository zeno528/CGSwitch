<script setup lang="ts">
import { defineAsyncComponent, h, onActivated, onMounted, ref, watch } from "vue";
import { NButton, NEmpty, useDialog, useMessage } from "naive-ui";
import AppSwitch from "../components/AppSwitch.vue";
import TrashIcon from "../components/TrashIcon.vue";
import McpIcon from "../components/McpIcon.vue";
import { api } from "../api";
import type { McpServerSpec } from "../types";
import { useWindowActivation } from "../composables/useWindowActivation";
import {
  PhArrowCircleDown,
  PhArrowCircleUp,
  PhCircleDashed,
  PhGlobe,
  PhNotePencil,
  PhPlus,
  PhTerminalWindow,
} from "@phosphor-icons/vue";

// 编辑页按需加载：只在打开编辑/新建时拉取
const McpEdit = defineAsyncComponent(() => import("../components/McpEdit.vue"));

const props = defineProps<{ navReset: number }>();

const message = useMessage();
const dialog = useDialog();
const servers = ref<McpServerSpec[]>([]);
const loadError = ref("");
const loaded = ref(false);
const editingServer = ref<McpServerSpec | null>(null);
const creatingServer = ref(false);
const togglingName = ref("");
const syncing = ref(false);

watch(
  () => props.navReset,
  () => {
    editingServer.value = null;
    creatingServer.value = false;
  },
);

async function refresh() {
  try {
    servers.value = await api.listMcpServers();
    loadError.value = "";
  } catch (error) {
    loadError.value = String(error);
  } finally {
    loaded.value = true;
  }
}

onMounted(refresh);
onActivated(() => {
  if (loaded.value) void refresh();
});
// Codex CLI / 桌面版可能在外部改 config.toml，窗口激活时同步一次列表
useWindowActivation({
  onActive: () => {
    if (loaded.value) void refresh();
  },
});

type Transport = "http" | "stdio" | "unknown";

function transportOf(server: McpServerSpec): Transport {
  if (server.url) return "http";
  if (server.command) return "stdio";
  return "unknown";
}

function transportIconOf(server: McpServerSpec) {
  const map: Record<Transport, typeof PhGlobe> = {
    http: PhGlobe,
    stdio: PhTerminalWindow,
    unknown: PhCircleDashed,
  };
  return map[transportOf(server)];
}

function transportTextOf(server: McpServerSpec): string {
  const map: Record<Transport, string> = {
    http: "HTTP",
    stdio: "STDIO",
    unknown: "未知",
  };
  return map[transportOf(server)];
}

function metaOf(server: McpServerSpec): string {
  if (server.command) return [server.command, ...server.args.slice(0, 2)].join(" ");
  return server.url ?? "";
}

function closeEdit() {
  editingServer.value = null;
  // 编辑页可能读取过外部修改后的 live 配置，返回列表时重新拉取
  void refresh();
}

// 行内启停开关：乐观更新，失败回滚（enabled: null = 键不存在 = Codex 默认启用）
async function toggleEnabled(server: McpServerSpec, enabled: boolean) {
  if (togglingName.value) return;
  togglingName.value = server.name;
  const previous = server.enabled;
  server.enabled = enabled ? null : false;
  try {
    await api.saveMcpServer(server.name, { ...server, enabled: enabled ? null : false });
  } catch (error) {
    server.enabled = previous;
    message.error(String(error));
  } finally {
    togglingName.value = "";
  }
}

function removeServer(server: McpServerSpec) {
  dialog.error({
    title: "删除 MCP 服务器",
    content: () =>
      h("span", [
        "确定删除“",
        h("strong", { class: "font-semibold" }, server.name),
        "”吗？~/.codex/config.toml 中对应的配置段将被移除。",
      ]),
    positiveText: "删除",
    negativeText: "取消",
    class: "delete-profile-dialog",
    icon: () => h(TrashIcon),
    positiveButtonProps: { type: "error" },
    onPositiveClick: async () => {
      try {
        await api.deleteMcpServer(server.name);
        message.success("MCP 服务器已删除");
        void refresh();
      } catch (error) {
        message.error(String(error));
      }
    },
  });
}

// 显式双向同步：配置文件 → 数据库（强制对齐，含清空已移除条目）
function importFromLive() {
  if (syncing.value) return;
  dialog.warning({
    title: "从配置文件导入数据库",
    content:
      "把当前 ~/.codex/config.toml 里的 MCP 段强制镜像进数据库（覆盖现有镜像，live 里没有的条目会从数据库清掉）。",
    positiveText: "导入",
    negativeText: "取消",
    onPositiveClick: async () => {
      syncing.value = true;
      try {
        const count = await api.importMcpFromLive();
        message.success(`已从配置文件导入 ${count} 台服务器到数据库`);
        void refresh();
      } catch (error) {
        message.error(String(error));
      } finally {
        syncing.value = false;
      }
    },
  });
}

// 显式双向同步：数据库 → 配置文件（配置损坏/段丢失后的恢复）
function restoreToLive() {
  if (syncing.value) return;
  dialog.warning({
    title: "从数据库恢复到配置文件",
    content:
      "把数据库里的 MCP 镜像写回 ~/.codex/config.toml（覆盖现有 MCP 段，其余内容不动；配置文件无法解析时将按镜像重建，写前自动备份原文件）。",
    positiveText: "恢复",
    negativeText: "取消",
    onPositiveClick: async () => {
      syncing.value = true;
      try {
        const count = await api.restoreMcpFromDatabase();
        message.success(`已恢复 ${count} 台服务器到配置文件`);
        void refresh();
      } catch (error) {
        message.error(String(error));
      } finally {
        syncing.value = false;
      }
    },
  });
}
</script>

<template>
  <McpEdit v-if="editingServer" :server="editingServer" @back="closeEdit" />
  <McpEdit v-else-if="creatingServer" :server="null" create @back="creatingServer = false" />
  <section v-else class="mx-auto w-full max-w-none">
    <header class="apple-page-bar apple-page-bar--roomy apple-page-bar--sticky flex-wrap justify-between gap-4">
      <div class="flex min-w-0 items-center gap-2.5">
        <span class="settings-icon-tile grid h-9 w-9 shrink-0 place-items-center rounded-[10px] text-accent">
          <McpIcon class="h-[22px] w-[22px]" />
        </span>
        <div class="min-w-0">
          <div class="flex items-center gap-2">
            <div class="apple-title">MCP 服务器管理</div>
            <span v-if="loaded" class="apple-chip" :aria-label="servers.length + ' 台服务器'">{{ servers.length }}</span>
          </div>
        </div>
      </div>
      <div class="flex flex-wrap items-center gap-2">
        <div class="apple-page-action relative">
          <button
            type="button"
            class="apple-icon-button text-zinc-500 hover:bg-[var(--sidebar-bg)] hover:text-accent dark:text-zinc-400"
            :disabled="syncing"
            title="从配置导入"
            aria-label="从配置导入"
            @click="importFromLive"
          >
            <PhArrowCircleDown class="h-4 w-4" weight="bold" aria-hidden="true" />
          </button>
          <span class="apple-sidebar-flyout" aria-hidden="true">从配置导入</span>
        </div>
        <div class="apple-page-action relative">
          <button
            type="button"
            class="apple-icon-button text-zinc-500 hover:bg-[var(--sidebar-bg)] hover:text-accent dark:text-zinc-400"
            :disabled="syncing"
            title="恢复到配置"
            aria-label="恢复到配置"
            @click="restoreToLive"
          >
            <PhArrowCircleUp class="h-4 w-4" weight="bold" aria-hidden="true" />
          </button>
          <span class="apple-sidebar-flyout" aria-hidden="true">恢复到配置</span>
        </div>
        <n-button type="primary" @click="creatingServer = true">
          <template #icon>
            <PhPlus class="h-4 w-4" weight="bold" aria-hidden="true" />
          </template>
          添加服务器
        </n-button>
      </div>
    </header>

    <p v-if="loadError" class="muted mt-4 text-sm">
      {{ loadError }}<span v-if="loaded">配置文件无法解析时，可点「恢复到配置」用数据库镜像修复。</span>
    </p>

    <div class="mt-[var(--gap-page)]">
      <n-empty
        v-if="loaded && servers.length === 0"
        description="还没有 MCP 服务器。点击“添加服务器”把第一个 MCP 服务写进 config.toml。"
        class="apple-group py-14"
      />
      <div v-else-if="servers.length" class="space-y-2">
        <div
          v-for="server in servers"
          :key="server.name"
          class="apple-list-row"
        >
          <div class="flex min-w-0 items-center gap-2.5">
            <span class="settings-icon-tile grid h-8 w-8 shrink-0 place-items-center rounded-lg text-accent">
              <component :is="transportIconOf(server)" class="h-4 w-4" weight="bold" aria-hidden="true" />
            </span>
            <div class="min-w-0">
              <div class="flex items-center gap-2">
                <span class="truncate text-[var(--font-size-base)] font-semibold">{{ server.name }}</span>
                <span class="shrink-0 rounded-md bg-black/5 px-1.5 py-px text-[10px] font-medium tracking-wide text-zinc-500 dark:bg-white/10 dark:text-zinc-400">
                  {{ transportTextOf(server) }}
                </span>
              </div>
              <div class="mono muted truncate text-[11px]">{{ metaOf(server) }}</div>
            </div>
          </div>
          <div class="flex shrink-0 items-center gap-1.5">
            <AppSwitch
              size="small"
              :value="server.enabled !== false"
              :aria-label="`启用 ${server.name}`"
              @update:value="toggleEnabled(server, $event)"
            />
            <button
              type="button"
              class="apple-icon-button text-zinc-600 hover:bg-[var(--sidebar-bg)] hover:text-accent dark:text-zinc-300"
              title="编辑"
              :aria-label="'编辑 ' + server.name"
              @click="editingServer = server"
            >
              <PhNotePencil class="h-4 w-4" weight="bold" aria-hidden="true" />
            </button>
            <button
              type="button"
              class="apple-icon-button text-[var(--danger)]/70 hover:bg-[var(--danger)]/10 hover:text-[var(--danger)]"
              title="删除"
              :aria-label="'删除 ' + server.name"
              @click="removeServer(server)"
            >
              <TrashIcon />
            </button>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>
