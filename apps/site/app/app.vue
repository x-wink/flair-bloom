<script setup lang="ts">
import type { AuroraBlob, AuroraRibbon } from '@xwink/ui';

// 彩带颜色从当前门派主色调出，换门派色时跟着变；库的缺省配色是主站自己的青紫琥珀，不属于本站
const ribbons: AuroraRibbon[] = [
  {
    left: '4%',
    rotate: 14,
    color: 'color-mix(in oklab, var(--ui-primary) 30%, transparent)',
    sway: '26s',
    breathe: '13s',
  },
  {
    left: '38%',
    rotate: -9,
    color: 'color-mix(in oklab, var(--ui-secondary) 24%, transparent)',
    sway: '34s',
    swayDelay: '-9s',
    breathe: '17s',
    breatheDelay: '-5s',
  },
  {
    left: '66%',
    rotate: 18,
    color: 'color-mix(in oklab, var(--ui-primary) 20%, transparent)',
    sway: '30s',
    swayDelay: '-18s',
    breathe: '15s',
    breatheDelay: '-11s',
  },
];

// 光斑不给 color 即跟随 --ui-primary；库的缺省光斑写死了主站的琥珀色，所以这里整组给出
const blobs: AuroraBlob[] = [
  { top: '-15%', left: '-10%', size: '55vw', alpha: 0.22, duration: '16s' },
  { top: '35%', right: '-18%', size: '48vw', alpha: 0.16, duration: '22s', delay: '-6s' },
  { bottom: '-20%', left: '15%', size: '60vw', alpha: 0.12, duration: '19s', delay: '-12s' },
];
</script>

<template>
  <!-- 极光是全页固定背景层，打印版不要它 -->
  <XAurora :ribbons :blobs class="print:hidden" />
  <NuxtPage />
</template>
