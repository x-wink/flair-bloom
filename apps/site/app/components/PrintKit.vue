<script setup lang="ts">
// 打印物料会被转发到论坛、群聊，那边对站外链接与二维码有限制：主体只做宣传与说明，
// 不放二维码、不引导跳转，每页底部只留一行不起眼的纯文本下载地址。
// 地址按打印时所在页面现算：在哪个站点打印就指向哪个站点。预渲染时拿到的是构建机地址，所以挂载后再取
const baseURL = useRuntimeConfig().app.baseURL;
const downloadAddress = ref('');

// 海报只放前四条作者写的纯骚话，不带摘自群聊的玩家评论；四条一行摆不下，两两错落排两行
const posterRumors = rumors.slice(0, 4);

// 纸上没有暗色：打印前临时切浅色，打完还原。只动 html 上的属性，不写用户存的偏好
let previousTheme: string | undefined;
let previousScheme = '';

function beforePrint() {
  const root = document.documentElement;
  previousTheme = root.dataset.theme;
  previousScheme = root.style.colorScheme;
  root.dataset.theme = 'light';
  root.style.colorScheme = 'light';
}

function afterPrint() {
  const root = document.documentElement;
  if (previousTheme === undefined) delete root.dataset.theme;
  else root.dataset.theme = previousTheme;
  root.style.colorScheme = previousScheme;
}

onMounted(() => {
  downloadAddress.value = `${window.location.host}${baseURL}download`;
  window.addEventListener('beforeprint', beforePrint);
  window.addEventListener('afterprint', afterPrint);
});

onBeforeUnmount(() => {
  window.removeEventListener('beforeprint', beforePrint);
  window.removeEventListener('afterprint', afterPrint);
});
</script>

