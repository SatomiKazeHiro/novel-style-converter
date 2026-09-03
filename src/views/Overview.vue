<template>
  <section class="overview">
    <PageHeader
      title="总览"
      subtitle="上传原文 · 数据资产 · 转换工程 · 工作流 一张图，数据 5 秒自动刷新"
    >
    </PageHeader>

    <div v-if="graph" class="stats-row">
      <div class="stat-card">
        <div class="num">{{ graph.stats.upload_count }}</div>
        <div class="lbl">上传原文</div>
      </div>
      <div class="stat-card">
        <div class="num">{{ graph.stats.data_asset_count }}</div>
        <div class="lbl">数据资产</div>
      </div>
      <div class="stat-card">
        <div class="num">{{ graph.stats.transformation_novel_count }}</div>
        <div class="lbl">转换工程</div>
      </div>
      <div class="stat-card run">
        <div class="num">{{ graph.stats.running_batch_count }}</div>
        <div class="lbl">工作中 batch</div>
      </div>
      <div class="stat-card fail">
        <div class="num">{{ graph.stats.failed_recent_count }}</div>
        <div class="lbl">24h 失败 batch</div>
      </div>
    </div>

    <div v-if="error" class="error">加载失败:{{ error }}</div>

    <VueFlow
      v-else
      v-model:nodes="flowNodes"
      v-model:edges="flowEdges"
      :node-types="nodeTypes"
      :nodes-draggable="false"
      :nodes-connectable="false"
      :elements-selectable="true"
      :zoom-on-double-click="false"
      :default-viewport="{ x: 0, y: 0, zoom: 0.8 }"
      :min-zoom="0.1"
      :max-zoom="4"
      :translate-on-scroll="false"
      :pan-on-scroll="true"
      :pan-on-drag="true"
      fit-view-on-init
      class="cy-container"
      @node-click="onNodeClick"
      @move="onMove"
    >
      <Background :gap="20" :size="1" />
      <Controls />
      <MiniMap pannable zoomable />
    </VueFlow>

    <div v-if="graph && graph.total_nodes_raw >= 500" class="hint">
      节点较多 ({{ graph.total_nodes_raw }}),可拖拽 / 滚轮缩放 / 空白处拖动平移。
    </div>
    <div v-if="!graph && !error" class="placeholder">加载中…</div>
  </section>
</template>

<script setup lang="ts">
import { onMounted, ref, markRaw, nextTick } from "vue";
import { useRouter } from "vue-router";
import { useIntervalFn } from "@vueuse/core";
import { VueFlow, useVueFlow, type Node, type Edge } from "@vue-flow/core";
import { Background } from "@vue-flow/background";
import { Controls } from "@vue-flow/controls";
import { MiniMap } from "@vue-flow/minimap";
import dagre from "dagre";

import "@vue-flow/core/dist/style.css";
import "@vue-flow/controls/dist/style.css";
import "@vue-flow/minimap/dist/style.css";

import PageHeader from "../components/ui/PageHeader.vue";
import UploadNode from "../components/overview/UploadNode.vue";
import SourceDaNode from "../components/overview/SourceDaNode.vue";
import PromotedDaNode from "../components/overview/PromotedDaNode.vue";
import TnNode from "../components/overview/TnNode.vue";
import BatchNode from "../components/overview/BatchNode.vue";
import { getOverviewGraph } from "../ipc/commands";
import type { OverviewGraph, OverviewNode, OverviewEdge as ApiEdge } from "../ipc/types";

const nodeTypes: Record<string, any> = markRaw({
  upload: UploadNode,
  source_data_asset: SourceDaNode,
  promoted_data_asset: PromotedDaNode,
  transformation_novel: TnNode,
  batch: BatchNode,
});

const graph = ref<OverviewGraph | null>(null);
const error = ref("");
const loading = ref(false);
const flowNodes = ref<Node[]>([]);
const flowEdges = ref<Edge[]>([]);
const router = useRouter();
const { setViewport } = useVueFlow();
let savedViewport: { x: number; y: number; zoom: number } | null = null;
/// 总览图自动 5s 刷新 —— vueuse useIntervalFn 自动随组件卸载清理。

async function reload() {
  if (loading.value) return;
  loading.value = true;
  error.value = "";
  try {
    const g = await getOverviewGraph();
    graph.value = g;
    applyGraph(g);
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    loading.value = false;
  }
}

function onMove(moveEvent: { event: unknown; flowTransform: { x: number; y: number; zoom: number } }) {
  const ft = moveEvent.flowTransform;
  savedViewport = { x: ft.x, y: ft.y, zoom: ft.zoom };
}

function onNodeClick({ node }: { node: Node }) {
  const d = node.data as { kind?: string; id?: number; tn_id?: number | null };
  if (!d.kind || d.id == null) return;
  switch (d.kind) {
    case "upload":
      void router.push({ name: "upload", params: { uploadId: String(d.id) } });
      return;
    case "source_data_asset":
    case "promoted_data_asset":
      void router.push({ name: "data-asset", params: { dataAssetId: String(d.id) } });
      return;
    case "transformation_novel":
      void router.push({ name: "transformation-detail", params: { tnId: String(d.id) } });
      return;
    case "batch":
      // batch 跳到所属 tn 的详情页(用户从节点进入最自然的入口是工作流的"母工程")
      if (d.tn_id != null) {
        void router.push({ name: "transformation-detail", params: { tnId: String(d.tn_id) } });
      }
      return;
  }
}

