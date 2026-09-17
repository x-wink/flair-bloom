import tailwindcss from '@tailwindcss/vite';
import { fileURLToPath } from 'node:url';
import {
  BRAND_STORAGE_KEY,
  brandPresets,
  brandVariables,
  defaultBrandColor,
} from './app/utils/brand.ts';

const baseURL = '/flair-bloom/';

const brandTable = Object.fromEntries(
  brandPresets.map((preset) => [preset.color, brandVariables(preset.color)]),
);

// SSG 首帧没有 data-theme 与所选门派色，等客户端补上会先闪一帧；在样式生效前按已存偏好打上。
// 明暗存储键与 @xwink/ui 的 useTheme 一致，app.xwink.fun 同源下与产品索引页共享选择；
// 门派色只认色板里的值，存储被改坏时回落默认色。
const themeBootstrap = `(function(){try{var d=document.documentElement,p=localStorage.getItem('xwink-ui:theme');if(p!=='light'&&p!=='dark'){p=matchMedia('(prefers-color-scheme: dark)').matches?'dark':'light'}d.setAttribute('data-theme',p);var t=${JSON.stringify(brandTable)},v=t[localStorage.getItem(${JSON.stringify(BRAND_STORAGE_KEY)})]||t[${JSON.stringify(defaultBrandColor)}];for(var k in v)d.style.setProperty(k,v[k])}catch(e){}})()`;

export default defineNuxtConfig({
  compatibilityDate: '2026-01-01',
  devServer: { port: 3900 },
  modules: ['@xwink/ui/nuxt'],
  alias: {
    // 与应用内更新公告共用同一个解析器，网站与应用对公告语法的支持范围不会漂
    '#markdown-parse': fileURLToPath(
      new URL('../main/src/windows/panel/components/markdown-parse.ts', import.meta.url),
    ),
  },
  app: {
    baseURL,
    head: {
      htmlAttrs: { lang: 'zh-CN' },
      title: '气质花按键助手 FlairBloom · 下载与更新公告',
      meta: [
        {
          name: 'description',
          content:
            'PVE 打本按键小助手：长按连发、一键宏与多段宏切换。Windows 10 / 11 安装包国内直连下载，附完整更新公告。',
        },
        { property: 'og:title', content: '气质花按键助手 FlairBloom' },
        { property: 'og:description', content: 'PVE 打本按键小助手，让手指歇会儿。' },
        { property: 'og:image', content: 'https://app.xwink.fun/flair-bloom/icon.png' },
      ],
      link: [{ rel: 'icon', type: 'image/png', sizes: '32x32', href: `${baseURL}favicon-32.png` }],
      script: [{ innerHTML: themeBootstrap, tagPosition: 'head' }],
    },
  },
  typescript: {
    tsConfig: {
      compilerOptions: {
        // Nuxt 4 默认开启，主应用的 tsconfig 没开；共享的 markdown-parse 按主应用口径编写，
        // 为网站改主应用代码不值当，这里对齐主应用
        noUncheckedIndexedAccess: false,
      },
    },
  },
  css: ['~/assets/css/main.css'],
  nitro: {
    prerender: { crawlLinks: false, routes: ['/'] },
  },
  $development: {
    // 本地先跑 pnpm mirror:dev 把真实发布数据同步进 .mirror，开发服务按线上同一路径提供
    nitro: {
      publicAssets: [
        { baseURL: 'releases', dir: fileURLToPath(new URL('.mirror', import.meta.url)) },
      ],
    },
  },
  vite: {
    plugins: [tailwindcss()],
    server: {
      fs: { allow: [fileURLToPath(new URL('..', import.meta.url))] },
    },
  },
});
