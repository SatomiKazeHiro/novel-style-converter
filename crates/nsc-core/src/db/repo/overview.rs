use std::sync::MutexGuard;
use rusqlite::{params, Connection};
use serde::Serialize;

use crate::error::Result;
use crate::models::DataAssetKind;

/// 总览页面单次拉取的图 + 统计的最小完整数据。
///
/// `nodes` + `edges` 严格只画 5 条正向边,绝不画回溯边:
///  - upload        -> source_da
///  - upload        -> promoted_da    (派生资产属于哪个上传文件,structural 关系,始终画)
///  - {source,promoted}_da -> transformation_novel
///  - transformation_novel  -> batch
///  - batch         -> promoted_da    (工作流存在时才有的"转换路径"边)
///
/// promoted_da 自身可以再次成为 transformation_novel 的源,所以图天然支持多代派生,
/// 结构是 DAG(不会形成环):upload 是 sink-less 起点,batch 是有入无出的中间节点,
/// da / tn 是中继节点;upload 既连 source_da 也连 promoted_da,但都向下,无回环。
///
/// 工作流被删时 batch 节点消失,batch->promoted_da 这条边也跟着消失,但
/// upload->promoted_da 始终在 —— promoted_da 永远不会变成孤儿。
///
/// `data_assets.source_data_asset_id` 是回溯指针(谁派生了我),不在图里画边,只在 tooltip / detail 显示。
#[derive(Debug, Clone, Serialize)]
pub struct OverviewGraph {
    pub nodes: Vec<OverviewNode>,
    pub edges: Vec<OverviewEdge>,
    pub stats: OverviewStats,
    /// 当前节点数(没做截断,纯前端做提示)。
    pub total_nodes_raw: i64,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OverviewNodeKind {
    Upload,
    SourceDataAsset,
    PromotedDataAsset,
    TransformationNovel,
    Batch,
}

#[derive(Debug, Clone, Serialize)]
pub struct OverviewNode {
    pub id: i64,
    /// 全局唯一 id 字符串,前端 cytoscape 用 `id` 字段(避免直接用实体 id 与其它 id 冲突)。
    /// 形如 `upload:1` / `da:7` / `tn:3` / `batch:42`。
    pub key: String,
    pub kind: OverviewNodeKind,
    pub title: String,
    pub word_count: Option<i64>,
    pub chapter_count: Option<i64>,
    pub child_count: Option<i64>,
    /// 仅 batch 有。
    pub status: Option<String>,
    /// 仅 upload 有:文件字节数,前端用 formatSize 渲染。
    pub byte_size: Option<i64>,
    /// DA:回溯来源("从 batch 42 生成")。
    pub subtitle: Option<String>,
    /// 仅 batch 有:所属 transformation_novel.id,前端点击跳转用。
    pub tn_id: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OverviewEdgeKind {
    UploadToSourceDa,
    UploadToPromotedDa,
    DaToTn,
    TnToBatch,
    BatchToPromotedDa,
}

#[derive(Debug, Clone, Serialize)]
pub struct OverviewEdge {
    /// cytoscape 用 `source` / `target` 字段,存的是 `OverviewNode.key`。
    pub source: String,
    pub target: String,
    pub kind: OverviewEdgeKind,
}

/// 顶部统计卡片所需数值。
#[derive(Debug, Clone, Default, Serialize)]
pub struct OverviewStats {
    pub upload_count: i64,
    pub data_asset_count: i64,
    pub transformation_novel_count: i64,
    /// 进行中的 batch 数(running + paused)。
    pub running_batch_count: i64,
    /// 最近 24h 失败的 batch 数。
    pub failed_recent_count: i64,
}

pub struct OverviewRepo<'a> { pub(crate) conn: MutexGuard<'a, Connection> }

impl<'a> OverviewRepo<'a> {
    /// 单次拉取整张图。5s 轮询时只走这条,避免多次 IPC 往返。
    pub fn load_graph(&self) -> Result<OverviewGraph> {
        let mut nodes: Vec<OverviewNode> = Vec::new();
        let mut edges: Vec<OverviewEdge> = Vec::new();

        self.collect_uploads(&mut nodes)?;
        self.collect_data_assets(&mut nodes, &mut edges)?;
        self.collect_transformation_novels(&mut nodes, &mut edges)?;
        self.collect_batches(&mut nodes, &mut edges)?;
        self.connect_batches_to_promoted(&mut edges)?;

        let stats = self.compute_stats()?;
        let total_nodes_raw = nodes.len() as i64;
        Ok(OverviewGraph { nodes, edges, stats, total_nodes_raw, truncated: false })
    }

