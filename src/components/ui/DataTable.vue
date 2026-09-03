<template>
  <div
    ref="wrapEl"
    class="dt-wrap"
    :class="{ 'is-end': isAtEnd, 'has-max': maxHeight != null }"
    :style="maxHeight != null ? { maxHeight } : undefined"
  >
    <table class="dt">
      <thead>
        <tr v-for="hg in table.getHeaderGroups()" :key="hg.id">
          <th
            v-for="header in hg.headers"
            :key="header.id"
            :class="thClass(header)"
            :style="thStyle(header)"
            @click="onThClick(header.column)"
          >
            <slot
              v-if="!header.isPlaceholder"
              :name="`header-${header.column.id}`"
              :header="header"
              :column="header.column"
            >
              <FlexRender
                :render="header.column.columnDef.header"
                :props="header.getContext()"
              />
            </slot>
            <span v-if="isSortable(header.column)" class="sort-mark" aria-hidden="true">
              <svg width="9" height="9" viewBox="0 0 9 9">
                <path d="M2 4 L4.5 1.5 L7 4" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" />
                <path d="M2 6 L4.5 8.5 L7 6" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" />
              </svg>
            </span>
          </th>
        </tr>
      </thead>
      <tbody>
        <tr v-if="table.getRowModel().rows.length === 0">
          <td :colspan="table.getAllColumns().length" class="empty">{{ emptyText }}</td>
        </tr>
        <tr v-for="row in table.getRowModel().rows" :key="rowKey(row.original)">
          <td
            v-for="cell in row.getAllCells()"
            :key="cell.id"
            :class="tdClass(cell)"
            :style="tdStyle(cell)"
          >
            <slot :name="`cell-${cell.column.id}`" :row="row.original">
              <FlexRender
                v-if="cell.column.columnDef.cell"
                :render="cell.column.columnDef.cell"
                :props="cell.getContext()"
              />
              <template v-else>{{ cell.getValue() }}</template>
            </slot>
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import {
  FlexRender,
  createCoreRowModel,
  createSortedRowModel,
  rowSortingFeature,
  sortFn_alphanumeric,
  sortFn_basic,
  sortFn_datetime,
  sortFn_text,
  tableFeatures,
  useTable,
} from '@tanstack/vue-table';

const props = withDefaults(
  defineProps<{
    columns: any[];
    data: any[];
    emptyText?: string;
    rowKey: (row: any) => string | number;
    widths?: Record<string, number>;
    numericColumns?: string[];
    truncateColumns?: string[];
    frozenColumn?: string | null;
    /// wrap 元素最大高度(例如 "400px")。超出会出现垂直滚动条,
    /// 不传则让表格自然撑开。
    maxHeight?: string;
  }>(),
  {
    emptyText: '暂无数据',
    widths: () => ({}),
    numericColumns: () => [],
    truncateColumns: () => [],
    frozenColumn: null,
  },
);

const numericAlign = computed(() => new Set(props.numericColumns));

const features = tableFeatures({
  rowSortingFeature,
  coreRowModel: createCoreRowModel(),
  sortedRowModel: createSortedRowModel(),
  // Register the common auto-picked sortFns so TanStack does not warn when a column
  // resolves its default sort by value type (string → alphanumeric/text, date → datetime).
  sortFns: {
    alphanumeric: sortFn_alphanumeric,
    basic: sortFn_basic,
    datetime: sortFn_datetime,
    text: sortFn_text,
  },
});

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const table: any = useTable({
  features,
  data: computed(() => props.data),
  columns: computed(() => props.columns),
});

const wrapEl = ref<HTMLElement | null>(null);
const isAtEnd = ref(false);
let resizeObserver: ResizeObserver | null = null;

function checkScrollEnd() {
  const el = wrapEl.value;
  if (!el) return;
  // 浮点容差:scrollLeft + clientWidth 接近 scrollWidth 即可视为到底
  isAtEnd.value = el.scrollLeft + el.clientWidth >= el.scrollWidth - 1;
}

onMounted(() => {
  checkScrollEnd();
  wrapEl.value?.addEventListener('scroll', checkScrollEnd, { passive: true });
  // data / columns 变化会让表格宽度变,需要重新判断"是否已到末尾"
  resizeObserver = new ResizeObserver(checkScrollEnd);
  if (wrapEl.value) resizeObserver.observe(wrapEl.value);
});
onUnmounted(() => {
  wrapEl.value?.removeEventListener('scroll', checkScrollEnd);
  resizeObserver?.disconnect();
  resizeObserver = null;
});

function widthOf(columnId: string): number | null {
  return props.widths[columnId] ?? null;
}

function thStyle(header: any) {
  const w = widthOf(header.column.id);
  return { width: w ? `${w}px` : 'auto' };
}
function tdStyle(cell: any) {
  const w = widthOf(cell.column.id);
  return { width: w ? `${w}px` : 'auto' };
}

