<script setup lang="ts">
// 组件库离线图标子集里没有这几枚，走 Iconify 运行时会在线上去外网拉；路径取自 @iconify-json/ph
const PATHS = {
  lightning:
    'm213.85 125.46l-112 120a8 8 0 0 1-13.69-7l14.66-73.33l-57.63-21.64a8 8 0 0 1-3-13l112-120a8 8 0 0 1 13.69 7l-14.7 73.41l57.63 21.61a8 8 0 0 1 3 12.95Z',
  palette:
    'M200.77 53.89A103.27 103.27 0 0 0 128 24h-1.07A104 104 0 0 0 24 128c0 43 26.58 79.06 69.36 94.17A32 32 0 0 0 136 192a16 16 0 0 1 16-16h46.21a31.81 31.81 0 0 0 31.2-24.88a104.4 104.4 0 0 0 2.59-24a103.28 103.28 0 0 0-31.23-73.23M84 168a12 12 0 1 1 12-12a12 12 0 0 1-12 12m0-56a12 12 0 1 1 12-12a12 12 0 0 1-12 12m44-24a12 12 0 1 1 12-12a12 12 0 0 1-12 12m44 24a12 12 0 1 1 12-12a12 12 0 0 1-12 12',
  keyboard:
    'M224 48H32a16 16 0 0 0-16 16v128a16 16 0 0 0 16 16h192a16 16 0 0 0 16-16V64a16 16 0 0 0-16-16M64 168H48a8 8 0 0 1 0-16h16a8 8 0 0 1 0 16m96 0H96a8 8 0 0 1 0-16h64a8 8 0 0 1 0 16m48 0h-16a8 8 0 0 1 0-16h16a8 8 0 0 1 0 16m0-32H48a8 8 0 0 1 0-16h160a8 8 0 0 1 0 16m0-32H48a8 8 0 0 1 0-16h160a8 8 0 0 1 0 16',
  gear: 'M216 130.16q.06-2.16 0-4.32l14.92-18.64a8 8 0 0 0 1.48-7.06a107.6 107.6 0 0 0-10.88-26.25a8 8 0 0 0-6-3.93l-23.72-2.64q-1.48-1.56-3-3L186 40.54a8 8 0 0 0-3.94-6a107.3 107.3 0 0 0-26.25-10.86a8 8 0 0 0-7.06 1.48L130.16 40h-4.32L107.2 25.11a8 8 0 0 0-7.06-1.48a107.6 107.6 0 0 0-26.25 10.88a8 8 0 0 0-3.93 6l-2.64 23.76q-1.56 1.49-3 3L40.54 70a8 8 0 0 0-6 3.94a107.7 107.7 0 0 0-10.87 26.25a8 8 0 0 0 1.49 7.06L40 125.84v4.32L25.11 148.8a8 8 0 0 0-1.48 7.06a107.6 107.6 0 0 0 10.88 26.25a8 8 0 0 0 6 3.93l23.72 2.64q1.49 1.56 3 3L70 215.46a8 8 0 0 0 3.94 6a107.7 107.7 0 0 0 26.25 10.87a8 8 0 0 0 7.06-1.49L125.84 216q2.16.06 4.32 0l18.64 14.92a8 8 0 0 0 7.06 1.48a107.2 107.2 0 0 0 26.25-10.88a8 8 0 0 0 3.93-6l2.64-23.72q1.56-1.48 3-3l23.78-2.8a8 8 0 0 0 6-3.94a107.7 107.7 0 0 0 10.87-26.25a8 8 0 0 0-1.49-7.06ZM128 168a40 40 0 1 1 40-40a40 40 0 0 1-40 40',
} as const;

const props = defineProps<{ name: HighlightIconName }>();

// 组件库离线子集里有的那几枚直接交给 XIcon；子集外的图标 XIcon 会去外网拉，
// 所以只能在「子集里有」和「上面手抄 path」之间选，不能随手挑一枚 ph 图标
const ICONIFY: Partial<Record<HighlightIconName, string>> = {
  float: 'ph:corners-in',
  modes: 'ph:squares-four',
};

const path = computed(() => PATHS[props.name as keyof typeof PATHS]);
const iconify = computed(() => ICONIFY[props.name]);
</script>

<template>
  <span
    class="flex size-11 shrink-0 items-center justify-center rounded-full bg-(--ui-primary) text-(--ui-primary-fg) shadow-(--ui-shadow-primary)"
  >
    <XIcon v-if="iconify" :name="iconify" class="size-6" />
    <svg v-else viewBox="0 0 256 256" class="size-6" aria-hidden="true">
      <path fill="currentColor" :d="path" />
    </svg>
  </span>
</template>
