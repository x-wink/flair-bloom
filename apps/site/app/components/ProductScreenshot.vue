<script setup lang="ts">
const props = defineProps<{ view: 'portrait' | 'landscape' | 'floating' }>();
const baseURL = useRuntimeConfig().app.baseURL;
const appearances = ['light', 'dark'] as const;
const screenshots = {
  portrait: {
    width: 405,
    height: 720,
    alt: '竖版规则列表：开关规则、长按规则与互斥规则组',
  },
  landscape: {
    width: 1072,
    height: 603,
    alt: '横版键鼠图：键盘与鼠标按键、规则组和运行状态',
  },
  floating: {
    width: 159,
    height: 42,
    alt: '悬浮窗：连接状态、启停与展开按钮',
  },
} as const;
const screenshot = computed(() => screenshots[props.view]);
</script>

<template>
  <div
    :data-product-view="view"
    :class="view === 'floating' ? 'inline-flex max-w-full drop-shadow-lg' : 'w-full'"
  >
    <a
      v-for="appearance in appearances"
      :key="appearance"
      :href="`${baseURL}screenshots/${appearance}/${view}.png`"
      target="_blank"
      rel="noopener"
      :class="`product-screenshot-${appearance}`"
    >
      <img
        :src="`${baseURL}screenshots/${appearance}/${view}.png`"
        :alt="screenshot.alt"
        :width="screenshot.width"
        :height="screenshot.height"
        loading="lazy"
        decoding="async"
        class="product-screenshot-image"
        :class="view === 'floating' ? 'floating-screenshot' : 'rounded-lg shadow-xl'"
      />
    </a>
  </div>
</template>

<style>
.product-screenshot-image {
  display: block;
  width: auto;
  max-width: 100%;
  height: auto;
  margin-inline: auto;
}

/* 与主题首帧脚本共用 data-theme，水合前也能显示对应图片。 */
.product-screenshot-light {
  display: block;
}

.product-screenshot-dark {
  display: none;
}

[data-theme='dark'] .product-screenshot-light {
  display: none;
}

[data-theme='dark'] .product-screenshot-dark {
  display: block;
}
</style>
