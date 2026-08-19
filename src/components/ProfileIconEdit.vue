<script setup lang="ts">
import { ref } from "vue";
import { NButton } from "naive-ui";
import { providerIcons } from "../icons";
import { PhArrowLeft, PhFloppyDisk } from "@phosphor-icons/vue";

const props = defineProps<{ icon: string | null; name: string }>();

const emit = defineEmits<{
  back: [];
  save: [icon: string | null];
}>();

const selected = ref<string | null>(props.icon);
</script>

<template>
  <section class="apple-edit-page mx-auto flex w-full max-w-none flex-col">
    <div class="apple-page-bar apple-page-bar--roomy apple-edit-toolbar apple-edit-toolbar--header">
      <button
        type="button"
        class="apple-page-header apple-back-button"
        aria-label="返回"
        @click="emit('back')"
      >
        <PhArrowLeft class="h-4 w-4 shrink-0 text-accent" weight="bold" aria-hidden="true" />
        <span class="apple-title">选择供应商图标</span>
      </button>
    </div>

    <div class="apple-edit-content">
      <div class="apple-group shrink-0 p-[var(--gap-card)]">
      <div class="grid grid-cols-4 gap-1.5 sm:grid-cols-5 md:grid-cols-6 lg:grid-cols-8">
        <button
          v-for="icon in providerIcons"
          :key="icon.id"
          type="button"
          class="flex flex-col items-center gap-1 rounded-lg px-1.5 py-2 transition-colors"
          :class="selected === icon.id ? 'shadow-[0_0_0_1px_var(--accent)] bg-[var(--selection-bg)]' : 'shadow-[0_0_0_1px_var(--panel-ring)] hover:bg-black/3 dark:hover:bg-white/4'"
          :aria-pressed="selected === icon.id"
          @click="selected = icon.id"
        >
          <span class="grid h-7 w-7 place-items-center rounded-lg bg-[#f0f0f3]" aria-hidden="true">
            <img :src="icon.url" :alt="icon.label" class="h-4 w-4" />
          </span>
          <span class="w-full truncate text-center text-xs">{{ icon.label }}</span>
        </button>
      </div>

      <button
        type="button"
        class="mt-3 w-full rounded-lg border border-dashed px-2 py-2.5 text-xs transition-colors"
        :class="selected === null ? 'border-accent font-medium text-accent' : 'muted border-[var(--panel-border)] hover:bg-black/3 dark:hover:bg-white/4'"
        :aria-pressed="selected === null"
        @click="selected = null"
      >
        不使用图标（显示名称首字）
      </button>
      </div>
    </div>

    <div class="apple-edit-toolbar apple-edit-toolbar--footer">
      <n-button secondary @click="emit('back')">取消</n-button>
      <n-button type="primary" @click="emit('save', selected)">
        <template #icon>
          <PhFloppyDisk class="h-4 w-4" weight="bold" aria-hidden="true" />
        </template>
        保存
      </n-button>
    </div>
  </section>
</template>