    /// upload 节点 + 子节点计数(子 = 它派生的 source_da 数)。
    /// 一并把 upload -> source_da 的边建出来,避免下一次再扫一遍 data_assets。
    fn collect_uploads(&self, nodes: &mut Vec<OverviewNode>) -> Result<()> {
        let mut stmt = self.conn.prepare(
            "SELECT u.id, u.filename, u.byte_size,
                    (SELECT COUNT(*) FROM data_assets d WHERE d.upload_id = u.id AND d.kind = ?1)
             FROM uploads u
             ORDER BY u.id ASC",
        )?;
        let rows = stmt.query_map(params![DataAssetKind::Source.as_str()], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?;
        for row in rows {
            let (id, filename, byte_size, da_count) = row?;
            nodes.push(OverviewNode {
                id,
                key: format!("upload:{id}"),
                kind: OverviewNodeKind::Upload,
                title: filename,
                word_count: None,
                chapter_count: None,
                child_count: Some(da_count),
                status: None,
                byte_size: Some(byte_size),
                subtitle: None,
                tn_id: None,
            });
        }
        Ok(())
    }

    /// 所有 data_asset(包含 source + promoted)。
    /// - source_da:边上源 upload;边由 caller 在 collect_uploads 时预生成,这里补回。
    /// - promoted_da:不画入边,只从 subtitle 字段里说明"从 batch X 生成"。
    fn collect_data_assets(&self, nodes: &mut Vec<OverviewNode>, edges: &mut Vec<OverviewEdge>) -> Result<()> {
        let mut stmt = self.conn.prepare(
            "SELECT da.id, da.upload_id, da.title, da.kind, da.source_workflow_id, da.source_data_asset_id,
                    (SELECT COUNT(*) FROM chapters c WHERE c.data_asset_id = da.id) AS chap_count,
                    (SELECT COALESCE(SUM(c.word_count),0) FROM chapters c WHERE c.data_asset_id = da.id) AS wc,
                    (SELECT COUNT(*) FROM transformation_novels tn WHERE tn.data_asset_id = da.id) AS tn_count
             FROM data_assets da
             ORDER BY da.id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, Option<i64>>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)?,
            ))
        })?;
        for row in rows {
            let (id, upload_id, title, kind_s, sw_id, sda_id, chap_count, wc, tn_count) = row?;
            let kind = DataAssetKind::parse(&kind_s).unwrap_or(DataAssetKind::Source);
            let (node_kind, subtitle) = match kind {
                DataAssetKind::Source => (OverviewNodeKind::SourceDataAsset, None),
                DataAssetKind::Promoted => (
                    OverviewNodeKind::PromotedDataAsset,
                    Some(match (sw_id, sda_id) {
                        (Some(b), Some(d)) => format!("由 batch {b} 生成,源 DA {d}"),
                        (Some(b), None)    => format!("由 batch {b} 生成"),
                        _ => "由批次生成".into(),
                    }),
                ),
            };
            nodes.push(OverviewNode {
                id,
                key: format!("da:{id}"),
                kind: node_kind,
                title,
                word_count: Some(wc),
                chapter_count: Some(chap_count),
                child_count: Some(tn_count),
                status: None,
                byte_size: None,
                subtitle,
                tn_id: None,
            });
            // upload -> da 边:source 和 promoted 都画,后者是 structural 关系(始终在),
            // 前者是原始解析路径。promote 时 upload_id 物理拷贝,所以 promoted 也有值。
            if let Some(uid) = upload_id {
                let kind_edge = match kind {
                    DataAssetKind::Source    => OverviewEdgeKind::UploadToSourceDa,
                    DataAssetKind::Promoted  => OverviewEdgeKind::UploadToPromotedDa,
                };
                edges.push(OverviewEdge {
                    source: format!("upload:{uid}"),
                    target: format!("da:{id}"),
                    kind: kind_edge,
                });
            }
        }
        Ok(())
    }

    fn collect_transformation_novels(&self, nodes: &mut Vec<OverviewNode>, edges: &mut Vec<OverviewEdge>) -> Result<()> {
        let mut stmt = self.conn.prepare(
            "SELECT tn.id, tn.data_asset_id, tn.title, tn.created_at,
                    (SELECT COUNT(*) FROM batches b WHERE b.transformation_novel_id = tn.id) AS batch_count
             FROM transformation_novels tn
             ORDER BY tn.id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?;
        for row in rows {
            let (id, da_id, title, _created_at, batch_count) = row?;
            nodes.push(OverviewNode {
                id,
                key: format!("tn:{id}"),
                kind: OverviewNodeKind::TransformationNovel,
                title,
                word_count: None,
                chapter_count: None,
                child_count: Some(batch_count),
                status: None,
                byte_size: None,
                subtitle: None,
                tn_id: None,
            });
            edges.push(OverviewEdge {
                source: format!("da:{da_id}"),
                target: format!("tn:{id}"),
                kind: OverviewEdgeKind::DaToTn,
            });
        }
        Ok(())
    }

    fn collect_batches(&self, nodes: &mut Vec<OverviewNode>, edges: &mut Vec<OverviewEdge>) -> Result<()> {
        // 子节点计数 = 该 batch 派生出去的 promoted_da 数。
        let mut stmt = self.conn.prepare(
            "SELECT b.id, b.transformation_novel_id, b.label, b.status,
                    (SELECT COUNT(*) FROM data_assets d WHERE d.source_workflow_id = b.id) AS derived_count
             FROM batches b
             ORDER BY b.id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?;
        for row in rows {
            let (id, tn_id, label, status, derived_count) = row?;
            let title = label.unwrap_or_else(|| format!("工作流 #{id}"));
            nodes.push(OverviewNode {
                id,
                key: format!("batch:{id}"),
                kind: OverviewNodeKind::Batch,
                title,
                word_count: None,
                chapter_count: None,
                child_count: Some(derived_count),
                status: Some(status),
                byte_size: None,
                subtitle: None,
                tn_id: Some(tn_id),
            });
            edges.push(OverviewEdge {
                source: format!("tn:{tn_id}"),
                target: format!("batch:{id}"),
                kind: OverviewEdgeKind::TnToBatch,
            });
        }
        Ok(())
    }

    /// batch -> promoted_da 边。前面的 collect_data_assets 已经把 promoted_da 节点建好,
    /// 这里仅补边。
    fn connect_batches_to_promoted(&self, edges: &mut Vec<OverviewEdge>) -> Result<()> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source_workflow_id FROM data_assets WHERE kind = ?1 AND source_workflow_id IS NOT NULL",
        )?;
        let rows = stmt.query_map(params![DataAssetKind::Promoted.as_str()], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (da_id, batch_id) = row?;
            edges.push(OverviewEdge {
                source: format!("batch:{batch_id}"),
                target: format!("da:{da_id}"),
                kind: OverviewEdgeKind::BatchToPromotedDa,
            });
        }
        Ok(())
    }

    fn compute_stats(&self) -> Result<OverviewStats> {
        let upload_count: i64 = self.conn.query_row("SELECT COUNT(*) FROM uploads", [], |r| r.get(0))?;
        let data_asset_count: i64 = self.conn.query_row("SELECT COUNT(*) FROM data_assets", [], |r| r.get(0))?;
        let transformation_novel_count: i64 = self.conn.query_row("SELECT COUNT(*) FROM transformation_novels", [], |r| r.get(0))?;
        let running_batch_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM batches WHERE status IN ('running','paused')",
            [],
            |r| r.get(0),
        )?;
        // 仅 "Terminated" 算失败:失败策略触发系统终止。
        // "Stopped" 是用户主动停止/启动失败/启动恢复的收口,非失败;"Cancelled" 当前死状态。
        let failed_recent_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM batches
             WHERE status = 'terminated'
               AND ended_at IS NOT NULL
               AND julianday('now') - julianday(ended_at) <= 1.0",
            [],
            |r| r.get(0),
        ).unwrap_or(0);
        Ok(OverviewStats {
            upload_count,
            data_asset_count,
            transformation_novel_count,
            running_batch_count,
            failed_recent_count,
        })
    }
}


