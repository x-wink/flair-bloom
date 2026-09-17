<script setup lang="ts">
import { useTheme } from '@xwink/ui';

const { manifest, latest, status } = useReleases();
const { isDark, preference: themePreference, toggle: toggleTheme } = useTheme();

const themeModes = [
  { value: 'light', label: '浅色', icon: 'ph:sun-dim' },
  { value: 'dark', label: '深色', icon: 'ph:moon' },
  { value: 'auto', label: '跟随系统', icon: 'ph:desktop' },
] as const;

// 预渲染时读不到本地存的明暗档，服务端只能按「跟随系统」出选中态；水合时 class 不一致 Vue 不纠正会残留。
// 选中高亮等挂载后再给，挂载前两端都是「无选中」，挂载后是正常的响应式更新
const mounted = ref(false);
onMounted(() => {
  mounted.value = true;
});

const nsis = computed(() => latest.value?.assets.find((asset) => asset.kind === 'nsis'));
const msi = computed(() => latest.value?.assets.find((asset) => asset.kind === 'msi'));
const unavailable = computed(
  () => status.value === 'error' || (status.value === 'success' && !nsis.value),
);

const { color: brandColor, select: selectBrand } = useBrandColor();
const currentBrand = computed(() => brandPresets.find((preset) => preset.color === brandColor.value));
</script>

