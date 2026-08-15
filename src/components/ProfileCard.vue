<script setup lang="ts">
import { NButton, NTag } from "naive-ui";
import ProfileIconTile from "./ProfileIconTile.vue";
import type { ProfileSummary } from "../types";

const props = defineProps<{
  profile: ProfileSummary;
  active: boolean;
  busy: boolean;
}>();

const emit = defineEmits<{
  apply: [];
  rename: [];
  remove: [];
  edit: [];
}>();

</script>

<template>
  <article class="flex flex-col gap-4 px-5 py-4 transition-colors sm:flex-row sm:items-center sm:justify-between" :class="active ? 'bg-[linear-gradient(90deg,var(--selection-bg),transparent_65%)]' : 'hover:bg-black/3 dark:hover:bg-white/4'" title="双击编辑" @dblclick="emit('edit')">
    <div class="flex min-w-0 flex-1 items-center gap-3">
      <ProfileIconTile :name="profile.name" :icon="profile.icon" />
      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-2">
          <h3 class="cursor-pointer truncate font-semibold transition-colors hover:text-[#007aff]" title="点击重命名" @click="emit('rename')">{{ profile.name }}</h3>
          <n-tag v-if="active" type="success" size="small">当前生效</n-tag>
        </div>
        <div class="muted mt-1 flex flex-wrap gap-1.5 text-xs">
          <span class="rounded-full border border-current/15 bg-black/4 px-2.5 py-0.5 dark:bg-white/8">{{ profile.model ?? "未设置" }}</span>
          <span class="rounded-full border border-current/15 bg-black/4 px-2.5 py-0.5 dark:bg-white/8">{{ profile.provider ?? "官方" }}</span>
          <span class="rounded-full border border-current/15 bg-black/4 px-2.5 py-0.5 dark:bg-white/8">{{ profile.reasoning_effort ?? "默认" }}</span>
        </div>
      </div>
    </div>
    <div class="flex shrink-0 items-center gap-2" @dblclick.stop>
      <n-button type="primary" size="small" :disabled="busy || active" @click="emit('apply')">应用</n-button>
      <button
        type="button"
        class="grid h-8 w-8 place-items-center rounded-lg text-[#ff3b30]/60 transition-colors hover:bg-[#ff3b30]/10 hover:text-[#ff3b30] disabled:pointer-events-none disabled:opacity-40"
        :disabled="busy || active"
        title="删除"
        aria-label="删除"
        @click="emit('remove')"
      >
        <svg class="h-[18px] w-[18px]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M9.5 4.5a2.5 2.5 0 0 1 5 0" />
          <path d="M5 6.5h14" />
          <path d="M6 6.5V18a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V6.5" />
          <path d="M9.5 10v4.5" />
          <path d="M14.5 10v4.5" />
        </svg>
      </button>
    </div>
  </article>
</template>
