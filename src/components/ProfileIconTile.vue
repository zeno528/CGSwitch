<script setup lang="ts">
import { computed } from "vue";
import { providerIconUrl } from "../icons";

const props = defineProps<{
  name: string;
  icon: string | null;
  size?: "sm" | "lg";
}>();

const iconUrl = computed(() => providerIconUrl(props.icon));
const sizeClass = computed(() => {
  const map = {
    sm: { tile: "h-10 w-10 rounded-[12px]", img: "h-6 w-6", text: "text-sm" },
    lg: { tile: "h-[76px] w-[76px] rounded-[22px]", img: "h-10 w-10", text: "text-xl" },
  };
  return map[props.size ?? "sm"];
});
</script>

<template>
  <span class="grid shrink-0 place-items-center bg-[#f0f0f3]" :class="sizeClass.tile" aria-hidden="true">
    <img v-if="iconUrl" :src="iconUrl" alt="" :class="sizeClass.img" />
    <span v-else class="font-bold text-[#007aff]" :class="sizeClass.text">{{ name.charAt(0) }}</span>
  </span>
</template>
