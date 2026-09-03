// 与 Rust 端 catalog 模块对齐 —— REMOTE_URL / 解析 catalog JSON 的最小类型
// 后端实现 crates/nsc-core/src/catalog/mod.rs
export const REMOTE_URL = 'https://models.dev/api.json';

// models.dev api.json 顶层 = { [providerId: string]: Provider }
// 我们只关心 Provider / Model 用得到的字段；其他字段透明放过。
export interface CatalogModel {
  id: string;
  name: string;
  reasoning?: boolean;
  temperature?: boolean;
  limit?: {
    context?: number;
    output?: number;
  };
  /** models.dev 表达"思考控制选项"的 schema:
   *  - {type:"toggle"}                二元开关(Anthropic style,我们的 OpenAI provider 不能直接表达)
   *  - {type:"effort", values:[...]}   努力等级;values 含 "none" 时可被本应用 disable_thinking 表达
   *  - {type:"budget_tokens", min,max} Anthropic token budget,本应用不表达
   *  - [] / undefined                   模型自决,无控制
   *  参考统计:4572 个推理模型中,~844 个 effort 含 "none"(本应用支持),其余走默认行为。 */
  reasoning_options?: Array<
    | { type: 'toggle' }
    | { type: 'effort'; values: string[] }
    | { type: 'budget_tokens'; min: number; max: number }
  >;
}


export interface CatalogProvider {
  id: string;
  name: string;
  // 部分 provider 有显式 base_url（如 deepseek / openrouter / MiniMax），
  // 没有时按 SDK 默认（OpenAI / Anthropic / Google 这种）让用户手填。
  api?: string;
  models: Record<string, CatalogModel>;
}

export type CatalogData = Record<string, CatalogProvider>;
