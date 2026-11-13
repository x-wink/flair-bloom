import type { TourDef } from '../types';

export const settings: TourDef = {
  id: 'settings',
  title: '热键、声音与外观',
  summary: '热键声音与主题',
  steps: [
    {
      id: 'hotkeys',
      target: 'settings-hotkeys',
      title: '设置热键',
      body: ['这里设置全局热键', '按热键开始或结束'],
      prepare: (host) => {
        host.closeMenus();
        host.openSettings('hotkeys');
      },
    },
    {
      id: 'sound',
      target: 'settings-sound',
      title: '设置声音',
      body: ['这里开启提示音', '声音可按需关闭'],
      prepare: (host) => host.openSettings('sound'),
    },
    {
      id: 'theme',
      target: 'settings-general-theme',
      title: '选择外观',
      body: ['这里选择主题色', '也能切换明暗模式'],
      prepare: (host) => host.openSettings('general'),
    },
  ],
};
