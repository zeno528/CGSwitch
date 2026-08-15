<script setup lang="ts">
import { ref, watch } from "vue";
import { NButton } from "naive-ui";
import { providerIcons } from "../icons";
import type { ProfileSummary } from "../types";

const props = defineProps<{ profile: ProfileSummary }>();

const emit = defineEmits<{
  back: [];
  save: [icon: string | null];
}>();

const selected = ref<string | null>(props.profile.icon);

watch(
  () => props.profile.id,
  () => {
    selected.value = props.profile.icon;
  },
);
</script>

<template>
  <section class="mx-auto w-full max-w-none">
    <div class="flex items-center gap-2">
      <n-button quaternary circle size="small" aria-label="返回" @click="emit('back')">
        <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
          <path d="M15 5.5 8.5 12l6.5 6.5" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </n-button>
      <h1 class="apple-title">编辑档案图标</h1>
    </div>
    <p class="muted mt-2 text-sm">为「{{ profile.name }}」选择一个供应商图标，显示在配置列表中。</p>

    <div class="apple-group mt-7 p-5">
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

    <div class="mt-5 flex justify-end gap-2">
      <n-button @click="emit('back')">取消</n-button>
      <n-button type="primary" @click="emit('save', selected)">保存</n-button>
    </div>
  </section>
</template>
