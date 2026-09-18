import type { TourDef } from '../types';
import { p } from './helpers';

export const profiles: TourDef = {
  id: 'profiles',
  title: '多套配置',
  summary: '新建、切换与导入导出配置',
  steps: [
    {
      id: 'current',
      target: 'profile',
      title: '当前用的是哪套',
      body: p(
        '左下角显示当前配置。一个角色一套连发键位，换角色直接换配置。',
        '改动「默认配置」时会自动另存成一套新的，默认配置本身不会被改坏。',
      ),
      prepare: async (host) => {
        host.closeMenus();
        await host.setLayout('vertical');
      },
    },
    {
      id: 'switch',
      target: 'profile',
      title: '切换和新建',
      body: p('点开它就能切换已有配置、新建一套，或者进去管理。'),
    },
    {
      id: 'manage',
      target: 'settings-tabs',
      title: '在设置里管理',
      body: p(
        '设置 → 配置文件：重命名、删除，导出成 .qzh 文件，或导入别人给的。',
        '从别的按键工具搬过来也行，它能读那些工具的 config.json。',
      ),
      prepare: (host) => host.openSettings('profiles'),
    },
    {
      id: 'tray',
      title: '游戏里不开面板也能换',
      body: p('托盘图标右键有「切换配置」，打着游戏也能直接换一套。'),
    },
    {
      id: 'finish',
      title: '分享给朋友',
      body: p(
        '导出的 .qzh 发给朋友，对方点「导入配置」选中就能用。',
        '文件是加密的，别的软件打不开。全局热键也存在配置里，换配置会跟着一起换。',
      ),
    },
  ],
};