function isSortable(column: any): boolean {
  return !!column?.getCanSort?.();
}

function sortDir(column: any): false | 'asc' | 'desc' {
  return column?.getIsSorted?.() ?? false;
}

function thClass(header: any) {
  const col = header.column;
  const dir = sortDir(col);
  return {
    sortable: isSortable(col),
    'sort-asc': dir === 'asc',
    'sort-desc': dir === 'desc',
    'col-numeric': numericAlign.value.has(col.id),
    'col-actions': col.id === 'actions',
    frozen: props.frozenColumn != null && col.id === props.frozenColumn,
  };
}

function tdClass(cell: any) {
  const col = cell.column;
  return {
    actions: col.id === 'actions',
    numeric: numericAlign.value.has(col.id),
    truncate: props.truncateColumns.includes(col.id),
    frozen: props.frozenColumn != null && col.id === props.frozenColumn,
  };
}

function onThClick(column: any) {
  if (!isSortable(column)) return;
  column.toggleSorting();
}
</script>

<style scoped>
.dt-wrap {
  width: 100%;
  background: var(--color-sheet);
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-card);
  /* 不设 overflow: hidden 下,table 总宽 > 容器时启动横向滚动条,
     高度仍被 border-radius 裁出圆角。点击 tooltip 被 body 里的 host
     接住，不受表格滑动器影响。 */
  overflow-x: auto;
  /* maxHeight 不传时自然撑开;传了就允许垂直滚动,跟 x 一起走 auto */
  overflow-y: auto;
  box-shadow: var(--shadow);
}
.dt {
  width: 100%;
  border-collapse: collapse;
  table-layout: fixed;
  font-size: 13.5px;
}
.dt th,
.dt td {
  padding: 10px 12px;
  border-bottom: 1px solid var(--border-soft);
  text-align: left;
  vertical-align: middle;
  /* table-layout: fixed 下 width 默认是 content-box,会让 12px 左右 padding 把 cell
     撑大 24px(列宽 ≠ 设定值)。改成 border-box 后,widths 真正就是 cell 总宽,
     pick 这种窄列(checkbox + 大 padding)也不会被撑宽。 */
  box-sizing: border-box;
}
.has-max .dt th {
  position: sticky;
  top: 0;
  background: var(--color-sheet);
  z-index: 1;
}
.dt th {
  background: transparent;
  font-family: var(--font-serif);
  font-weight: var(--font-weight-regular);
  font-size: 12px;
  color: var(--text-muted);
  letter-spacing: 0.04em;
  text-transform: uppercase;
  user-select: none;
  white-space: nowrap;
}
.dt th.sortable {
  cursor: pointer;
}
.dt th.sortable:hover {
  color: var(--text-secondary);
}
.dt th.col-numeric {
  text-align: right;
}
.dt th .sort-mark {
  display: inline-block;
  margin-left: 6px;
  vertical-align: -1px;
  color: var(--text-muted);
  opacity: 0.45;
}
.dt th.sort-asc .sort-mark,
.dt th.sort-desc .sort-mark {
  opacity: 1;
  color: var(--accent);
}
.dt th.sort-asc .sort-mark svg path:first-child,
.dt th.sort-desc .sort-mark svg path:last-child {
  stroke: currentColor;
  stroke-width: 1.8;
}
.dt th.col-actions {
  text-align: right;
}
.dt td.numeric {
  text-align: right;
  font-variant-numeric: tabular-nums;
}
.dt td.actions {
  text-align: right;
  white-space: nowrap;
}
.dt td.truncate {
  max-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.dt tbody tr:hover td {
  background: var(--color-paper);
}
.dt th.frozen,
.dt td.frozen {
  position: sticky;
  right: 0;
  background: var(--color-sheet);
  z-index: 2;
}
/* 滚到右端时,frozen 列后面没有内容可"浮",阴影就不该出现 */
.dt-wrap.is-end .frozen::before {
  display: none;
}
/* box-shadow 在 td/th + border-collapse 下经常被 cell 边界裁掉,
   用伪元素在 cell 左侧画一道渐变阴影更靠谱 */
.dt th.frozen::before,
.dt td.frozen::before {
  content: '';
  position: absolute;
  top: 0;
  bottom: 0;
  left: -6px;
  width: 6px;
  background: linear-gradient(to left, rgba(26, 23, 20, 0.14), transparent);
  pointer-events: none;
}
.dt th.frozen {
  z-index: 3;
}
.dt tbody tr:hover td.frozen {
  background: var(--color-paper);
}
.dt tbody tr:last-child td {
  border-bottom: none;
}
.empty {
  text-align: center;
  color: var(--text-muted);
  padding: 48px 0;
  font-family: var(--font-serif);
  font-style: italic;
}
</style>
