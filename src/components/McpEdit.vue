<script setup lang="ts">
import { computed, ref } from "vue";
import { NButton, NCollapse, NCollapseItem, NDynamicInput, NInput, NInputNumber, NSelect, useMessage } from "naive-ui";
import { api } from "../api";
import type { McpServerSpec } from "../types";
import { PhArrowLeft, PhFloppyDisk, PhInfo } from "@phosphor-icons/vue";

const props = defineProps<{ server: McpServerSpec | null; create?: boolean }>();
const emit = defineEmits<{ back: [] }>();

const message = useMessage();
const creating = computed(() => props.create);

const name = ref(props.server?.name ?? "");
const transport = ref<"stdio" | "http">(props.server?.url ? "http" : "stdio");
const command = ref(props.server?.command ?? "");
const argsText = ref((props.server?.args ?? []).join("\n"));
const url = ref(props.server?.url ?? "");
const bearer = ref(props.server?.bearer_token_env_var ?? "");
const startupTimeout = ref<number | null>(props.server?.startup_timeout_sec ?? null);
const toolTimeout = ref<number | null>(props.server?.tool_timeout_sec ?? null);

interface KVPair {
  key: string;
  value: string;
}

function recordToPairs(record: Record<string, string>): KVPair[] {
  return Object.entries(record).map(([key, value]) => ({ key, value }));
}

function pairsToRecord(pairs: KVPair[]): Record<string, string> {
  const record: Record<string, string> = {};
  for (const pair of pairs) {
    const key = pair.key.trim();
    if (key) record[key] = pair.value.trim();
  }
  return record;
}

const envPairs = ref<KVPair[]>(recordToPairs(props.server?.env ?? {}));
const headerPairs = ref<KVPair[]>(recordToPairs(props.server?.http_headers ?? {}));
const envHeaderPairs = ref<KVPair[]>(recordToPairs(props.server?.env_http_headers ?? {}));

const transportOptions = [
  { label: "本地进程 (STDIO)", value: "stdio" },
  { label: "远程服务 (HTTP)", value: "http" },
];

const saving = ref(false);

function argsList(): string[] {
  return argsText.value
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
}

