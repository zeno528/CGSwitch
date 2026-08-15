<script setup lang="ts" generic="T extends string">
defineProps<{
  options: { value: T; label: string }[];
  modelValue: T;
}>();

const emit = defineEmits<{
  "update:modelValue": [value: T];
}>();
</script>

<template>
  <div class="apple-group flex p-1">
    <button
      v-for="option in options"
      :key="option.value"
      type="button"
      class="h-9 flex-1 rounded-xl px-3 text-sm transition-colors"
      :class="modelValue === option.value ? 'bg-[var(--selection-bg)] font-semibold text-[#007aff]' : 'font-medium hover:bg-black/5 dark:hover:bg-white/8'"
      :aria-pressed="modelValue === option.value"
      @click="emit('update:modelValue', option.value)"
    >
      {{ option.label }}
    </button>
  </div>
</template>
