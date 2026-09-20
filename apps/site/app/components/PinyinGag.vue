<script setup lang="ts">
// 玩梗注音：平时淡淡地显示假拼音，悬停或划选时换成真拼音。
// 两串拼音都常驻 DOM、只切样式——换文本节点会把选区弄没，一选就跳回原样。
defineProps<{ gag: string; real: string }>();

const root = ref<HTMLElement>();
const selected = ref(false);
// 触屏没有悬停，点一下当悬停用；再点别处收回去
const tapped = ref(false);

function syncSelection() {
  const selection = window.getSelection();
  selected.value = Boolean(
    root.value && selection && !selection.isCollapsed && selection.containsNode(root.value, true),
  );
}

function onPointerDown(event: PointerEvent) {
  if (event.pointerType === 'touch') tapped.value = true;
}

function onDocumentPointerDown(event: PointerEvent) {
  if (!root.value?.contains(event.target as Node)) tapped.value = false;
}

onMounted(() => {
  document.addEventListener('selectionchange', syncSelection);
  document.addEventListener('pointerdown', onDocumentPointerDown);
});
onBeforeUnmount(() => {
  document.removeEventListener('selectionchange', syncSelection);
  document.removeEventListener('pointerdown', onDocumentPointerDown);
});

const revealed = computed(() => selected.value || tapped.value);
</script>

<template>
  <!-- inline-grid 同格叠放：容器宽度取两串里更宽的那串，切换时这行字不会跳 -->
  <span
    ref="root"
    class="group inline-grid [&>*]:col-start-1 [&>*]:row-start-1"
    @pointerdown="onPointerDown"
  >
    <span
      aria-hidden="true"
      class="line-through transition-opacity select-none group-hover:opacity-0"
      :class="revealed ? 'opacity-0' : 'opacity-10'"
      >{{ gag }}</span
    >
    <!-- select-all 让整串一起选中，划不走半个字母 -->
    <span
      class="transition-colors select-all group-hover:text-(--ui-fg)"
      :class="revealed ? 'text-(--ui-fg)' : 'text-transparent'"
      >{{ real }}</span
    >
  </span>
</template>
