<template>
  <div class="layout">
    <Sidebar />
    <main class="app">
      <router-view />
    </main>
    <TooltipHost />
    <ConfirmHost />
  </div>
</template>

<script setup lang="ts">
import Sidebar from './components/Sidebar.vue';
import TooltipHost from './components/ui/TooltipHost.vue';
import ConfirmHost from './components/ui/ConfirmHost.vue';
</script>

<style scoped>
.layout {
  display: flex;
  height: 100vh;
}
.app {
  flex: 1;
  min-width: 0;
  padding: 24px;
  box-sizing: border-box;
  overflow: auto;
  background: var(--bg-app);
  color: var(--text-primary);
}
</style>