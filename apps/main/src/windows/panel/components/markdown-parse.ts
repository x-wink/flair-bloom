/**
 * 用户协议与更新公告的 Markdown 解析。纯函数，不含渲染与 DOM 依赖，便于单独验证。
 *
 * 支持范围按两份文档的实际用法划定：ATX 标题、`---` 分隔线、有序 / 无序列表（可嵌套）、
 * 段落、加粗、斜体、行内代码、链接。不支持表格、引用块、围栏代码块、图片与内联嵌套
 * （如加粗里再套代码）——超出范围的语法当普通文本处理，不会报错。
 */

export const HEADING = /^(#{1,6})\s+(.+?)\s*$/;
export const THEMATIC_BREAK = /^ {0,3}(?:-{3,}|\*{3,}|_{3,})\s*$/;
export const LIST_ITEM = /^(\s*)([-*+]|\d+[.)])\s+(.*)$/;

/**
 * 内联标记，捕获组依次是代码、加粗、斜体、链接、成串星号。顺序 / 边界都不能动：
 * - `**` 必须排在 `*` 前面，否则加粗会被斜体先吃掉半边。
 * - 未闭合的 `**` 由最后的成串星号分支整串吃掉、原样输出。否则斜体会从第二个星号起匹配，
 *   把后面的正文一路吞到下一个星号（实测「未闭合 **粗体 与 *斜体* 混排」整段被吞）。
 * - 不用后行断言 `(?<!…)`：macOS 12 的系统 WebKit（≈ Safari 15）不认，整个模块解析失败，
 *   面板白屏。Safari 升级只换 Safari 自带的 WebKit，WKWebView 仍用系统的那份。
 */
export const INLINE =
  /(`[^`\n]+`)|(\*\*(?:(?!\*\*).)+\*\*)|(\*(?!\*)(?:(?!\*).)+\*(?!\*))|(\[[^\]\n]*\]\([^)\s]+\))|(\*{2,})/g;

export const LINK = /^\[([^\]\n]*)\]\(([^)\s]+)\)$/;

export interface ListItem {
  text: string;
  /** 该项下更深缩进的内容，递归解析。 */
  children?: Block[];
}

export type Block =
  | { kind: 'heading'; level: number; text: string }
  | { kind: 'paragraph'; text: string }
  | { kind: 'break' }
  | { kind: 'list'; ordered: boolean; items: ListItem[] };

/**
 * 拼接段落内的软换行。中文之间直接相连，含 ASCII 的一侧补空格——统一加空格会在中文里
 * 插进多余空隙，统一不加又会把英文单词粘成一团。
 */
function joinSoftWrap(prev: string, next: string): string {
  if (!prev) return next;
  const a = prev[prev.length - 1];
  const b = next[0];
  const cjk = (c: string) => c.charCodeAt(0) > 0x2e7f;
  return cjk(a) && cjk(b) ? prev + next : `${prev} ${next}`;
}

function isBlockStart(line: string): boolean {
  return THEMATIC_BREAK.test(line) || HEADING.test(line) || LIST_ITEM.test(line);
}

export function parseBlocks(lines: string[]): Block[] {
  const blocks: Block[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];
    if (!line.trim()) {
      i++;
      continue;
    }

    if (THEMATIC_BREAK.test(line)) {
      blocks.push({ kind: 'break' });
      i++;
      continue;
    }

    const heading = HEADING.exec(line);
    if (heading) {
      blocks.push({ kind: 'heading', level: heading[1].length, text: heading[2] });
      i++;
      continue;
    }

    const first = LIST_ITEM.exec(line);
    if (first) {
      const indent = first[1].length;
      const ordered = /\d/.test(first[2]);
      const items: ListItem[] = [];

      while (i < lines.length) {
        const cur = lines[i];
        if (!cur.trim()) {
          // 空行只有在后面仍是同级或更深的列表项时才算列表内部的间隔
          const next = lines[i + 1];
          const nextItem = next === undefined ? null : LIST_ITEM.exec(next);
          if (!nextItem || nextItem[1].length < indent) break;
          i++;
          continue;
        }
        const item = LIST_ITEM.exec(cur);
        if (!item || item[1].length < indent) break;

        if (item[1].length > indent) {
          // 更深缩进：整段收集后按自身最小缩进对齐，递归解析为上一项的子块
          const nested: string[] = [];
          while (i < lines.length) {
            const deeper = LIST_ITEM.exec(lines[i]);
            const indented = lines[i].length - lines[i].trimStart().length > indent;
            if (!lines[i].trim() || (!deeper && !indented)) break;
            if (deeper && deeper[1].length <= indent) break;
            nested.push(lines[i]);
            i++;
          }
          const base = Math.min(...nested.map((l) => l.length - l.trimStart().length));
          const parent = items[items.length - 1];
          if (parent) parent.children = parseBlocks(nested.map((l) => l.slice(base)));
          continue;
        }

        items.push({ text: item[3] });
        i++;
      }

      blocks.push({ kind: 'list', ordered, items });
      continue;
    }

    let text = '';
    while (i < lines.length && lines[i].trim() && !isBlockStart(lines[i])) {
      text = joinSoftWrap(text, lines[i].trim());
      i++;
    }
    blocks.push({ kind: 'paragraph', text });
  }

  return blocks;
}
