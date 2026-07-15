<script setup lang="ts">
import AppShell from "../components/AppShell.vue";
import { getEngineBridge } from "../bridges/engine";

getEngineBridge().run("render-shell");
</script>

<template>
  <AppShell />
</template>
