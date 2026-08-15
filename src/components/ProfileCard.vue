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
    <div class="flex shrink-0 items-center gap-2" @dblclick.stop>
      <n-button type="primary" size="small" :disabled="busy || active" @click="emit('apply')">应用</n-button>
      <n-button size="small" :disabled="busy" @click="emit('rename')">重命名</n-button>
      <n-button size="small" quaternary type="error" :disabled="busy" @click="emit('remove')">删除</n-button>
    </div>
  </article>
</template>
