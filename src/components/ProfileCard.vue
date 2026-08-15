<script setup lang="ts">
import { computed } from "vue";
import { NButton, NTag } from "naive-ui";
import { providerIconUrl } from "../icons";
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

const iconUrl = computed(() => providerIconUrl(props.profile.icon));
</script>

<template>
  <article class="flex flex-col gap-4 px-5 py-4 transition-colors sm:flex-row sm:items-center sm:justify-between" :class="active ? 'bg-[var(--selection-bg)]' : 'hover:bg-black/3 dark:hover:bg-white/4'">
    <div class="flex min-w-0 flex-1 items-center gap-3">
      <img v-if="iconUrl" :src="iconUrl" :alt="profile.name" class="h-10 w-10 shrink-0 rounded-[10px]" />
      <span v-else class="grid h-10 w-10 shrink-0 place-items-center rounded-[10px] bg-[#007aff]/10 text-sm font-bold text-[#007aff]" aria-hidden="true">{{ profile.name.charAt(0) }}</span>
      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-2">
          <h3 class="truncate font-semibold">{{ profile.name }}</h3>
          <n-tag v-if="active" type="success" size="small">当前生效</n-tag>
        </div>
        <div class="muted mt-1 flex flex-wrap gap-x-3 gap-y-1 text-xs">
          <span class="mono">{{ profile.model ?? "未设置" }}</span>
          <span>{{ profile.provider ?? "官方" }}</span>
          <span>推理：{{ profile.reasoning_effort ?? "默认" }}</span>
        </div>
      </div>
    </div>
    <div class="flex shrink-0 items-center gap-2">
      <n-button type="primary" size="small" :disabled="busy || active" @click="emit('apply')">应用</n-button>
      <n-button size="small" :disabled="busy" @click="emit('rename')">重命名</n-button>
      <n-button size="small" :disabled="busy" @click="emit('edit')">编辑</n-button>
      <n-button size="small" quaternary type="error" :disabled="busy" @click="emit('remove')">删除</n-button>
    </div>
  </article>
</template>