function formatWc(n: number): string {
  if (n >= 10000) return `${(n / 10000).toFixed(1)}万`;
  return String(n);
}

function buildMeta(n: OverviewNode): string {
  const parts: string[] = [];
  switch (n.kind) {
    case "upload":
      if (n.subtitle) parts.push(n.subtitle);
      break;
    case "source_data_asset":
    case "promoted_data_asset":
      if (n.chapter_count != null && n.chapter_count > 0) parts.push(`${n.chapter_count} 章`);
      if (n.word_count != null && n.word_count > 0) parts.push(`${formatWc(n.word_count)} 字`);
      break;
    case "transformation_novel":
      if (n.child_count != null && n.child_count > 0) parts.push(`${n.child_count} 工作流`);
      break;
    case "batch":
      if (n.child_count != null && n.child_count > 0) parts.push(`${n.child_count} 派生`);
      break;
  }
  return parts.join(" · ");
}

function applyGraph(g: OverviewGraph) {
  const newNodes: Node[] = g.nodes.map((n) => ({
    id: n.key,
    type: n.kind,
    position: { x: 0, y: 0 },
    data: {
      kind: n.kind,
      id: n.id,
      tn_id: n.tn_id ?? null,
      title: n.title,
      subtitle: n.subtitle ?? null,
      byte_size: n.byte_size ?? null,
      status: n.status ?? null,
      meta: buildMeta(n),
    },
  }));

  const newEdges: Edge[] = g.edges.map((e: ApiEdge, i: number) => ({
    id: `${e.source}->${e.target}-${i}`,
    source: e.source,
    target: e.target,
    type: "smoothstep",
    animated: false,
    style: edgeStyle(e.kind),
  }));

  // dagre 布局
  const gLayout = new dagre.graphlib.Graph();
  gLayout.setGraph({ rankdir: "TB", nodesep: 80, ranksep: 120, marginx: 40, marginy: 40 });
  gLayout.setDefaultEdgeLabel(() => ({}));
  for (const node of newNodes) gLayout.setNode(node.id, { width: 260, height: 120 });
  for (const edge of newEdges) gLayout.setEdge(edge.source, edge.target);
  dagre.layout(gLayout);

  for (const node of newNodes) {
    const pos = gLayout.node(node.id);
    if (pos) node.position = { x: pos.x - 130, y: pos.y - 60 };
  }

  flowNodes.value = newNodes;
  flowEdges.value = newEdges;
  // 复原用户操作过的视口 —— 5s 轮询重建 nodes/edges 时不重置 zoom/pan。
  // 首次加载(savedViewport 仍为 null)走 VueFlow 的 `fit-view-on-init`,不重复 fit。
  if (savedViewport) {
    const vp = savedViewport;
    void nextTick(() => {
      setViewport(vp);
    });
  }
}

function edgeStyle(kind: ApiEdge["kind"]) {
  switch (kind) {
    case "upload_to_source_da":
      return { stroke: "#475569", strokeWidth: 1.8 };
    case "upload_to_promoted_da":
      // structural —— 派生资产"属于哪个上传文件"的结构关系,工作流被删时也保留。
      // 虚线 + 浅灰跟"主路径"实线区分,视觉层级:实线=过程,虚线=归属。
      return { stroke: "#94a3b8", strokeWidth: 1.4, strokeDasharray: "6 4" };
    case "da_to_tn":
      return { stroke: "#1E40AF", strokeWidth: 1.8 };
    case "tn_to_batch":
      return { stroke: "#7C2D12", strokeWidth: 1.8 };
    case "batch_to_promoted_da":
      return { stroke: "#047857", strokeWidth: 1.8 };
  }
}

/// 5s 轮询 —— immediate:false 让首次不立即触发,挂载期 reload() 后再 resume() 启动。
const overviewPoll = useIntervalFn(() => void reload(), 5000, { immediate: false, immediateCallback: false });

onMounted(() => {
  void reload();
  overviewPoll.resume();
});
</script>

<style scoped>
.overview {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}
.stats-row {
  display: grid;
  grid-template-columns: repeat(5, minmax(120px, 1fr));
  gap: 12px;
  margin-bottom: 12px;
}
.stat-card {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 10px;
  padding: 14px 16px;
}
.stat-card .num {
  font-size: 26px;
  font-weight: 600;
  font-family: var(--font-serif);
  line-height: 1.1;
}
.stat-card .lbl {
  margin-top: 4px;
  font-size: 12px;
  color: var(--text-secondary);
}
.stat-card.run .num { color: #2563EB; }
.stat-card.fail .num { color: #DC2626; }
.cy-container {
  flex: 1 1 auto;
  min-height: 0;
  border: 1px solid var(--border-color);
  border-radius: 10px;
  background: var(--bg-card);
}
.hint,
.placeholder,
.error {
  margin-top: 8px;
  padding: 8px 12px;
  border-radius: 6px;
  font-size: 13px;
}
.hint    { background: var(--bg-hover); color: var(--text-secondary); }
.error   { background: #FEE2E2; color: #991B1B; }
.placeholder { color: var(--text-muted); }
</style>