<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { basicSetup } from "codemirror";
import { StreamLanguage } from "@codemirror/language";
import { toml } from "@codemirror/legacy-modes/mode/toml";
import { json } from "@codemirror/lang-json";
import { oneDark } from "@codemirror/theme-one-dark";
import { Codemirror } from "vue-codemirror";

const props = defineProps<{
  modelValue: string;
  language: "toml" | "json";
  placeholder?: string;
}>();

const emit = defineEmits<{ "update:modelValue": [value: string] }>();

const dark = ref(false);
let observer: MutationObserver | null = null;

const language = computed(() =>
  props.language === "toml" ? StreamLanguage.define(toml) : json(),
);

const extensions = computed(() => [
  basicSetup,
  language.value,
  ...(dark.value ? [oneDark] : []),
]);

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
  <div class="overflow-hidden rounded-xl border border-[var(--panel-border)] bg-black/4 dark:bg-white/6">
    <Codemirror
      :model-value="modelValue"
      :placeholder="placeholder ?? '在此编辑配置…'"
      :extensions="extensions"
      @update:model-value="emit('update:modelValue', $event)"
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
</style>