async function save() {
  if (saving.value) return;
  const trimmedName = name.value.trim();
  if (!/^[A-Za-z0-9_-]+$/.test(trimmedName)) {
    message.error("名称只能包含字母、数字、下划线和连字符");
    return;
  }
  if (transport.value === "stdio" && !command.value.trim()) {
    message.error("请填写启动命令");
    return;
  }
  if (transport.value === "http") {
    if (!url.value.trim()) {
      message.error("请填写服务地址");
      return;
    }
    if (!/^https?:\/\//.test(url.value.trim())) {
      message.error("服务地址必须以 http:// 或 https:// 开头");
      return;
    }
  }
  if (startupTimeout.value !== null && startupTimeout.value <= 0) {
    message.error("启动超时必须为正数（秒）");
    return;
  }
  if (toolTimeout.value !== null && toolTimeout.value <= 0) {
    message.error("工具调用超时必须为正数（秒）");
    return;
  }

  saving.value = true;
  try {
    // 只提交当前传输类型的建模字段；表单未涵盖的键（tools.*、cwd 等）由后端原样保留
    const spec: McpServerSpec = {
      name: trimmedName,
      enabled: props.server?.enabled ?? null,
      startup_timeout_sec: startupTimeout.value,
      tool_timeout_sec: toolTimeout.value,
      command: transport.value === "stdio" ? command.value : null,
      args: transport.value === "stdio" ? argsList() : [],
      env: transport.value === "stdio" ? pairsToRecord(envPairs.value) : {},
      url: transport.value === "http" ? url.value : null,
      bearer_token_env_var: transport.value === "http" ? bearer.value : null,
      http_headers: transport.value === "http" ? pairsToRecord(headerPairs.value) : {},
      env_http_headers: transport.value === "http" ? pairsToRecord(envHeaderPairs.value) : {},
    };
    await api.saveMcpServer(props.server?.name ?? null, spec);
    message.success("MCP 服务器已保存");
    emit("back");
  } catch (error) {
    message.error(String(error));
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <section class="apple-edit-page mx-auto flex w-full max-w-none flex-col" @keydown.ctrl.enter="save">
    <div class="apple-page-bar apple-page-bar--roomy apple-edit-toolbar apple-edit-toolbar--header">
      <button type="button" class="apple-page-header apple-back-button" aria-label="返回" @click="emit('back')">
        <PhArrowLeft class="h-4 w-4 shrink-0 text-accent" weight="bold" aria-hidden="true" />
        <span class="apple-title">{{ creating ? "新建 MCP 服务器" : "编辑 MCP 服务器" }}</span>
      </button>
    </div>

    <div class="apple-edit-content">
      <div class="apple-group shrink-0 p-0">
        <div class="apple-panel-section">
          <div class="grid gap-4 sm:grid-cols-2">
            <div>
              <div class="field-label mb-1.5">名称</div>
              <n-input v-model:value="name" maxlength="64" placeholder="例如：context7" />
              <p class="muted mt-1.5 text-xs">写入 config.toml 的 [mcp_servers.名称]，仅限字母、数字、下划线和连字符。</p>
            </div>
            <div>
              <div class="field-label mb-1.5">传输类型</div>
              <n-select v-model:value="transport" :options="transportOptions" />
            </div>
          </div>
        </div>

        <div class="apple-panel-section">
          <template v-if="transport === 'stdio'">
            <div>
              <div class="field-label mb-1.5">启动命令</div>
              <n-input v-model:value="command" class="mono" placeholder="例如：npx 或 C:\tools\server.exe" />
            </div>
            <div class="mt-4">
              <div class="field-label mb-1.5">启动参数</div>
              <n-input v-model:value="argsText" type="textarea" :rows="2" class="mono" placeholder="每行一个参数，例如：-y" />
            </div>
          </template>
          <template v-else>
            <div>
              <div class="field-label mb-1.5">服务地址</div>
              <n-input v-model:value="url" class="mono" placeholder="https://mcp.example.com/mcp" />
            </div>
            <div class="mt-4">
              <div class="field-label mb-1.5">Bearer Token 环境变量名（可选）</div>
              <n-input v-model:value="bearer" class="mono" placeholder="例如：TAVILY_API_KEY" />
              <p class="muted mt-1.5 text-xs">Codex 启动时从该环境变量读取令牌放入 Authorization 头；留空则不携带。</p>
            </div>
          </template>
        </div>

        <div class="apple-panel-section">
          <n-collapse>
            <n-collapse-item title="高级选项（环境变量 / 请求头 / 超时）" name="advanced">
              <template v-if="transport === 'stdio'">
                <div class="field-label mb-1.5">环境变量</div>
                <n-dynamic-input v-model:value="envPairs" preset="pair" key-placeholder="变量名" value-placeholder="值" :on-create="() => ({ key: '', value: '' })" />
              </template>
              <template v-else>
                <div class="field-label mb-1.5">HTTP 请求头（固定值）</div>
                <n-dynamic-input v-model:value="headerPairs" preset="pair" key-placeholder="Header 名" value-placeholder="值" :on-create="() => ({ key: '', value: '' })" />
                <div class="field-label mb-1.5 mt-4">HTTP 请求头（值取自环境变量）</div>
                <n-dynamic-input v-model:value="envHeaderPairs" preset="pair" key-placeholder="Header 名" value-placeholder="环境变量名" :on-create="() => ({ key: '', value: '' })" />
              </template>
              <div class="mt-4 grid gap-4 sm:grid-cols-2">
                <div>
                  <div class="field-label mb-1.5">启动超时（秒，可选）</div>
                  <n-input-number v-model:value="startupTimeout" class="w-full" :min="1" clearable placeholder="默认 10" />
                </div>
                <div>
                  <div class="field-label mb-1.5">工具调用超时（秒，可选）</div>
                  <n-input-number v-model:value="toolTimeout" class="w-full" :min="1" clearable placeholder="默认 60" />
                </div>
              </div>
            </n-collapse-item>
          </n-collapse>

          <p v-if="!creating" class="muted mt-3 flex items-start gap-1.5 text-xs">
            <PhInfo class="mt-0.5 h-3.5 w-3.5 shrink-0 text-accent" weight="bold" aria-hidden="true" />
            表单未涵盖的配置项（tools.*、cwd 等）与注释会原样保留；切换传输类型时仅保留当前类型的字段。
          </p>
        </div>
      </div>
    </div>

    <div class="apple-edit-toolbar apple-edit-toolbar--footer">
      <n-button secondary @click="emit('back')">取消</n-button>
      <n-button type="primary" :loading="saving" @click="save">
        <template #icon>
          <PhFloppyDisk class="h-4 w-4" weight="bold" aria-hidden="true" />
        </template>
        保存
      </n-button>
    </div>
  </section>
</template>
