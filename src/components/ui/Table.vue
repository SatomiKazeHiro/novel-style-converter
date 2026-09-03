<template>
  <div class="wrap">
    <table class="table">
      <thead>
        <tr>
          <th
            v-for="col in columns"
            :key="col.key"
            :style="{ width: typeof col.width === 'number' ? `${col.width}px` : col.width ?? 'auto' }"
          >
            {{ col.title }}
          </th>
        </tr>
      </thead>
      <tbody>
        <tr v-if="data.length === 0">
          <td :colspan="columns.length" class="empty">{{ emptyText }}</td>
        </tr>
        <tr v-for="(row, i) in data" :key="rowKey(row, i)">
          <td
            v-for="col in columns"
            :key="col.key"
            :class="{ actions: col.type === 'actions' }"
          >
            <slot :name="`cell-${col.key}`" :row="row">
              {{ row[col.key] }}
            </slot>
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<script setup lang="ts">
withDefaults(
  defineProps<{
    columns: { key: string; title: string; width?: string | number; type?: 'text' | 'actions' }[];
    data: any[];
    emptyText?: string;
    rowKey?: (row: any, idx: number) => string | number;
  }>(),
  {
    emptyText: '暂无数据',
    rowKey: (_row: any, idx: number) => idx,
  },
);
</script>

<style scoped>
/*
  ruled-manuscript table: 白卡浮在米纸, 行间用浅 hairline,
  th 用衬线 small caps 风格, hover 红线突出
*/
.wrap {
  width: 100%;
  background: var(--color-sheet);
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-card);
  overflow: hidden;
  box-shadow: var(--shadow);
}
.table {
  width: 100%;
  border-collapse: collapse;
  font-size: 13.5px;
}
.table th,
.table td {
  padding: 12px 16px;
  border-bottom: 1px solid var(--border-soft);
  text-align: left;
}
.table th {
  background: transparent;
  font-family: var(--font-serif);
  font-weight: var(--font-weight-regular);
  font-size: 12px;
  color: var(--text-muted);
  letter-spacing: 0.04em;
  text-transform: uppercase;
}
.table td {
  color: var(--text-primary);
}
/* 操作列(`type: 'actions'`)里多个 button 紧贴 —— 在相邻 button 之间加 margin。
   故意不用 flex:flex 容器的 align-items 会让按钮垂直居中,同行若其它 cell 很高
   (例如文件名换行 3 行)就会在按钮下方留一大块空白;保持 table-cell 布局让 cell
   高度只受 padding 影响。 */
.table td.actions :slotted(button) + :slotted(button) {
  margin-left: 6px;
}
.table tbody tr:last-child td {
  border-bottom: none;
}
.table tbody tr:hover td {
  background: var(--color-paper);
}
.empty {
  text-align: center;
  color: var(--text-muted);
  padding: 48px 0;
  font-family: var(--font-serif);
  font-style: italic;
}
</style>
