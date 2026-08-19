<script setup lang="ts">
import { ref, watch } from "vue";
import { NButton, NModal } from "naive-ui";
import type { McpSyncDiffEntry, McpSyncPreview } from "../types";
import { mcpTransportText } from "../utils";

type SyncDirection = "live-to-db" | "db-to-live";

const props = defineProps<{
  show: boolean;
  preview: McpSyncPreview | null;
  /** live 配置无法解析时的错误文本；非空进入“仅可从数据库恢复”降级模式。 */
  previewError: string;
  busy: boolean;
}>();

const emit = defineEmits<{
  "update:show": [value: boolean];
  apply: [direction: SyncDirection];
}>();

// 展开明细的服务器名；预览数据更新后整体收起
const expandedNames = ref<Set<string>>(new Set());
watch(
  () => props.preview,
  () => {
    expandedNames.value = new Set();
  },
);

// 记录被点中的方向，只让那个按钮转 loading；同步结束后父组件把 busy 置回 false
const clickedDirection = ref<SyncDirection | null>(null);
watch(
  () => props.busy,
  (busy) => {
    if (!busy) clickedDirection.value = null;
  },
);

function toggleExpand(name: string) {
  const next = new Set(expandedNames.value);
  if (next.has(name)) {
    next.delete(name);
  } else {
    next.add(name);
  }
  expandedNames.value = next;
}

function apply(direction: SyncDirection) {
  clickedDirection.value = direction;
  emit("apply", direction);
}

function onShowChange(value: boolean) {
  if (props.busy) return;
  emit("update:show", value);
}

// 建模字段的中文名（与 Rust 侧 mcp_sync_preview 输出的字段一一对应）
const fieldLabels: Record<string, string> = {
  enabled: "启用状态",
  startup_timeout_sec: "启动超时秒",
  tool_timeout_sec: "工具超时秒",
  command: "启动命令",
  args: "启动参数",
  env: "环境变量",
  url: "服务地址",
  bearer_token_env_var: "令牌环境变量",
  http_headers: "HTTP 头",
  env_http_headers: "环境变量 HTTP 头",
};

function fieldValueText(value: unknown) {
  if (value === null) return "未设置";
  if (typeof value === "string") return value;
  return JSON.stringify(value);
}

function kindText(entry: McpSyncDiffEntry) {
  if (entry.kind === "live_only") return "仅配置文件";
  if (entry.kind === "db_only") return "仅数据库";
  return entry.unmodeled_only ? "仅格式差异" : "内容不同";
}

function kindChipClass(entry: McpSyncDiffEntry) {
  if (entry.kind === "changed" && !entry.unmodeled_only) {
    return "apple-chip chip-danger";
  }
  return "apple-chip chip-warn";
}

function transportTextOf(entry: McpSyncDiffEntry) {
  return mcpTransportText(entry.live_spec ?? entry.db_spec);
}
</script>

<template>
  <n-modal
    :show="show"
    preset="card"
    class="max-w-[560px]"
    title="MCP 同步差异"
    @update:show="onShowChange"
  >
    <!-- 降级模式：live 无法解析，无法对比，只能从数据库镜像恢复 -->
    <div v-if="previewError" class="space-y-3">
      <p class="muted text-sm">{{ previewError }}</p>
      <p class="muted text-sm">
        配置文件当前无法解析，无法对比差异；可从数据库镜像恢复（写前自动备份原文件）。
      </p>
    </div>
    <!-- 正常模式：body 只放摘要与差异列表（列表自身滚动），操作区固定在卡片 footer -->
    <div v-else-if="preview" class="space-y-3">
      <p class="muted text-sm">
        配置文件 <span class="font-semibold text-accent">{{ preview.live_count }}</span> 台 ·
        数据库镜像 <span class="font-semibold text-success">{{ preview.db_count }}</span> 台 ·
        <span class="font-semibold text-[var(--warning)]">{{ preview.entries.length }}</span>
        项差异
      </p>
      <div class="max-h-[60vh] space-y-2 overflow-y-auto pr-1">
        <div v-for="entry in preview.entries" :key="entry.name">
          <button
            type="button"
            class="apple-list-row apple-list-row--outlined w-full text-left transition-colors hover:bg-black/4 dark:hover:bg-white/6"
            @click="toggleExpand(entry.name)"
          >
            <span class="flex min-w-0 items-center gap-2">
              <span :class="kindChipClass(entry)" class="shrink-0">{{ kindText(entry) }}</span>
              <span class="truncate text-[var(--font-size-base)] font-semibold">{{ entry.name }}</span>
              <span class="shrink-0 rounded-md bg-black/5 px-1.5 py-px text-[10px] font-medium tracking-wide text-zinc-500 dark:bg-white/10 dark:text-zinc-400">
                {{ transportTextOf(entry) }}
              </span>
            </span>
            <span class="muted shrink-0 text-xs">{{ expandedNames.has(entry.name) ? "收起" : "明细" }}</span>
          </button>
          <div
            v-if="expandedNames.has(entry.name)"
            class="mono mt-1 space-y-1 rounded-[var(--radius-control-sm)] bg-black/4 p-3 text-[11px] leading-relaxed break-all dark:bg-white/6"
          >
            <div v-if="entry.changed_fields.length" class="space-y-1">
              <div v-for="diff in entry.changed_fields" :key="diff.field">
                {{ fieldLabels[diff.field] ?? diff.field }}：{{ fieldValueText(diff.live) }} →
                {{ fieldValueText(diff.db) }}
              </div>
            </div>
            <p v-else-if="entry.unmodeled_only">建模字段全部相同，差异只在注释 / 格式 / 未建模键。</p>
            <p v-else class="whitespace-pre-wrap">{{ entry.live_toml ?? entry.db_toml }}</p>
          </div>
        </div>
      </div>
    </div>

    <template #footer>
      <div class="space-y-2">
        <p v-if="!previewError" class="muted text-xs">
          选择一种方向覆盖另一侧；写回配置文件以数据库为准，更新数据库以 config.toml 为准，写回前会自动备份原文件。
        </p>
        <div class="dialog-actions grid grid-cols-[auto_minmax(0,1fr)_minmax(0,1fr)] items-center gap-2">
          <n-button class="shrink-0" :disabled="busy" @click="onShowChange(false)">取消</n-button>
          <n-button
            v-if="!previewError"
            secondary
            :disabled="busy"
            :loading="busy && clickedDirection === 'db-to-live'"
            @click="apply('db-to-live')"
          >
            写回配置文件
          </n-button>
          <n-button
            type="primary"
            :disabled="busy"
            :loading="busy && clickedDirection === 'live-to-db'"
            @click="apply(previewError ? 'db-to-live' : 'live-to-db')"
          >
            {{ previewError ? "从数据库恢复" : "更新数据库" }}
          </n-button>
        </div>
      </div>
    </template>
  </n-modal>
</template>
