/// 展示层格式化工具。统一前后端渲染口径:

///

/// - `formatWordCount` 在 Library.vue 和 parse.vue 之前各写一份,产物不一致

///   (list 写 "1.2 万字"、parse 写 "12,000 字")。集中到这里,选 parse 的版本

///   (千分位 + "字"),因为字数对作者来说要精确,不能 1 位小数截掉。

/// - `formatSize` / `formatTime` 之前只 Library.vue 用;Transform 页等后续要展示

///   字节/时间时直接复用,不用再拷一遍。

///

/// 全部 pure 函数,不依赖 Vue / Tauri / DOM;放在 `utils/` 而不是 `composables/`

/// 因为 composables 暗示会用到响应式 ref。



const ZH_LOCALE = 'zh-Hans-CN';



export function formatSize(b: number): string {

  if (b < 1024) return `${b} B`;

  if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)} KB`;

  return `${(b / 1024 / 1024).toFixed(2)} MB`;

}



/// 与后端 text::word::count 一致: 字数 = 除空白外的所有字符。

/// 包含汉字、字母、数字、标点符号(ASCII 标点 + CJK 标点)。

/// 与 Word / WPS / 网文平台 / AI 输出的字数概念一致。

export function countWords(s: string): number {

  if (s.length === 0) return 0;

  let n = 0;

  for (const c of s) {

    if (!/\s/.test(c)) n += 1;

  }

  return n;

}





/// 千分位 + "字"。`!Number.isFinite(n) || n < 0` 时显示 "?",避免负数 / NaN

/// 渲染成 "NaN 字" 之类的调试串。零值显示 "0 字"。

export function formatWordCount(n: number): string {

  if (!Number.isFinite(n) || n < 0) return '?';

  return `${n.toLocaleString(ZH_LOCALE)} 字`;

}



/// RFC3339 → "YYYY-MM-DD HH:mm:ss"(本地时区)。使用者看到的是本机时间;后端不参与日期计算,所以仅在渲染层转换。空值返回 "—" 避免展示原 ISO。
/// 与 formatTime 同样的语义:本地时区,空值显示 "—"。
export function formatDate(iso: string | null | undefined): string {
  if (!iso) return '—';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const pad = (n: number) => n.toString().padStart(2, "0");
  return d.getFullYear() + "-" + pad(d.getMonth() + 1) + "-" + pad(d.getDate());
}

/// RFC3339 → "YYYY-MM-DD HH:mm:ss"(本地时区)。使用者看到的是本机时间;后端不参与日期计算,所以仅在渲染层转换。空值返回 "—" 避免展示原 ISO。
export function formatTime(iso: string | null | undefined): string {
  if (!iso) return '—';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const pad = (n: number) => n.toString().padStart(2, "0");
  return d.getFullYear() + "-" + pad(d.getMonth() + 1) + "-" + pad(d.getDate()) + " " + pad(d.getHours()) + ":" + pad(d.getMinutes()) + ":" + pad(d.getSeconds());
}

/// RFC3339 → "YYYY-MM-DD HH:mm"(本地时区)。CatalogUpdateDialog 拉取成功时的元信息展示用 ——
/// 秒字段对"刚才拉到一份"不必要,简化为时分即可。空值返回 "—"。
export function formatTimeShort(iso: string | null | undefined): string {
  if (!iso) return '—';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const pad = (n: number) => n.toString().padStart(2, "0");
  return d.getFullYear() + "-" + pad(d.getMonth() + 1) + "-" + pad(d.getDate()) + " " + pad(d.getHours()) + ":" + pad(d.getMinutes());
}