#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use crate::db::Db;
    use super::*;
    use crate::models::{
        NewBatch, NewChapter, NewDataAsset,
        NewTransformationChapter, NewTransformationNovel, NewUpload,
        OnFailurePolicy, PromptKind,
    };

    fn fresh_db() -> Arc<Db> {
        let dir = tempfile::tempdir().unwrap();
        crate::db::Db::open(&dir.path().join("test.db")).unwrap()
    }

    /// Build: upload -> source_da -> tn1 -> batch1 -> promoted_da1
    /// promoted_da1 -> tn2 -> batch2 -> promoted_da2
    /// (multi-generation chain, ensures graph is naturally DAG)
    fn seed_multi_generation(db: &crate::db::Db) -> (i64, i64, i64, i64, i64, i64) {
        let upload_id = db.uploads().insert(&NewUpload {
            sha256: "x".into(), filename: "f.txt".into(), byte_size: 10,
            file_path: "/tmp/f.txt".into(), original_text: "原文".into(), word_count: 4,
        }).unwrap();
        let da1_id = db.data_assets().insert(&NewDataAsset {
            upload_id, title: "源1".into(), source_filename: "f.txt".into(), ..Default::default()
        }).unwrap();
        let tn1_id = db.transformation_novels().insert(&NewTransformationNovel {
            data_asset_id: da1_id, title: "tn1".into(), note: "".into(),
        }).unwrap();
        let b1 = db.batches().insert(&NewBatch {
            transformation_novel_id: tn1_id, label: Some("w1".into()),
            on_failure_policy: OnFailurePolicy::PauseAndReview,
            prompt_id: 1, model_config_id: 1, mode: "compress".into(),
            ctx_prev_original: 0, ctx_prev_transformed: 0,
            ctx_next_original: 0, ctx_next_transformed: 0,
        }).unwrap();
        let c1 = db.chapters().insert(&NewChapter {
            data_asset_id: da1_id, idx: 1, title: "c1".into(),
            body: "原文".into(), word_count: 5, ..Default::default()
        }).unwrap();
        db.transformation_chapters().insert(&NewTransformationChapter {
            transformation_novel_id: tn1_id, chapter_id: c1,
            mode: PromptKind::Compress, prompt_id: 1, model_config_id: 1,
            ctx_prev_original: 0, ctx_prev_transformed: 0, ctx_next_original: 0,
            batch_id: Some(b1), style_ref_chapter_id: None,
        }).unwrap();

        // 人为构造 promoted_da(直接 insert,绕过 promotion 需要的 batch 状态等复杂链)
        let now = chrono::Utc::now().to_rfc3339();
        db.lock().execute(
            "INSERT INTO data_assets (upload_id, title, parsed_at, source_filename, kind, source_workflow_id, source_data_asset_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![upload_id, "派生1", now, "f.txt", "promoted", b1, da1_id],
        ).unwrap();
        let da2_id = db.lock().last_insert_rowid();

        let tn2_id = db.transformation_novels().insert(&NewTransformationNovel {
            data_asset_id: da2_id, title: "tn2".into(), note: "".into(),
        }).unwrap();
        let b2 = db.batches().insert(&NewBatch {
            transformation_novel_id: tn2_id, label: Some("w2".into()),
            on_failure_policy: OnFailurePolicy::PauseAndReview,
            prompt_id: 1, model_config_id: 1, mode: "compress".into(),
            ctx_prev_original: 0, ctx_prev_transformed: 0,
            ctx_next_original: 0, ctx_next_transformed: 0,
        }).unwrap();
        let c2 = db.chapters().insert(&NewChapter {
            data_asset_id: da2_id, idx: 1, title: "c2".into(),
            body: "原文".into(), word_count: 5, ..Default::default()
        }).unwrap();
        db.transformation_chapters().insert(&NewTransformationChapter {
            transformation_novel_id: tn2_id, chapter_id: c2,
            mode: PromptKind::Compress, prompt_id: 1, model_config_id: 1,
            ctx_prev_original: 0, ctx_prev_transformed: 0, ctx_next_original: 0,
            batch_id: Some(b2), style_ref_chapter_id: None,
        }).unwrap();
        db.lock().execute(
            "INSERT INTO data_assets (upload_id, title, parsed_at, source_filename, kind, source_workflow_id, source_data_asset_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![upload_id, "派生2", now, "f.txt", "promoted", b2, da2_id],
        ).unwrap();
        let da3_id = db.lock().last_insert_rowid();
        (upload_id, da1_id, da2_id, da3_id, b1, b2)
    }

    #[test]
    fn empty_db_returns_empty_graph() {
        let db = fresh_db();
        let g = db.overview().load_graph().unwrap();
        assert!(g.nodes.is_empty());
        assert!(g.edges.is_empty());
        assert_eq!(g.stats.upload_count, 0);
        assert_eq!(g.total_nodes_raw, 0);
    }

    #[test]
    fn multi_generation_produces_dag_with_5_edge_kinds() {
        let db = fresh_db();
        let (_u, da1, da2, da3, b1, b2) = seed_multi_generation(&db);
        let g = db.overview().load_graph().unwrap();

        // 节点:1 upload + 3 da + 2 tn + 2 batch = 8
        assert_eq!(g.nodes.len(), 8, "expected 8 nodes, got {:#?}", g.nodes);

        // 校验每个节点在图里都能找到
        let kinds: Vec<_> = g.nodes.iter().map(|n| (n.key.clone(), n.kind)).collect();
        assert!(kinds.iter().any(|(k, _)| k == "upload:1"));
        assert!(kinds.iter().any(|(k, _)| k == "da:1"));
        assert!(kinds.iter().any(|(k, _)| k == "da:2"));
        assert!(kinds.iter().any(|(k, _)| k == "da:3"));
        assert_eq!(kinds.iter().filter(|(_, k)| matches!(k, OverviewNodeKind::Batch)).count(), 2);

        // 边数:
        //   upload->da1 (source_da)
        //   upload->da2 (promoted_da)   <-- 新增的 structural 边
        //   upload->da3 (promoted_da)   <-- 新增的 structural 边
        //   da1->tn1, tn1->b1, b1->da2, da2->tn2, tn2->b2, b2->da3 = 7
        // 总共 9 条边。
        assert_eq!(g.edges.len(), 9, "expected 9 edges, got {:#?}", g.edges);

        // 验证 5 类边都存在
        let kinds: std::collections::HashSet<_> = g.edges.iter().map(|e| e.kind).collect();
        assert!(kinds.contains(&OverviewEdgeKind::UploadToSourceDa));
        assert!(kinds.contains(&OverviewEdgeKind::UploadToPromotedDa));
        assert!(kinds.contains(&OverviewEdgeKind::DaToTn));
        assert!(kinds.contains(&OverviewEdgeKind::TnToBatch));
        assert!(kinds.contains(&OverviewEdgeKind::BatchToPromotedDa));

        // 验证多代深度:从 upload 到 promoted_da3 一共 6 条边路
        // upload:1 -> da:1 -> tn:1 -> batch:1 -> da:2 -> tn:2 -> batch:2 -> da:3
        assert_eq!(b1, 1); // batch ids
        assert_eq!(b2, 2);
        assert_eq!(da1, 1);
        assert_eq!(da2, 2);
        assert_eq!(da3, 3);

        // promoted_da3 的字幕应当展示回溯来源
        let da3_node = g.nodes.iter().find(|n| n.key == "da:3").unwrap();
        let s = da3_node.subtitle.as_deref().unwrap();
        assert!(s.contains("batch 2"), "subtitle should mention source batch 2, got: {s}");
    }

    #[test]
    fn stats_count_runs_with_running_or_paused() {
        let db = fresh_db();
        let (_u, _da1, _da2, _da3, b1, b2) = seed_multi_generation(&db);
        // 把 b1 标为 running,b2 标为 paused
        use crate::models::BatchStatus;
        db.batches().set_status(b1, BatchStatus::Running).unwrap();
        db.batches().set_status(b2, BatchStatus::Paused).unwrap();

        let g = db.overview().load_graph().unwrap();
        assert_eq!(g.stats.transformation_novel_count, 2);
        assert_eq!(g.stats.data_asset_count, 3); // da1 source + da2 promoted + da3 promoted
        assert_eq!(g.stats.running_batch_count, 2); // running + paused
    }

    /// 24h 失败 batch 严格只算 Terminated;Stopped(用户主动停止/启动恢复收口)不能算失败。
    /// 修 overview.rs 之前 SQL 含 ('stopped') 会把正常停止的 batch 当成失败计入。
    #[test]
    fn failed_recent_count_only_counts_terminated() {
        let db = fresh_db();
        let (_u, _da1, _da2, _da3, b1, b2) = seed_multi_generation(&db);
        use crate::models::BatchStatus;
        db.batches().set_status(b1, BatchStatus::Terminated).unwrap();
        db.batches().set_status(b2, BatchStatus::Stopped).unwrap();

        let g = db.overview().load_graph().unwrap();
        assert_eq!(g.stats.failed_recent_count, 1, "Stopped 不应计入 24h 失败 batch");
    }
}