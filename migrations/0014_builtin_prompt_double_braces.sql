-- Builtin prompt 模板修复：render_raw 替换 `{{var}}`(双花括号),
-- builtin.rs 历史版本误写成 `{var}`(单花括号),导致 LLM 收到字面占位符
-- (典型反应: "请提供章节内容...")。
-- render / spec / 前端校验 / 测试全都按双花括号实现,builtin 是唯一 outlier。
-- 这条 migration 把已 seeded 的两行 builtin template 替换成正确的双花括号。
-- 只动 is_builtin=1 的 builtin 行,不动用户自建的 prompt(用户自建的可能
-- 复制 builtin 时继承了坏模板,但用户编辑后属于"已知语义",不能默默覆盖)。

UPDATE prompts
SET template = '你是一名专业的小说编辑，请对以下章节进行压缩（保留关键情节和人物，去除冗余描写）。
目标压缩比例为原文的 60%~70%。

# 上一章原文
{{prev_original}}

# 上一章改写后
{{prev_transformed}}

# 下一章原文
{{next_original}}

# 当前章节
{{chapter_title}}
{{chapter_content}}

请只输出压缩后的章节正文，不要附加说明。'
WHERE name = 'compress_default' AND is_builtin = 1;

UPDATE prompts
SET template = '你是一名资深网文风格改写编辑，请将以下章节改写为更紧凑、更有张力的现代网文风格，保持人物性格和情节走向不变。

# 上下文（前几章已改写）
{{prev_transformed}}

# 当前章节
{{chapter_title}}
{{chapter_content}}

请只输出改写后的章节正文，不要附加说明。'
WHERE name = 'style_default' AND is_builtin = 1;