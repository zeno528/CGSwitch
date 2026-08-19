<script setup lang="ts">
import { NSwitch } from "naive-ui";

defineOptions({ inheritAttrs: false });
</script>

<template>
  <n-switch v-bind="$attrs" :rubber-band="true" />
</template>
