<script setup lang="ts">
import { INLINE, LINK, parseBlocks, type Block } from '#markdown-parse';
import { h, type VNode } from 'vue';

const props = defineProps<{ source: string }>();

// 与应用内公告同一个解析器；这里只把块结构映射成 VNode，文本一律走子节点转义，不拼 HTML 字符串。
// 网站里链接可以真跳转，只放行 http(s)，其余协议当普通文本。
function renderInline(text: string): (VNode | string)[] {
  const out: (VNode | string)[] = [];
  let last = 0;
  for (const match of text.matchAll(INLINE)) {
    const at = match.index ?? 0;
    if (at > last) out.push(text.slice(last, at));
    const token = match[0];
    if (token.startsWith('`')) {
      out.push(
        h(
          'code',
          { class: 'rounded bg-(--ui-surface-muted) px-1 py-0.5 text-[0.9em]' },
          token.slice(1, -1),
        ),
      );
    } else if (token.startsWith('**')) {
      out.push(h('strong', { class: 'font-semibold text-(--ui-fg-strong)' }, token.slice(2, -2)));
    } else if (token.startsWith('*')) {
      out.push(h('em', token.slice(1, -1)));
    } else {
      const link = LINK.exec(token);
      const href = link?.[2];
      out.push(
        link && href && /^https?:\/\//.test(href)
          ? h(
              'a',
              {
                href,
                target: '_blank',
                rel: 'noopener',
                class: 'text-(--ui-primary) underline-offset-2 hover:underline',
              },
              link[1],
            )
          : token,
      );
    }
    last = at + token.length;
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}

function renderBlocks(blocks: Block[]): VNode[] {
  return blocks.map((block) => {
    switch (block.kind) {
      case 'break':
        return h('hr', { class: 'my-4 border-(--ui-border-muted)' });
      case 'heading':
        return h(
          block.level <= 2 ? 'h3' : 'h4',
          { class: 'mt-5 mb-2 font-semibold text-(--ui-fg-strong) first:mt-0' },
          renderInline(block.text),
        );
      case 'paragraph':
        return h('p', { class: 'my-2' }, renderInline(block.text));
      case 'list':
        return h(
          block.ordered ? 'ol' : 'ul',
          { class: [block.ordered ? 'list-decimal' : 'list-disc', 'my-2 space-y-1.5 ps-5'] },
          block.items.map((item) =>
            h('li', [
              ...renderInline(item.text),
              ...(item.children ? renderBlocks(item.children) : []),
            ]),
          ),
        );
    }
  });
}

const Notes = () => h('div', renderBlocks(parseBlocks(props.source.split(/\r?\n/))));
</script>

<template>
  <div class="text-sm leading-relaxed text-(--ui-fg)">
    <Notes />
  </div>
</template>