<template>
  <div class="hidden print:block" aria-hidden="true">
    <!-- 第一页：宣发海报 -->
    <section class="print-page flex flex-col">
      <div class="print-glow" />
      <div class="relative flex flex-1 flex-col items-center text-center">
        <p
          class="rounded-full bg-(--ui-primary)/12 px-4 py-1 text-[11pt] font-medium text-(--ui-primary)"
        >
          PVE 打本按键小助手 · 有效降低输入延迟
        </p>
        <h1 class="mt-5 text-[40pt] leading-tight font-bold text-(--ui-fg-strong)">
          气质花 <span class="text-(--ui-primary)">FlairBloom</span>
        </h1>
        <p class="mt-3 text-[13pt] text-(--ui-fg-muted)">
          手搓长按等 CD、武学助手 FFF 启动、一键宏启动、多段宏切换……让手指歇会儿。
        </p>
        <img src="/icon.png" alt="" class="mt-6 size-[48mm] drop-shadow-xl" />

        <ul class="mt-8 grid w-full grid-cols-2 gap-[5mm] text-start">
          <li
            v-for="item in highlights"
            :key="item.title"
            class="ui-glow-card print-avoid flex gap-[4mm] p-[5mm]"
          >
            <HighlightIcon :name="item.icon" />
            <div>
              <h2 class="text-[13pt] font-semibold text-(--ui-fg-strong)">{{ item.title }}</h2>
              <!-- 海报按纸面排，要点连成一段读着更省地方，页面上才逐条列 -->
              <p class="mt-1 text-[9.5pt] leading-relaxed text-(--ui-fg-muted)">
                {{ item.points.join('') }}
              </p>
            </div>
          </li>
        </ul>

        <ul class="mt-auto flex w-full flex-wrap justify-center gap-x-[4mm] gap-y-[3mm] pt-[5mm]">
          <li
            v-for="(rumor, index) in posterRumors"
            :key="rumor.text"
            class="rounded-(--ui-radius) border border-(--ui-primary)/40 bg-(--ui-primary)/8 px-[4mm] py-[2.5mm] text-[9.5pt] text-(--ui-fg-strong)"
            :style="{ rotate: `${index % 2 ? 1.5 : -1.5}deg` }"
          >
            “{{ rumor.text }}”
          </li>
        </ul>
      </div>

      <div
        class="relative mt-[6mm] flex items-center gap-[6mm] rounded-(--ui-radius) bg-(--ui-primary) p-[6mm] text-(--ui-primary-fg)"
      >
        <div class="flex-1">
          <p class="text-[20pt] font-bold">骚话谷出品，必属精品</p>
          <p class="mt-1 text-[11pt] opacity-90">
            PVE 打本按键小助手 · Windows 10 / 11（64 位）· 免费使用
          </p>
          <p class="mt-2 text-[9pt] opacity-80">20 种门派色随心换，这张海报用的就是其中一种。</p>
        </div>
        <div class="size-[30mm] shrink-0 rounded-[4mm] bg-white p-[2mm]">
          <img src="/icon.png" alt="" class="size-full" />
        </div>
      </div>
      <p class="relative mt-[3mm] text-center text-[8.5pt] text-(--ui-fg-muted)">
        下载地址：{{ downloadAddress }}
      </p>
    </section>

    <!-- 第二页：使用说明 -->
    <section class="print-page flex flex-col text-[10pt]">
      <header class="flex items-center gap-[3mm] border-b-2 border-(--ui-primary) pb-[4mm]">
        <img src="/icon.png" alt="" class="size-[12mm]" />
        <div class="flex-1">
          <p class="text-[18pt] font-bold text-(--ui-fg-strong)">气质花 FlairBloom 使用说明</p>
          <p class="text-[9pt] text-(--ui-fg-muted)">PVE 打本按键小助手 · Windows 10 / 11（64 位）</p>
        </div>
      </header>

      <h2 class="print-heading">能帮剑三玩家干嘛</h2>
      <ul class="grid grid-cols-3 gap-[3mm]">
        <li v-for="item in highlights" :key="item.title" class="ui-glow-card print-avoid p-[3.5mm]">
          <p class="font-semibold text-(--ui-fg-strong)">{{ item.title }}</p>
          <p class="mt-1 text-[8.5pt] leading-relaxed text-(--ui-fg-muted)">
            {{ item.points.join('') }}
          </p>
        </li>
      </ul>

      <h2 class="print-heading">用着放心</h2>
      <ul class="grid grid-cols-2 gap-x-[6mm] gap-y-[2.5mm]">
        <li v-for="item in assurances" :key="item.title" class="print-avoid flex gap-[2mm]">
          <span class="font-bold text-(--ui-primary)">✓</span>
          <p>
            <span class="font-semibold text-(--ui-fg-strong)">{{ item.title }}：</span
            ><span class="text-(--ui-fg-muted)">{{ item.text }}</span>
          </p>
        </li>
      </ul>

      <h2 class="print-heading">常见问题</h2>
      <ul class="space-y-[2mm]">
        <li v-for="faq in faqs" :key="faq.title" class="print-avoid">
          <span class="font-semibold text-(--ui-fg-strong)">{{ faq.title }}</span>
          <span class="text-(--ui-fg-muted)">{{ faq.text }}</span>
        </li>
      </ul>

      <div
        class="print-avoid mt-auto rounded-(--ui-radius) border border-(--ui-warning) bg-(--ui-warning)/10 p-[4mm]"
      >
        <p class="font-semibold text-(--ui-fg-strong)">使用前请知悉</p>
        <ul class="mt-[1.5mm] list-disc space-y-[1mm] ps-[5mm] text-[9pt] text-(--ui-fg)">
          <li v-for="caution in cautions" :key="caution">{{ caution }}</li>
        </ul>
      </div>
      <p class="mt-[3mm] text-center text-[8.5pt] text-(--ui-fg-muted)">
        下载地址：{{ downloadAddress }}
      </p>
    </section>
  </div>
</template>
