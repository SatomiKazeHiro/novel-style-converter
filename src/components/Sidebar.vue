<template>
  <aside class="sidebar" :class="{ collapsed }">
    <nav class="nav">
      <router-link
        v-for="item in topItems"
        :key="item.to"
        :to="item.to"
        class="nav-item"
        :class="{ active: isActive(item.to) }"
        :title="collapsed ? item.label : ''"
      >
        <component :is="item.icon" :size="16" :stroke-width="1.5" />
        <span class="label">{{ item.label }}</span>
      </router-link>
    </nav>

    <div class="handle">
      <button
        class="collapse-btn"
        :title="collapsed ? '展开侧栏' : '折叠侧栏'"
        :aria-label="collapsed ? '展开侧栏' : '折叠侧栏'"
        @click="toggle"
      >
        <component :is="collapsed ? IconChevronsRight : IconChevronsLeft" :size="14" :stroke-width="2" />
      </button>
    </div>

    <nav class="nav nav-bottom">
      <button
        class="nav-item theme-btn"
        :title="theme.theme === 'light' ? '切换到暗色' : '切换到亮色'"
        @click="theme.toggle()"
      >
        <component :is="theme.theme === 'light' ? IconSun : IconMoon" :size="16" :stroke-width="1.5" />
        <span class="label">{{ theme.theme === 'light' ? '暗色' : '亮色' }}</span>
      </button>
    </nav>
  </aside>
</template>

<script setup lang="ts">
import { markRaw, ref, watch, type Component } from 'vue';
import { useRoute } from 'vue-router';
import IconUpload from '~icons/lucide/upload';
import IconDatabase from '~icons/lucide/database';
import IconRepeat from '~icons/lucide/repeat';
import IconFileText from '~icons/lucide/file-text';
import IconBox from '~icons/lucide/box';
import IconActivity from '~icons/lucide/activity';
import IconNetwork from '~icons/lucide/network';
import IconSun from '~icons/lucide/sun';
import IconMoon from '~icons/lucide/moon';
import IconChevronsLeft from '~icons/lucide/chevrons-left';
import IconChevronsRight from '~icons/lucide/chevrons-right';
import { useThemeStore } from '../stores/theme';

interface Item { to: string; label: string; icon: Component }

const topItems: Item[] = [
  { to: '/overview', label: '总览', icon: markRaw(IconNetwork) },
  { to: '/uploads', label: '上传原文', icon: markRaw(IconUpload) },
  { to: '/data-assets', label: '数据资产', icon: markRaw(IconDatabase) },
  { to: '/transformations', label: '转换工程', icon: markRaw(IconRepeat) },
  { to: '/prompts', label: '提示词', icon: markRaw(IconFileText) },
  { to: '/models', label: '模型', icon: markRaw(IconBox) },
  { to: '/ai-calls', label: 'AI 调用', icon: markRaw(IconActivity) },
];

const route = useRoute();
const theme = useThemeStore();

/// 折叠状态持久化 —— localStorage key 加版本号,改语义时可平滑迁移。
/// 默认展开(false),符合用户进入 app 第一眼看到完整菜单的预期。
const STORAGE_KEY = 'sidebar_collapsed_v1';
function readCollapsed(): boolean {
  try { return localStorage.getItem(STORAGE_KEY) === '1'; }
  catch { return false; }
}
const collapsed = ref(readCollapsed());
watch(collapsed, (v) => {
  try { localStorage.setItem(STORAGE_KEY, v ? '1' : '0'); }
  catch { /* localStorage 不可用(隐私模式等)不致命 —— 下次启动按默认展开 */ }
});
function toggle() { collapsed.value = !collapsed.value; }

function isActive(to: string): boolean {
  if (route.path === to) return true;
  if (to === '/uploads') return route.path.startsWith('/library/upload/');
  if (to === '/data-assets') return route.path.startsWith('/library/data/');
  if (to === '/transformations') return route.path.startsWith('/library/transformation');
  if (to === '/overview') return route.path === '/overview';
  return false;
}
</script>

<style scoped>
/* 单一尺寸贯穿展开/折叠 —— 折叠只移动 sidebar 宽度 + 隐藏 label,
   nav-item / icon 几何尺寸永远不变;图标靠左填充整个 nav-item 内框,
   折叠态下 sidebar=64 → 内框=16 → 图标(16)自然落在几何中心,无瞬移。 */
.sidebar {
  width: 220px;
  flex-shrink: 0;
  background: var(--bg-sidebar);
  display: flex;
  flex-direction: column;
  padding: 16px 0;
  box-sizing: border-box;
  height: 100vh;
  position: relative;
  transition: width 0.18s ease;
}
.sidebar.collapsed {
  width: 64px;
}
.sidebar::after {
  content: '';
  position: absolute;
  right: 0;
  top: 0;
  bottom: 0;
  width: 1px;
  background: var(--border-rouge);
}

.handle {
  position: absolute;
  right: -14px;
  top: 50%;
  transform: translateY(-50%);
  z-index: 1;
}

.collapse-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: 1px solid var(--border-soft);
  background: var(--bg-card);
  color: var(--text-secondary);
  border-radius: 50%;
  cursor: pointer;
  font-family: inherit;
  padding: 0;
  transition: color 0.12s, border-color 0.12s, background 0.12s;
}
.collapse-btn:hover {
  color: var(--color-cinnabar);
  border-color: var(--color-cinnabar);
  background: var(--color-cinnabar-light);
}

.nav {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 0 12px;
}
.nav-bottom {
  margin-top: auto;
  padding-top: 12px;
  border-top: 1px solid var(--border-soft);
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  border-radius: var(--radius-pin);
  color: var(--text-secondary);
  text-decoration: none;
  background: transparent;
  border: none;
  cursor: pointer;
  font-size: 14px;
  font-family: inherit;
  width: 100%;
  box-sizing: border-box;
  text-align: left;
  overflow: hidden;
  transition: background 0.12s, color 0.12s;
}
.nav-item svg {
  flex-shrink: 0;
}
.nav-item:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}
.nav-item.active {
  background: var(--color-cinnabar-light);
  color: var(--color-cinnabar-deep);
  font-weight: var(--font-weight-medium);
  position: relative;
}
.nav-item.active::before {
  content: '';
  position: absolute;
  left: 0;
  top: 6px;
  bottom: 6px;
  width: 2px;
  background: var(--color-cinnabar);
}

.theme-btn { font-size: 13px; color: var(--text-muted); }

.label {
  white-space: nowrap;
  overflow: hidden;
  opacity: 1;
  width: auto;
  transition: opacity 0.12s ease, width 0.18s ease;
}
.sidebar.collapsed .label {
  opacity: 0;
  width: 0;
  pointer-events: none;
}

</style>
