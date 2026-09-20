//! 产出比例护栏 —— 量测 LLM 输出长度与输入正文长度的比值,越界时给出说明文本。
//!
//! ## 为什么需要
//! 提示词里写了「压缩到 30%–50%」「篇幅与原文相当」,但那只是**给模型的指令**:
//! 系统从不实测输出长度,所以既无法证明提示词生效,也无法发现"压缩等于没压"
//! 或"改写把一章写没了"这类静默损坏。本模块补上量测。
//!
//! ## 为什么护栏区间与提示词目标区间不重合
//! 提示词的 30%–50% 是**引导目标**,护栏是**失败检测**。两者若重合,任何轻微偏离
//! 都会被标成异常,标签很快就失去信息量。所以护栏只抓真异常:
//! - 压缩:<15% 说明情节被删掉(压缩不是摘要);>90% 说明基本没删,任务未完成。
//! - 文风:篇幅承诺是 ±15%,护栏放宽到 <50% / >200% —— 只抓"缩写成梗概"与"注水翻倍"。
//!
//! ## 为什么只量测、不判失败、不重试
//! 自动重试会让每章最坏多花一次调用(50 章批量最坏多 50 次),而模型越界的常见原因
//! 是提示词没写好 —— 重试同一提示词大概率得到同样的结果。所以这里只产出**可见性**:
//! 说明文本落 `ai_call_logs.ratio_note`,UI 上带标签展示,由用户决定是否重跑。

use crate::models::PromptKind;

/// 压缩模式护栏:输出低于输入的这个比例 = 情节被删(压缩 ≠ 摘要)。
pub const COMPRESS_FLOOR_PCT: f64 = 15.0;
/// 压缩模式护栏:输出高于输入的这个比例 = 基本没压。
pub const COMPRESS_CEILING_PCT: f64 = 90.0;
/// 文风模式护栏:输出低于这个比例 = 被缩写成梗概。
pub const STYLE_FLOOR_PCT: f64 = 50.0;
/// 文风模式护栏:输出高于这个比例 = 注水/跑飞。
pub const STYLE_CEILING_PCT: f64 = 200.0;

/// 比例显示与比较的小数位 —— 100.0 的比值精度足够,避免浮点尾巴进日志。
const SCALE: f64 = 10.0;

/// 量测产出比例并判定是否越界。
///
/// 返回 `None` 表示正常(或无法量测):
/// - `out_chars == 0`:模型没输出内容 —— 那是失败路径,由 status/error 表达,不在这里重复。
/// - `in_chars == 0`:没有可比输入 —— `TestModel` 业务就是这种(输入是连通性探测串),
///   对它的输出长度做比例判定没有意义。
///
/// 返回 `Some(note)` 表示越界,note 直接落 `ai_call_logs.ratio_note` 供 UI 展示。
pub fn ratio_note(kind: PromptKind, in_chars: usize, out_chars: usize) -> Option<String> {
    if in_chars == 0 || out_chars == 0 {
        return None;
    }
    let pct = (out_chars as f64) * 100.0 / (in_chars as f64);
    let pct_rounded = (pct * SCALE).round() / SCALE;
    let (floor, ceiling, what) = match kind {
        PromptKind::Compress => (COMPRESS_FLOOR_PCT, COMPRESS_CEILING_PCT, "压缩"),
        PromptKind::Style => (STYLE_FLOOR_PCT, STYLE_CEILING_PCT, "改写"),
    };
    if pct < floor {
        return Some(format!(
            "{what}后仅剩原文 {pct_rounded}%(低于护栏 {floor}%),疑似丢失情节 —— 压缩不等于摘要,建议检查本章输出"
        ));
    }
    if pct > ceiling {
        return Some(format!(
            "{what}后为原文 {pct_rounded}%(高于护栏 {ceiling}%),任务可能未生效 —— 建议检查 prompt 与模型"
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 落在护栏内 → 无说明(不打扰用户)。含提示词目标区间 30%–50%。
    #[test]
    fn in_band_returns_none() {
        assert!(ratio_note(PromptKind::Compress, 1000, 300).is_none());
        assert!(ratio_note(PromptKind::Compress, 1000, 500).is_none());
        assert!(ratio_note(PromptKind::Compress, 1000, 150).is_none());
        assert!(ratio_note(PromptKind::Style, 1000, 500).is_none());
        assert!(ratio_note(PromptKind::Style, 1000, 1000).is_none());
        assert!(ratio_note(PromptKind::Style, 1000, 2000).is_none());
    }

    /// 护栏边界:正好等于上下限算正常(闭区间),越 1 个字符才报。
    #[test]
    fn band_edges_are_inclusive() {
        assert!(
            ratio_note(PromptKind::Compress, 1000, 150).is_none(),
            "15% 是护栏内"
        );
        assert!(
            ratio_note(PromptKind::Compress, 1000, 149).is_some(),
            "14.9% 越界"
        );
        assert!(
            ratio_note(PromptKind::Compress, 1000, 900).is_none(),
            "90% 是护栏内"
        );
        assert!(
            ratio_note(PromptKind::Compress, 1000, 901).is_some(),
            "90.1% 越界"
        );
    }

    /// 压缩把一章删成摘要 —— 这是护栏要抓的核心失败。
    #[test]
    fn compress_loss_is_flagged() {
        let note = ratio_note(PromptKind::Compress, 10_000, 800).expect("越界应有说明");
        assert!(note.contains("8%"), "说明要带实际比例: {note}");
        assert!(note.contains("15%"), "说明要带护栏阈值: {note}");
    }

    /// 压缩几乎没删 —— 「没压缩」也要能被发现。
    #[test]
    fn compress_no_effect_is_flagged() {
        let note = ratio_note(PromptKind::Compress, 10_000, 9_800).expect("越界应有说明");
        assert!(note.contains("98%"), "说明要带实际比例: {note}");
    }

    /// 两档护栏确实分开:同一产出比例在两个 mode 下判定不同。
    /// 30% 对压缩是正常区间(提示词要的就是 30–50%),对文风却是严重缩水(<50%)。
    #[test]
    fn bands_differ_between_kinds() {
        assert!(
            ratio_note(PromptKind::Compress, 1000, 300).is_none(),
            "压缩 30% 正常"
        );
        assert!(
            ratio_note(PromptKind::Style, 1000, 300).is_some(),
            "文风 30% 越下限"
        );

        // 文风下限是 50%:60% 通过、40% 越界;压缩下限是 15%:两者都是正常。
        assert!(ratio_note(PromptKind::Style, 1000, 600).is_none());
        assert!(ratio_note(PromptKind::Style, 1000, 400).is_some());
        assert!(ratio_note(PromptKind::Compress, 1000, 600).is_none());
        assert!(ratio_note(PromptKind::Compress, 1000, 400).is_none());
    }

    /// 文风注水翻倍以上 → 报。
    #[test]
    fn style_bloat_is_flagged() {
        assert!(ratio_note(PromptKind::Style, 1000, 2500).is_some());
        assert!(ratio_note(PromptKind::Style, 1000, 1900).is_none());
    }

    /// 无可比输入 / 空输出:不产出说明(这两种情况由 status/error 表达)。
    #[test]
    fn unmeasurable_returns_none() {
        assert!(
            ratio_note(PromptKind::Compress, 0, 500).is_none(),
            "输入为空不可比"
        );
        assert!(
            ratio_note(PromptKind::Compress, 1000, 0).is_none(),
            "输出为空走失败路径"
        );
        assert!(ratio_note(PromptKind::Style, 0, 0).is_none());
    }
}
