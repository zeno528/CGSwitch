<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { basicSetup } from "codemirror";
import { StreamLanguage, syntaxTree } from "@codemirror/language";
import { toml } from "@codemirror/legacy-modes/mode/toml";
import { json } from "@codemirror/lang-json";
import { forEachDiagnostic, linter, lintGutter, type Diagnostic } from "@codemirror/lint";
import { oneDark } from "@codemirror/theme-one-dark";
import { Codemirror } from "vue-codemirror";
import { api } from "../api";
import type { EditorDiagnosticSummary } from "../types";

type EditorReadyPayload = Parameters<
  NonNullable<InstanceType<typeof Codemirror>["$props"]["onReady"]>
>[0];
type EditorView = EditorReadyPayload["view"];

const props = defineProps<{
  modelValue: string;
  language: "toml" | "json";
  placeholder?: string;
}>();

const emit = defineEmits<{
  "update:modelValue": [value: string];
  diagnostics: [summary: EditorDiagnosticSummary];
}>();

const dark = ref(false);
let observer: MutationObserver | null = null;
let editorView: EditorView | null = null;
const jsonDiagnostics = linter((view) => {
  if (!view.state.doc.toString().trim()) return [];
  const diagnostics: Diagnostic[] = [];
  syntaxTree(view.state).iterate({
    enter(node) {
      if (!node.type.isError) return;
      diagnostics.push({
        from: node.from,
        to: Math.min(view.state.doc.length, Math.max(node.to, node.from + 1)),
        severity: "error",
        source: "JSON",
        message: "JSON 语法错误，请检查此处的逗号、括号或值",
      });
    },
  });
  return diagnostics;
});
const tomlDiagnostics = linter(async (view) => {
  const diagnostics = await api.validateToml(view.state.doc.toString());
  return diagnostics.map(({ from, to, message }) => ({
    from,
    to,
    severity: "error" as const,
    source: "TOML",
    message,
  }));
});

const language = computed(() =>
  props.language === "toml" ? StreamLanguage.define(toml) : json(),
);

const extensions = computed(() => [
  basicSetup,
  language.value,
  props.language === "json" ? jsonDiagnostics : tomlDiagnostics,
  lintGutter(),
  ...(dark.value ? [oneDark] : []),
]);

// 摘要未变则不 emit：光标移动等事务不会触发父组件重渲染
let lastSummary: EditorDiagnosticSummary | null = null;

function reportDiagnostics(view: EditorView) {
  let count = 0;
  let firstLine: number | null = null;
  forEachDiagnostic(view.state, (_diagnostic, from) => {
    count += 1;
    if (firstLine === null) {
      firstLine = view.state.doc.lineAt(from).number;
    }
  });
  if (lastSummary?.count === count && lastSummary.firstLine === firstLine) return;
  lastSummary = { count, firstLine };
  emit("diagnostics", { count, firstLine });
}

function handleReady(payload: { view: EditorView }) {
  editorView = payload.view;
  reportDiagnostics(payload.view);
}

function handleUpdate(update: { view: EditorView }) {
  reportDiagnostics(update.view);
}

function focusFirstDiagnostic() {
  if (!editorView) return;
  let firstFrom: number | null = null;
  let firstTo: number | null = null;
  forEachDiagnostic(editorView.state, (_diagnostic, from, to) => {
    if (firstFrom === null) {
      firstFrom = from;
      firstTo = to;
    }
  });
  if (firstFrom === null || firstTo === null) return;
  editorView.dispatch({
    selection: { anchor: firstFrom, head: firstTo },
    scrollIntoView: true,
  });
  editorView.focus();
}

defineExpose({ focusFirstDiagnostic });

onMounted(() => {
  dark.value = document.documentElement.classList.contains("dark");
  observer = new MutationObserver(() => {
    dark.value = document.documentElement.classList.contains("dark");
  });
  observer.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ["class"],
  });
});

onBeforeUnmount(() => observer?.disconnect());
</script>

<template>
  <div class="apple-editor-shell">
    <Codemirror
      :model-value="modelValue"
      :placeholder="placeholder ?? '在此编辑配置…'"
      :extensions="extensions"
      @update:model-value="emit('update:modelValue', $event)"
      @ready="handleReady"
      @update="handleUpdate"
    />
  </div>
</template>

<style scoped>
:deep(.cm-editor) {
  height: auto;
  background: transparent;
}
:deep(.cm-scroller) {
  /* 编辑器默认按内容自动撑高，无纵向滚动条；横向滚动保留 */
  overflow: auto;
  font-family: Consolas, "Cascadia Mono", monospace;
  font-size: 12px;
  line-height: 1.6;
}
:deep(.cm-lineNumbers) {
  width: 20px;
}
:deep(.cm-lineNumbers .cm-gutterElement) {
  min-width: 20px;
  padding: 0;
}
:deep(.cm-activeLine),
:deep(.cm-activeLineGutter) {
  background-color: var(--selection-bg) !important;
}
</style>