<template>
  <div>
  <!-- 打印时整页换成 PrintKit 的海报加说明书排版，屏幕版不参与打印 -->
  <PrintKit />
  <div class="min-h-dvh print:hidden">
    <header
      class="sticky top-0 z-40 border-b border-(--ui-border-muted) bg-(--ui-bg)/85 backdrop-blur"
    >
      <div class="mx-auto flex h-14 max-w-5xl items-center gap-3 px-4 sm:px-6">
        <a href="#top" class="flex min-w-0 items-center gap-2 font-semibold text-(--ui-fg-strong)">
          <img src="/icon.png" alt="" class="size-7 rounded-md" />
          <span class="truncate text-(--ui-primary)">气质花按键助手</span>
        </a>
        <nav class="ms-auto hidden items-center gap-5 text-sm text-(--ui-fg-muted) sm:flex">
          <a href="#features" class="hover:text-(--ui-fg)">功能</a>
          <a href="#start" class="hover:text-(--ui-fg)">上手</a>
          <a href="#changelog" class="hover:text-(--ui-fg)">更新公告</a>
          <a href="#support" class="hover:text-(--ui-fg)">支持</a>
        </nav>
        <!-- 顶栏只留一键明暗翻转；三档选择与门派色块在功能区卡片里。图标取决于本地存的档位，同样只在客户端渲染 -->
        <ClientOnly>
          <button
            type="button"
            class="ms-auto flex size-9 items-center justify-center rounded-(--ui-radius) text-(--ui-fg-muted) hover:bg-(--ui-primary)/10 hover:text-(--ui-primary) sm:ms-0"
            :title="isDark ? '切换到浅色' : '切换到深色'"
            :aria-label="isDark ? '切换到浅色' : '切换到深色'"
            @click="toggleTheme"
          >
            <XIcon :name="isDark ? 'ph:moon' : 'ph:sun-dim'" class="size-5" />
          </button>
          <template #fallback><div class="ms-auto size-9 sm:ms-0" /></template>
        </ClientOnly>
      </div>
    </header>

    <main id="top">
      <section class="mx-auto max-w-5xl px-4 pt-14 pb-16 sm:px-6 sm:pt-20">
        <div class="flex flex-col items-start gap-10 md:flex-row md:items-center">
          <div class="flex-1">
            <p class="text-sm font-medium text-(--ui-primary)">PVE 打本按键小助手 · 有效降低输入延迟</p>
            <h1 class="mt-3 text-4xl font-bold tracking-tight text-(--ui-fg-strong) sm:text-5xl">
              气质花 FlairBloom
            </h1>
            <p class="mt-4 max-w-xl text-lg text-(--ui-fg-muted)">
              手搓长按等 CD、武学助手 FFF 启动、一键宏启动、多段宏切换……让手指歇会儿。
            </p>

            <div class="mt-8 flex flex-wrap items-center gap-3">
              <a
                v-if="nsis"
                :href="nsis.url"
                class="inline-flex items-center gap-2 rounded-(--ui-radius) border border-transparent bg-(--ui-primary) px-5 py-3 font-semibold text-(--ui-primary-fg) shadow-(--ui-shadow-primary) hover:opacity-90"
              >
                <XIcon name="ph:download-simple" class="size-5" />
                下载 {{ latest?.tag }}
              </a>
              <span
                v-else-if="!unavailable"
                class="inline-flex items-center gap-2 rounded-(--ui-radius) border border-(--ui-border) px-5 py-3 text-(--ui-fg-muted)"
              >
                <XIcon name="ph:spinner" class="size-5 animate-spin" />
                正在读取最新版本
              </span>
              <a
                v-else
                :href="`${REPOSITORY}/releases/latest`"
                target="_blank"
                rel="noopener"
                class="inline-flex items-center gap-2 rounded-(--ui-radius) border border-transparent bg-(--ui-primary) px-5 py-3 font-semibold text-(--ui-primary-fg) hover:opacity-90"
              >
                前往 GitHub 下载
                <XIcon name="ph:arrow-up-right" class="size-5" />
              </a>
              <a
                v-if="msi"
                :href="msi.url"
                class="text-sm text-(--ui-fg-muted) underline-offset-2 hover:text-(--ui-fg) hover:underline"
              >
                MSI 安装包
              </a>
            </div>

            <!-- 清单在浏览器里异步读，读到之前用同样长度的不可见文本占住行高，免得读完把下面整页往下推 -->
            <p class="mt-3 text-xs text-(--ui-fg-muted)">
              Windows 10 / 11（64 位）<template v-if="nsis">
                · {{ formatSize(nsis.size) }} · 发布于
                {{ formatDate(latest?.publishedAt ?? '') }} ·
                <a
                  :href="nsis.githubUrl"
                  target="_blank"
                  rel="noopener"
                  class="underline-offset-2 hover:underline"
                  >GitHub 下载</a
                ></template
              ><span v-else-if="!unavailable" class="invisible" aria-hidden="true">
                · 0.0 MB · 发布于 0000-00-00 · GitHub 下载</span
              >
            </p>
          </div>

          <img
            src="/icon.png"
            alt="气质花图标"
            width="220"
            height="220"
            class="mx-auto size-40 drop-shadow-xl sm:size-56"
          />
        </div>
      </section>

      <section id="features" class="border-t border-(--ui-border-muted) bg-(--ui-surface-muted)/40">
        <div class="mx-auto max-w-5xl px-4 py-16 sm:px-6">
          <h2 class="text-2xl font-semibold text-(--ui-fg-strong)">能帮剑三玩家干嘛</h2>
          <ul class="mt-8 grid gap-4 md:grid-cols-2">
            <li
              v-for="item in highlights"
              :key="item.title"
              class="glow-card flex flex-col gap-4 p-6 sm:flex-row"
            >
              <HighlightIcon :name="item.icon" />
              <div>
                <h3 class="text-lg font-semibold text-(--ui-fg-strong)">{{ item.title }}</h3>
                <p class="mt-1 text-sm text-(--ui-fg-muted)">
                  {{ item.text
                  }}<template v-if="item.icon === 'palette'"
                    >就在这儿试试，这个页面马上跟着变，当前是<span class="text-(--ui-fg)"
                      >{{ currentBrand?.sect }} · {{ currentBrand?.name
                      }}{{ mounted && isDark ? '（黑化版）' : '' }}</span
                    >。</template
                  >
                </p>
                <template v-if="item.icon === 'palette'">
                <div class="mt-4 flex items-center gap-2">
                  <button
                    v-for="mode in themeModes"
                    :key="mode.value"
                    type="button"
                    class="flex size-8 items-center justify-center rounded-full border-2 transition-all hover:scale-110 hover:text-(--ui-primary)"
                    :class="
                      mounted && themePreference === mode.value
                        ? 'border-(--ui-primary) bg-(--ui-primary)/15 text-(--ui-primary)'
                        : 'border-transparent bg-(--ui-primary)/5 text-(--ui-fg-muted)'
                    "
                    :title="mode.label"
                    :aria-label="mode.label"
                    :aria-pressed="mounted && themePreference === mode.value"
                    @click="themePreference = mode.value"
                  >
                    <XIcon :name="mode.icon" class="size-4" />
                  </button>
                </div>
                <div class="mt-3 flex flex-wrap gap-1.5">
                  <button
                    v-for="preset in brandPresets"
                    :key="preset.id"
                    type="button"
                    class="size-5 rounded-full border-2 transition-transform hover:scale-125"
                    :class="
                      preset.color === brandColor
                        ? 'border-(--ui-fg-strong)'
                        : 'border-transparent'
                    "
                    :style="{ background: preset.color }"
                    :title="`${preset.sect} · ${preset.name}`"
                    :aria-label="`换成${preset.sect}${preset.name}`"
                    :aria-pressed="preset.color === brandColor"
                    @click="selectBrand(preset.color)"
                  />
                </div>
                </template>
              </div>
            </li>
          </ul>
          <ul class="glow-marquee mt-4 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            <li
              v-for="(feature, index) in features"
              :key="feature.title"
              class="glow-card p-5"
              :style="{ '--i': index }"
            >
              <h3 class="font-semibold text-(--ui-fg-strong)">{{ feature.title }}</h3>
              <p class="mt-2 text-sm text-(--ui-fg-muted)">{{ feature.text }}</p>
            </li>
          </ul>
          <p class="mt-6 text-sm text-(--ui-fg-muted)">
            另有横版键鼠图、常驻悬浮窗等，完整说明见
            <a
              :href="`${REPOSITORY}#readme`"
              target="_blank"
              rel="noopener"
              class="text-(--ui-primary) underline-offset-2 hover:underline"
              >使用说明书</a
            >。
          </p>
        </div>
      </section>

      <section id="rumors" class="overflow-hidden border-b border-(--ui-border-muted)">
        <div class="mx-auto max-w-5xl px-4 py-16 sm:px-6">
          <h2 class="text-2xl font-semibold text-(--ui-fg-strong)">江湖骚话</h2>
          <p class="mt-2 text-sm text-(--ui-fg-muted)">骚话谷出品，必属精品。</p>
          <ul class="mt-10 flex flex-col">
            <li
              v-for="(rumor, index) in rumors"
              :key="rumor.text"
              class="rumor-card w-[90%] px-6 py-7 sm:w-[70%] md:w-[55%]"
              :class="{ '-mt-3 sm:-mt-6': index > 0 }"
              :style="{
                '--i': index,
                '--z': index,
                '--tilt': `${index % 2 ? 1.5 : -1.5}deg`,
                '--offset': rumor.offset,
                '--offset-sm': rumor.offsetSm,
              }"
            >
              <span
                aria-hidden="true"
                class="absolute -top-3 start-4 font-serif text-5xl leading-none text-(--ui-primary)"
                >“</span
              >
              <p class="text-(--ui-fg-strong)">{{ rumor.text }}</p>
            </li>
          </ul>
        </div>
      </section>

      <section id="start" class="mx-auto max-w-5xl px-4 py-16 sm:px-6">
        <h2 class="text-2xl font-semibold text-(--ui-fg-strong)">三步上手</h2>
        <ol class="mt-8 grid gap-4 md:grid-cols-3">
          <li v-for="(step, index) in steps" :key="step.title" class="glow-card flex gap-4 p-5">
            <span
              class="flex size-8 shrink-0 items-center justify-center rounded-full bg-(--ui-primary) text-sm font-semibold text-(--ui-primary-fg)"
              >{{ index + 1 }}</span
            >
            <div>
              <h3 class="font-semibold text-(--ui-fg-strong)">{{ step.title }}</h3>
              <p class="mt-1 text-sm text-(--ui-fg-muted)">{{ step.text }}</p>
            </div>
          </li>
        </ol>

        <div
          class="mt-10 rounded-(--ui-radius) border border-(--ui-warning) bg-(--ui-warning)/10 p-5 text-sm"
        >
          <p class="font-semibold text-(--ui-fg-strong)">使用前请知悉</p>
          <ul class="mt-2 list-disc space-y-1.5 ps-5 text-(--ui-fg)">
            <li v-for="caution in cautions" :key="caution">{{ caution }}</li>
          </ul>
        </div>
      </section>

      <section class="border-t border-(--ui-border-muted) bg-(--ui-surface-muted)/40">
        <div class="mx-auto max-w-5xl px-4 py-16 sm:px-6">
          <h2 class="text-2xl font-semibold text-(--ui-fg-strong)">用着放心</h2>
          <ul class="mt-8 grid gap-4 sm:grid-cols-2">
            <li v-for="item in assurances" :key="item.title" class="glow-card flex gap-3 p-5">
              <XIcon name="ph:check" class="mt-0.5 size-5 shrink-0 text-(--ui-success)" />
              <div>
                <h3 class="font-semibold text-(--ui-fg-strong)">{{ item.title }}</h3>
                <p class="mt-1 text-sm text-(--ui-fg-muted)">{{ item.text }}</p>
              </div>
            </li>
          </ul>
        </div>
      </section>

      <section id="changelog" class="mx-auto max-w-3xl px-4 py-16 sm:px-6">
        <h2 class="text-2xl font-semibold text-(--ui-fg-strong)">更新公告</h2>
        <p v-if="!manifest && !unavailable" class="mt-6 text-sm text-(--ui-fg-muted)">正在读取…</p>
        <p v-else-if="unavailable" class="mt-6 text-sm text-(--ui-fg-muted)">
          暂时读不到更新公告，可以到
          <a
            :href="`${REPOSITORY}/releases`"
            target="_blank"
            rel="noopener"
            class="text-(--ui-primary) underline-offset-2 hover:underline"
            >GitHub 发布页</a
          >查看。
        </p>
        <div v-else class="mt-6 space-y-3">
          <details
            v-for="(release, index) in manifest?.releases"
            :key="release.tag"
            :open="index === 0"
            class="group rounded-(--ui-radius) border border-(--ui-primary)/30 bg-(--ui-primary)/5 transition-colors open:border-(--ui-primary)/60 hover:border-(--ui-primary)/60"
          >
            <summary
              class="flex cursor-pointer list-none items-center gap-3 px-4 py-3 select-none [&::-webkit-details-marker]:hidden"
            >
              <XIcon
                name="ph:caret-right"
                class="size-4 text-(--ui-fg-muted) transition-transform group-open:rotate-90"
              />
              <span class="font-semibold text-(--ui-fg-strong)">{{ release.tag }}</span>
              <XBadge v-if="release.tag === manifest?.latest" tone="primary">最新</XBadge>
              <span class="ms-auto text-xs text-(--ui-fg-muted)">{{
                formatDate(release.publishedAt)
              }}</span>
            </summary>
            <div class="border-t border-(--ui-primary)/20 px-4 py-4">
              <ReleaseNotes :source="release.notes" />
              <p class="mt-4 flex flex-wrap gap-x-4 gap-y-1 text-xs text-(--ui-fg-muted)">
                <a
                  v-for="asset in release.assets"
                  :key="asset.name"
                  :href="asset.url"
                  class="hover:text-(--ui-fg)"
                  >{{ asset.kind === 'nsis' ? 'EXE' : 'MSI' }} · {{ formatSize(asset.size) }}</a
                >
                <a
                  :href="release.htmlUrl"
                  target="_blank"
                  rel="noopener"
                  class="hover:text-(--ui-fg)"
                  >GitHub 发布页</a
                >
              </p>
            </div>
          </details>
        </div>
      </section>

      <section id="support" class="border-t border-(--ui-border-muted) bg-(--ui-surface-muted)/40">
        <div class="mx-auto max-w-5xl px-4 py-16 sm:px-6">
          <h2 class="text-2xl font-semibold text-(--ui-fg-strong)">支持</h2>
          <div class="mt-8 grid gap-4 sm:grid-cols-2">
            <div class="glow-card p-5">
              <h3 class="font-semibold text-(--ui-fg-strong)">问题反馈</h3>
              <p class="mt-2 text-sm text-(--ui-fg-muted)">
                遇到问题或有想法，先到 GitHub Issues
                留言；崩溃时应用会提示日志路径，附上日志更好定位。站内反馈入口即将开放。
              </p>
              <a
                :href="`${REPOSITORY}/issues`"
                target="_blank"
                rel="noopener"
                class="mt-4 inline-flex items-center gap-1.5 text-sm text-(--ui-primary) hover:underline"
              >
                前往 Issues
                <XIcon name="ph:arrow-up-right" class="size-4" />
              </a>
            </div>
            <div class="glow-card p-5">
              <h3 class="font-semibold text-(--ui-fg-strong)">常见问题</h3>
              <p class="mt-2 text-sm text-(--ui-fg-muted)">
                剑三里没反应、连太快收不住、旧配置兼容这些问题，使用说明书里都有解答。
              </p>
              <a
                :href="`${REPOSITORY}#常见问题`"
                target="_blank"
                rel="noopener"
                class="mt-4 inline-flex items-center gap-1.5 text-sm text-(--ui-primary) hover:underline"
              >
                查看常见问题
                <XIcon name="ph:arrow-up-right" class="size-4" />
              </a>
            </div>
          </div>
        </div>
      </section>
    </main>

    <footer class="border-t border-(--ui-border-muted)">
      <div
        class="mx-auto flex max-w-5xl flex-wrap items-center gap-x-4 gap-y-2 px-4 py-6 text-xs text-(--ui-fg-muted) sm:px-6"
      >
        <span>© 2026 x-wink</span>
        <a
          :href="`${REPOSITORY}/blob/main/LICENSE`"
          target="_blank"
          rel="noopener"
          class="hover:text-(--ui-fg)"
          >CC BY-NC-SA 4.0</a
        >
        <a
          :href="`${REPOSITORY}/blob/main/THIRD_PARTY.md`"
          target="_blank"
          rel="noopener"
          class="hover:text-(--ui-fg)"
          >第三方声明</a
        >
        <a href="/" class="ms-auto hover:text-(--ui-fg)">更多产品</a>
      </div>
    </footer>
  </div>
  </div>
</template>
