<script setup lang="ts">
const { manifest, latest, status } = useReleases();

const nsis = computed(() => latest.value?.assets.find((asset) => asset.kind === 'nsis'));
const msi = computed(() => latest.value?.assets.find((asset) => asset.kind === 'msi'));
const unavailable = computed(
  () => status.value === 'error' || (status.value === 'success' && !nsis.value),
);

const REPOSITORY = 'https://github.com/x-wink/flair-bloom';
const SHA256_PLACEHOLDER = '0'.repeat(64);

const features = [
  {
    title: '武学助手 FFF',
    text: '开了武学助手只管狂按 F？让它替你自动连 F，手指解放。',
  },
  {
    title: '一键宏启动',
    text: '一长串技能宏，按一下就持续触发，不用反复戳。',
  },
  {
    title: '手搓循环',
    text: '输出循环要一直按同一个键时，按住就交给它连，松手就停。',
  },
  {
    title: '多段宏分组',
    text: '几条切换连发放进同一分组，组内同一时刻只跑一条，按 2 自动停掉 1。',
  },
  {
    title: '所有键都能连',
    text: '键盘、鼠标左右中、两个侧键、滚轮上下随意组合，比如按住侧键连左键。',
  },
  {
    title: '多套配置随手切',
    text: '不同角色、心法各存一套，托盘右键就能切；导出 .qzh 文件分享给朋友。',
  },
];

const steps = [
  {
    title: '安装助手',
    text: '下载安装包双击安装，首次打开同意协议。玩游戏请切到「游戏模式」，授权安装驱动后重启一次。',
  },
  {
    title: '设置规则',
    text: '在「按压连发」页点「+」，点输入框后直接按要连的那个键（键盘、鼠标、滚轮都行），打开右侧开关。',
  },
  {
    title: '打开总开关',
    text: '点右下角「全局已禁用」让它变成「已启用」，按住那个键自动连发，松手停。',
  },
];

const assurances = [
  { title: '松手就停', text: '多条规则一起连会自动控总速，不会越叠越快、停不下来。' },
  { title: '不改游戏、不读内存', text: '只在系统层面监听和模拟按键，不碰游戏文件。' },
  { title: '本地离线', text: '配置加密存在本机，不上传云端，除检查更新外不联网。' },
  { title: '更新有校验', text: '自动更新走 HTTPS 并校验数字签名，镜像下载同样防掉包。' },
];
</script>

<template>
  <div class="min-h-dvh">
    <header
      class="sticky top-0 z-40 border-b border-(--ui-border-muted) bg-(--ui-bg)/85 backdrop-blur"
    >
      <div class="mx-auto flex h-14 max-w-5xl items-center gap-3 px-4 sm:px-6">
        <a href="#top" class="flex min-w-0 items-center gap-2 font-semibold text-(--ui-fg-strong)">
          <img src="/icon.png" alt="" class="size-7 rounded-md" />
          <span class="truncate">气质花按键助手</span>
        </a>
        <nav class="ms-auto hidden items-center gap-5 text-sm text-(--ui-fg-muted) sm:flex">
          <a href="#features" class="hover:text-(--ui-fg)">功能</a>
          <a href="#start" class="hover:text-(--ui-fg)">上手</a>
          <a href="#changelog" class="hover:text-(--ui-fg)">更新公告</a>
          <a href="#support" class="hover:text-(--ui-fg)">支持</a>
        </nav>
        <div class="ms-auto flex items-center gap-2 sm:ms-0">
          <!-- 预渲染时读不到本地存的明暗档，只能输出「跟随系统」选中；水合时 class 不一致 Vue 不纠正，
               旧选中态会残留，所以只在客户端渲染，占位与组件同尺寸免得头部跳动 -->
          <ClientOnly>
            <XThemeToggle :modes="['light', 'dark', 'auto']" label="" />
            <template #fallback><div class="h-[38px] w-[138px]" /></template>
          </ClientOnly>
          <BrandColorPicker />
        </div>
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

            <!-- 清单在浏览器里异步读，读到之前用同样长度的不可见文本占住两行高度，免得读完把下面整页往下推 -->
            <p class="mt-3 text-xs text-(--ui-fg-muted)">
              Windows 10 / 11（64 位）<template v-if="nsis">
                · {{ formatSize(nsis.size) }} · 发布于
                {{ formatDate(latest?.publishedAt ?? '') }} ·
                <a
                  :href="nsis.githubUrl"
                  target="_blank"
                  rel="noopener"
                  class="underline-offset-2 hover:underline"
                  >GitHub 原始链接</a
                ></template
              ><span v-else-if="!unavailable" class="invisible" aria-hidden="true">
                · 0.0 MB · 发布于 0000-00-00 · GitHub 原始链接</span
              >
            </p>
            <p
              v-if="!unavailable"
              class="mt-1 text-xs break-all text-(--ui-fg-subtle)"
              :class="{ invisible: !nsis }"
              :aria-hidden="!nsis"
            >
              SHA-256：{{ nsis?.sha256 ?? SHA256_PLACEHOLDER }}
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
          <div class="glow-card mt-8 flex flex-col gap-4 p-6 sm:flex-row sm:items-center">
            <span
              class="flex size-12 shrink-0 items-center justify-center rounded-full bg-(--ui-primary) text-(--ui-primary-fg) shadow-(--ui-shadow-primary)"
            >
              <!-- 组件库离线图标子集没有闪电，内联 Phosphor lightning-fill，避免线上去外网拉图标 -->
              <svg viewBox="0 0 256 256" class="size-6" aria-hidden="true">
                <path
                  fill="currentColor"
                  d="M215.79 118.17a8 8 0 0 0-5-5.66L153.18 90.9l14.66-73.33a8 8 0 0 0-13.69-7l-112 120a8 8 0 0 0 3 13l57.63 21.61l-14.62 73.25a8 8 0 0 0 13.69 7l112-120a8 8 0 0 0 1.94-7.26"
                />
              </svg>
            </span>
            <div>
              <h3 class="text-lg font-semibold text-(--ui-fg-strong)">有效降低输入延迟</h3>
              <p class="mt-1 text-sm text-(--ui-fg-muted)">
                技能一转好就按出去，不用盯着 CD 手搓抢时机：连发间隔最低
                10ms，比手指狂按密得多；游戏模式在驱动层注入按键，直接送进游戏。
              </p>
            </div>
          </div>
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
            另有横版键鼠图、常驻悬浮窗、全局热键、语音播报与 20
            套门派配色（右上角色块可以先试），完整说明见
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
            <li>
              它靠模拟按键工作，存在被游戏反作弊检测的风险；能不能用、会不会处罚，请自行评估承担。
            </li>
            <li>
              游戏里请用「游戏模式」，需要安装驱动并以管理员运行；个别反作弊会拦驱动，遇到就切回「通用模式」。
            </li>
            <li>暂未做代码签名，打开时被 SmartScreen 拦截请点「更多信息 → 仍要运行」。</li>
          </ul>
        </div>
      </section>

      <section class="border-t border-(--ui-border-muted) bg-(--ui-surface-muted)/40">
        <div class="mx-auto max-w-5xl px-4 py-16 sm:px-6">
          <h2 class="text-2xl font-semibold text-(--ui-fg-strong)">稳不稳 · 安不安全</h2>
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
</template>
