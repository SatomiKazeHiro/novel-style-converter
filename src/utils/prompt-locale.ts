/// PromptKind 中文显示映射。
/// `compress` = 压缩(原 `压缩`);`style` = 文风转换(原简称 `文风`,精确化为 `文风转换`)。
/// 未知值 throw(per CLAUDE.md "不兜底 fallback")。
export type PromptKind = 'compress' | 'style';

export function formatPromptKind(kind: PromptKind): string {
  switch (kind) {
    case 'compress':
      return '压缩';
    case 'style':
      return '文风转换';
    default: {
      const _exhaustive: never = kind;
      throw new Error(`unknown PromptKind: ${kind}`);
    }
  }
}
