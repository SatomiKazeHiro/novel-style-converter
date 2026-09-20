import { describe, it, expect } from 'vitest';
import { PROMPT_VARIABLES } from '../utils/prompt-variables';

/// 这份清单必须与后端 `prompts::render` 的 vars 表一致 ——
/// 变量名写错不会报错(未知占位符原样保留),所以这里逐条钉住。
describe('PROMPT_VARIABLES', () => {
  it('covers exactly the variables the backend renders', () => {
    expect(PROMPT_VARIABLES.map((v) => v.token)).toEqual([
      '{{chapter_title}}',
      '{{chapter_content}}',
      '{{prev_original}}',
      '{{prev_transformed}}',
      '{{next_original}}',
      '{{novel_title}}',
      '{{prev_context_budget}}',
      '{{total_context_budget}}',
    ]);
  });

  it('gives every variable a description', () => {
    for (const v of PROMPT_VARIABLES) {
      expect(v.desc.trim().length).toBeGreaterThan(0);
    }
  });

  it('marks chapter_content as required', () => {
    const content = PROMPT_VARIABLES.find((v) => v.token === '{{chapter_content}}');
    expect(content?.desc).toContain('必填');
  });
});
