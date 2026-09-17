<script setup lang="ts">
const { color, select } = useBrandColor();
const open = ref(false);
const root = ref<HTMLElement>();

const current = computed(() => brandPresets.find((preset) => preset.color === color.value));

function choose(next: string) {
  select(next);
  open.value = false;
}

function onPointerDown(event: PointerEvent) {
  if (open.value && root.value && !root.value.contains(event.target as Node)) open.value = false;
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') open.value = false;
}

onMounted(() => {
  document.addEventListener('pointerdown', onPointerDown);
  document.addEventListener('keydown', onKeydown);
});

onBeforeUnmount(() => {
  document.removeEventListener('pointerdown', onPointerDown);
  document.removeEventListener('keydown', onKeydown);
});
</script>

<template>
  <div ref="root" class="relative">
    <button
      type="button"
      class="flex size-9 items-center justify-center rounded-(--ui-radius) border border-(--ui-border) hover:bg-(--ui-surface-hover)"
      :aria-expanded="open"
      aria-haspopup="true"
      :title="`主题色：${current?.name ?? ''}`"
      aria-label="切换主题色"
      @click="open = !open"
    >
      <span class="size-4 rounded-full bg-(--ui-primary)" />
    </button>

    <div
      v-if="open"
      class="absolute end-0 top-full z-50 mt-2 w-60 rounded-(--ui-radius) border border-(--ui-border) bg-(--ui-surface) p-3 shadow-(--ui-shadow)"
    >
      <p class="mb-2 text-xs text-(--ui-fg-muted)">
        主题色 · <span class="text-(--ui-fg)">{{ current?.name }}</span>
      </p>
      <div class="grid grid-cols-5 gap-2">
        <button
          v-for="preset in brandPresets"
          :key="preset.id"
          type="button"
          class="size-9 rounded-full border-2 transition-transform hover:scale-110"
          :class="
            preset.color === color
              ? 'border-(--ui-fg-strong) ring-2 ring-(--ui-bg) ring-inset'
              : 'border-transparent'
          "
          :style="{ background: preset.color }"
          :title="preset.name"
          :aria-label="preset.name"
          :aria-pressed="preset.color === color"
          @click="choose(preset.color)"
        />
      </div>
    </div>
  </div>
</template>
