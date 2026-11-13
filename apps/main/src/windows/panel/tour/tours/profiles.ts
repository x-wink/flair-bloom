import type { TourDef } from '../types';

export const profiles: TourDef = {
  id: 'profiles',
  title: '多套配置',
  summary: '切换新建与导入',
  steps: [
    {
      id: 'current',
      target: 'profile',
      title: '切换配置',
      body: ['这里切换配置', '不同角色用不同配置', '下拉也能导入配置'],
      prepare: async (host) => {
        host.closeMenus();
        await host.setLayout('vertical');
      },
    },
    {
      id: 'manage',
      target: 'settings-tabs',
      title: '管理配置',
      body: ['这里新建与导入', '也能重命名与删除'],
      prepare: (host) => host.openSettings('profiles'),
    },
    {
      id: 'share',
      title: '分享配置',
      body: ['导出后分享给朋友', '收到后导入即可'],
      prepare: (host) => host.closeSettings(),
    },
  ],
};
