-- migration 0020: transformation_novels 加 note 列。
--
-- 用户在"新建转换小说"时可填一段备注（用途、风格目标、注意事项等），
-- UI 在 TN 详情页头部标题下面只读展示，暂不提供编辑入口。
-- 空字符串等价于"无备注"。

ALTER TABLE transformation_novels
  ADD COLUMN note TEXT NOT NULL DEFAULT '';
