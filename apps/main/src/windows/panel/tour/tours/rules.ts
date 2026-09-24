import type { TourDef, TourSnapshot } from '../types';
import { p } from './helpers';

function hasHoldRule(snapshot: TourSnapshot): boolean {
  return snapshot.rules.some((r) => r.mode === 'hold');
}

export const rules: TourDef = {
  id: 'rules',
  title: '规则玩法',
  summary: '长按 / 切换、高级设置、互斥分组',
  steps: [
    {
      id: 'modes',
      target: 'filter',
      title: '两种连发各适合什么',
      body: p(
        '长按连发适合平 A、等 CD 戳键，按住才连。',
        '切换连发适合长时间挂机、采集、一键宏，按一下就一直跑。',
      ),
      prepare: async (host) => {
        host.closeMenus();
        await host.setLayout('vertical');
        host.setFilter('hold');
      },
    },
    {
      id: 'add',
      target: 'add-hold',
      title: '先加一条规则',
      body: p('后面两步要在规则卡上讲，先点这里添加一条长按连发规则。'),
      // 前置步：已经有长按规则就不出现，没有就等用户加一条，后面两步才有卡可指
      prepare: (host) => {
        host.setFilter('hold');
        return hasHoldRule(host.snapshot) ? 'skip' : undefined;
      },
      done: (now, entered) => now.rules.length > entered.rules.length,
      actionHint: '等你点一下…',
    },
    {
      id: 'card',
      target: 'rule-latest',
      title: '规则卡上有什么',
      body: p(
        '连发按键、间隔毫秒数、启用开关，左边的把手能拖动排序。',
        '多条规则一起连时软件会自动控速，不会越连越快、停不下来。',
      ),
      // 上一步被跳过、仍然没有规则卡：跳过而不是指着空处讲
      prepare: (host) => {
        host.setFilter('hold');
        return hasHoldRule(host.snapshot) ? undefined : 'skip';
      },
    },
    {
      id: 'advanced',
      target: 'rule-advanced',
      title: '高级设置：两个键分开',
      body: p(
        '展开它可以把「启动键」和「连发键」分开：按住侧键连左键、按 F 连滚轮都行。',
        '切换连发还能再单独设一个停止键。',
      ),
      prepare: (host) => {
        host.setFilter('hold');
        return hasHoldRule(host.snapshot) ? undefined : 'skip';
      },
    },
    {
      id: 'add-toggle',
      target: 'add-toggle',
      title: '添加切换连发',
      body: p('切换连发规则从这里加，列表已经替你筛到切换连发；这一步只看，不用点。'),
      prepare: (host) => host.setFilter('toggle'),
    },
    {
      id: 'group',
      target: 'add-group',
      title: '互斥分组',
      body: p(
        '把几条切换连发规则放进同一组，组内同一时刻只跑一条。',
        '多段宏就靠它：按 2 那条起来，按 1 那条自动停，不会打架。',
      ),
      prepare: (host) => host.setFilter('toggle'),
    },
    {
      id: 'finish',
      title: '按键框变色是在提醒你',
      body: p(
        '规则的按键框描边变红或变黄是冲突提醒：热键盖住了触发键、这条的连发键是另一条的启动键之类。',
        '红的会让规则失效，黄的只是可能互相干扰，换个键就恢复。',
      ),
    },
  ],
};
