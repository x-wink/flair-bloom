import type { TourDef } from '../types';
import { p } from './helpers';

export const jx3: TourDef = {
  id: 'jx3',
  title: '剑网三专栏',
  summary: '查心法白皮书，复制宏',
  steps: [
    {
      id: 'entry',
      target: 'menu',
      title: '入口在 ☰ 菜单',
      body: p(
        '☰ 菜单 → 剑网三专栏，打开心法白皮书和宏的列表。',
        '列表启动时从官网取，没网也有内置的一份。',
      ),
      prepare: (host) => {
        host.closeMenus();
        host.closeJx3();
      },
    },
    {
      id: 'pickers',
      target: 'jx3-pickers',
      title: '选门派和心法',
      body: p(
        '打开时默认选中当前主题色对应的门派，门派下拉里能换；无相楼单独列在「流派」里。',
        '右边切心法，下面的白皮书和宏跟着换。',
      ),
      prepare: (host) => host.openJx3(),
    },
    {
      id: 'papers',
      target: 'jx3-papers',
      title: '白皮书',
      body: p(
        '标「✓ 作者授权」的是作者同意收录的，点开在浏览器里看原文。',
        '下面折叠着公开收录的热门帖子，写着来源站点。',
      ),
      prepare: (host) => host.openJx3(),
    },
    {
      id: 'macros',
      target: 'jx3-macros',
      title: '复制宏',
      body: p(
        '宏名按钮复制「作者#宏名」，在游戏插件里粘贴就能同步云端宏。',
        '带正文的宏点「复制」，粘贴进游戏的宏编辑。',
      ),
      prepare: (host) => host.openJx3(),
    },
    {
      id: 'finish',
      title: '写过攻略？',
      body: p(
        '还没有授权内容的心法会显示邀请，写过攻略或宏的作者可以经「联系我们」授权收录。',
        '专栏只放标题、作者和原文链接。',
      ),
      // 教程是自己开的专栏，走完顺手收掉，别把它留在屏幕上
      prepare: (host) => host.closeJx3(),
    },
  ],
};
