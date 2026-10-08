// 用户协议 / 更新公告 Markdown 内联切分的断言：`pnpm test:ui`。
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { INLINE } from '../apps/main/src/windows/panel/components/markdown-parse.ts';

const kinds = ['code', 'bold', 'italic', 'link', 'stars'] as const;

function tokens(text: string): string[] {
  return [...text.matchAll(INLINE)].map((m) => {
    const kind = kinds[m.slice(1).findIndex((g) => g !== undefined)];
    return `${kind}:${m[0]}`;
  });
}

test('常规内联标记各归其类', () => {
  assert.deepEqual(tokens('`a` **b** *c* [d](https://e.f)'), [
    'code:`a`',
    'bold:**b**',
    'italic:*c*',
    'link:[d](https://e.f)',
  ]);
});

test('未闭合的 ** 原样保留，不让斜体从第二个星号起吞正文', () => {
  assert.deepEqual(tokens('未闭合 **粗体 与 *斜体* 混排'), ['stars:**', 'italic:*斜体*']);
});

test('单个孤立星号不成标记', () => {
  assert.deepEqual(tokens('3 * 4 = 12'), []);
});

test('加粗内含单星号仍整体算加粗', () => {
  assert.deepEqual(tokens('***x**'), ['bold:***x**']);
});

// macOS 12 的系统 WebKit 不认后行断言，用了整个面板白屏。
test('不使用后行断言', () => {
  assert.doesNotMatch(INLINE.source, /\(\?<[!=]/);
});
