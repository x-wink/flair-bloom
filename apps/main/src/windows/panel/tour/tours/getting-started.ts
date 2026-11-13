import type { TourDef } from '../types';
import { lastRule } from './helpers.ts';

export const gettingStarted: TourDef = {
  id: 'getting-started',
  title: '上手三步',
  summary: '建规则后开始连发',
  steps: [
    {
      id: 'add',
      target: ['add-toggle', 'add-hold'],
      title: '新建规则',
      body: ['选择开关或长按'],
      prepare: async (host) => {
        host.closeMenus();
        await host.setLayout('vertical');
        host.setFilter('all');
        host.openCreateMenu();
      },
      done: (now, entered) => now.rules.length > entered.rules.length,
      actionHint: '选择一种规则',
    },
    {
      id: 'configure',
      target: ['rule-key', 'rule-enable'],
      title: '设置并启用',
      body: ['点按键框录入', '打开启用开关'],
      prepare: (host) => {
        host.closeMenus();
        return lastRule(host.snapshot) ? undefined : 'skip';
      },
      done: (now, entered) =>
        now.keyCaptureSeq > entered.keyCaptureSeq &&
        now.rules.some((rule) => rule.id === lastRule(entered)?.id && rule.enabled),
      actionHint: '录键并启用规则',
    },
    {
      id: 'global',
      target: 'global',
      title: '开始运行',
      body: (host) => {
        const label = host.snapshot.ruleTriggerLabel ?? '触发键';
        const key = label !== '—' && Array.from(label).length <= 5 ? label : '触发键';
        return lastRule(host.snapshot)?.mode === 'toggle'
          ? ['点击开始运行', `按${key}开始连发`, '再按一次就停止']
          : ['点击开始运行', `按住${key}连发`, '松手即可停止'];
      },
      prepare: (host) => (host.snapshot.globalEnabled ? 'skip' : undefined),
      done: (now) => now.globalEnabled,
      actionHint: '等你开始运行',
    },
  ],
};
