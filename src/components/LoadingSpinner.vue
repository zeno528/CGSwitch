<script setup lang="ts">
withDefaults(
  defineProps<{ size?: "sm" | "md" }>(),
  { size: "sm" },
);
</script>

<template>
  <svg
    class="animate-spin"
    :class="size === 'md' ? 'h-4 w-4' : 'h-3 w-3'"
    viewBox="0 0 24 24"
    fill="none"
    aria-hidden="true"
  >
    <circle cx="12" cy="12" r="9" stroke="currentColor" stroke-opacity="0.25" stroke-width="2.5" />
    <path d="M21 12a9 9 0 0 0-9-9" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" />
  </svg>
</template>
