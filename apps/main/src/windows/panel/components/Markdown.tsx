import { type ReactNode } from 'react';
import { type Block, INLINE, LINK, parseBlocks } from './markdown-parse';
import './Markdown.css';

/**
 * 轻量 Markdown 渲染器，用于用户协议与更新公告。
 *
 * 刻意不引第三方库，也刻意不产出 HTML 字符串：
 * - 更新公告正文来自 updater 接口（网络），走 `dangerouslySetInnerHTML` 会开出 XSS 面；
 *   这里只产出 React 元素，文本一律走 children 转义，原始 HTML 当普通文本显示。
 * - 现成渲染器会输出真的 `<a href>`，在 Tauri WebView 里点一下就把面板导航走、
 *   应用变白且无法返回。本应用没有装 opener / shell 插件，故链接只显示不跳转。
 *
 * 解析在 [`./markdown-parse`]，本文件只负责把块结构映射成元素。
 */

function renderInline(text: string, key: string): ReactNode[] {
  const out: ReactNode[] = [];
  let last = 0;
  let n = 0;

  for (const match of text.matchAll(INLINE)) {
    const at = match.index ?? 0;
    if (at > last) out.push(text.slice(last, at));
    const [token, code, bold, italic] = match;

    if (code) {
      out.push(
        <code className="md-code" key={`${key}-${n}`}>
          {token.slice(1, -1)}
        </code>,
      );
    } else if (bold) {
      out.push(<strong key={`${key}-${n}`}>{token.slice(2, -2)}</strong>);
    } else if (italic) {
      out.push(<em key={`${key}-${n}`}>{token.slice(1, -1)}</em>);
    } else {
      const link = LINK.exec(token);
      if (link) {
        const [, label, href] = link;
        // 不渲染 <a>：WebView 会真的导航走。链接文本已含地址时不重复展开，
        // 完整地址放 title 供悬停查看。
        out.push(
          <span className="md-link" key={`${key}-${n}`} title={href}>
            {href.includes(label) ? label : `${label}（${href}）`}
          </span>,
        );
      } else {
        out.push(token);
      }
    }

    last = at + token.length;
    n++;
  }

  if (last < text.length) out.push(text.slice(last));
  return out;
}

function renderBlocks(blocks: Block[], key: string): ReactNode[] {
  return blocks.map((block, i) => {
    const k = `${key}-${i}`;
    switch (block.kind) {
      case 'break':
        return <hr className="md-hr" key={k} />;
      case 'heading': {
        const Tag = `h${Math.min(block.level, 6)}` as 'h1';
        return (
          <Tag className={`md-h md-h${block.level}`} key={k}>
            {renderInline(block.text, k)}
          </Tag>
        );
      }
      case 'paragraph':
        return (
          <p className="md-p" key={k}>
            {renderInline(block.text, k)}
          </p>
        );
      case 'list': {
        const Tag = block.ordered ? 'ol' : 'ul';
        return (
          <Tag className="md-list" key={k}>
            {block.items.map((item, j) => (
              <li className="md-li" key={`${k}-${j}`}>
                {renderInline(item.text, `${k}-${j}`)}
                {item.children && renderBlocks(item.children, `${k}-${j}`)}
              </li>
            ))}
          </Tag>
        );
      }
    }
  });
}

interface Props {
  /** Markdown 源文本。 */
  source: string;
  /** 附加到根容器的类名，供调用方接管字号 / 颜色。 */
  className?: string;
}

export default function Markdown({ source, className }: Props) {
  const lines = source.replace(/^﻿/, '').replace(/\r\n?/g, '\n').split('\n');
  return (
    <div className={className ? `md ${className}` : 'md'}>
      {renderBlocks(parseBlocks(lines), 'md')}
    </div>
  );
}
