<script setup lang="ts">
import { ref } from "vue";
import { NButton } from "naive-ui";
import { providerIcons } from "../icons";

const props = defineProps<{ icon: string | null; name: string }>();

const emit = defineEmits<{
  back: [];
  save: [icon: string | null];
}>();

const selected = ref<string | null>(props.icon);
</script>

<template>
  <section class="mx-auto flex h-[calc(100vh-2.75rem)] w-full max-w-none flex-col">
    <div class="-mx-8 flex items-center bg-[var(--app-bg)] px-8 py-2">
      <button
        type="button"
        class="apple-page-header apple-back-button"
        aria-label="返回"
        @click="emit('back')"
      >
        <svg class="h-4 w-4 shrink-0 text-[#007aff]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
          <path d="M15 5.5 8.5 12l6.5 6.5" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
        <span class="apple-title">选择供应商图标</span>
      </button>
    </div>

    <div class="-mx-8 flex min-h-0 flex-1 flex-col overflow-auto pl-[var(--gap-main)] pr-[calc(var(--gap-main)-var(--scrollbar-size))] pb-4 [scrollbar-gutter:stable]">
      <div class="apple-group mt-[var(--gap-page)] shrink-0 p-[var(--gap-card)]">
      <div class="grid grid-cols-4 gap-1.5 sm:grid-cols-5 md:grid-cols-6 lg:grid-cols-8">
        <button
          v-for="icon in providerIcons"
          :key="icon.id"
          type="button"
          class="flex flex-col items-center gap-1 rounded-lg border px-1.5 py-2 transition-colors"
          :class="selected === icon.id ? 'border-[#007aff] bg-[var(--selection-bg)]' : 'border-[var(--panel-border)] hover:bg-black/3 dark:hover:bg-white/4'"
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
        :class="selected === null ? 'border-[#007aff] font-medium text-[#007aff]' : 'muted border-[var(--panel-border)] hover:bg-black/3 dark:hover:bg-white/4'"
        :aria-pressed="selected === null"
        @click="selected = null"
      >
        不使用图标（显示名称首字）
      </button>
      </div>
    </div>

    <div class="-mx-8 -mb-7 flex items-center justify-end gap-2 bg-[var(--app-bg)] pl-8 pr-[42px] pt-2 pb-4">
      <n-button secondary @click="emit('back')">取消</n-button>
      <n-button type="primary" @click="emit('save', selected)">
        <template #icon>
          <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z" />
            <path d="M17 21v-8H7v8" />
            <path d="M7 3v5h8" />
          </svg>
        </template>
        保存
      </n-button>
    </div>
  </section>
</template>
