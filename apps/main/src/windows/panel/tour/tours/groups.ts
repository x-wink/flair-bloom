import { createElement, Fragment } from 'react';
import Button from '../../components/Button';
import { keyLabel } from '../../components/KeyCapture';
import GroupTimeline from '../components/GroupTimeline';
import type { TourDef, TourHost, TourSnapshot } from '../types';
import { p, SAMPLE_GROUP, SAMPLE_IDS, sampleRules } from './helpers';

/** 示例组三条规则的启动键名；示例组不在时用出厂键位讲。 */
function sampleKeys(snapshot: TourSnapshot): [string, string, string] {
  const s = sampleRules(snapshot);
  if (!s) return ['1', '2', 'V'];
  return [keyLabel(s.a.trigger_key), keyLabel(s.b.trigger_key), keyLabel(s.hold.trigger_key)];
}

function hasAnyGroup(snapshot: TourSnapshot): boolean {
  return snapshot.rules.some((r) => r.group);
}

/** 气泡里的宿主动作按钮（建 / 删示例组）。 */
function button(label: string, onClick: () => void) {
  return createElement(
    'div',
    { className: 'tour-bubble__actions' },
    createElement(Button, { size: 'sm', variant: 'outline', tone: 'primary', onClick }, label),
  );
}

/**
 * 第 6 步的两段判定：先看到某条示例切换规则被暂停，再看到它恢复运行且长按已松开。
 * 用户第 4、5 步按的顺序不同（最后在跑的可能是宏A 也可能是宏B）都能过。进入该步时复位。
 */
let pausedSeen: string | undefined;

export const groups: TourDef = {
  id: 'groups',
  title: '互斥组与多段宏',
  summary: '同组只跑一条：切换是换人，长按是插队',
  steps: [
    {
      id: 'concept',
      title: '同一时间只跑一条',
      body: (host: TourHost) => {
        const [a, b, hold] = sampleKeys(host.snapshot);
        return createElement(
          Fragment,
          null,
          p(
            '放进同一个互斥组的规则，同一时间只跑一条。',
            `切换是换人：按 ${b} 宏B 接班、宏A 停；长按是插队：按住 ${hold} 时宏B 让位，松手它自己接着跑。`,
          ),
          createElement(GroupTimeline, { keys: [a, b, hold] }),
        );
      },
      prepare: async (host) => {
        host.closeMenus();
        await host.setLayout('vertical');
        host.setFilter('all');
      },
    },
    {
      id: 'sample',
      target: 'add-group',
      title: '先建一个示例组',
      body: (host: TourHost) =>
        createElement(
          Fragment,
          null,
          p(
            '平时用这里新建分组，再把规则拖进去。',
            '这次直接建一个现成的「示例·多段宏」：两条切换、一条长按，都先停用着，键位会避开你已经在用的键。',
          ),
          button('建示例组', () => host.createSampleGroup()),
        ),
      prepare: (host) => {
        host.setFilter('all');
        return sampleRules(host.snapshot) ? 'skip' : undefined;
      },
      done: (now) => !!sampleRules(now),
      actionHint: '等你点「建示例组」…',
    },
    {
      id: 'container',
      target: 'group-latest',
      title: '组容器',
      body: p(
        '规则拖进拖出就是进组出组，卡片右上角 ⋯ 里也能移入分组。',
        '组头这句话就是这个组的规则；组里只有一条规则时互斥没有意义。',
      ),
      prepare: (host) => {
        host.setFilter('all');
        return hasAnyGroup(host.snapshot) ? undefined : 'skip';
      },
    },
    {
      id: 'switch-a',
      target: 'group-header',
      title: '让宏A 跑起来',
      body: (host: TourHost) => {
        const [a] = sampleKeys(host.snapshot);
        return p(
          '要做三件事：把示例组三条规则的开关都勾上；点亮右下角的总开关；',
          `再按一下 ${a}——组头亮起「宏A 在跑」。`,
        );
      },
      prepare: (host) => (sampleRules(host.snapshot) ? undefined : 'skip'),
      done: (now) => now.runningRuleIds.includes(SAMPLE_IDS.a),
      actionHint: (now) => `等你按一下 ${sampleKeys(now)[0]}…`,
    },
    {
      id: 'switch-b',
      target: 'group-header',
      title: '换成宏B',
      body: (host: TourHost) => {
        const [, b] = sampleKeys(host.snapshot);
        return p(`按一下 ${b}：宏A 自动停、宏B 接着跑——这就是多段宏切换。`);
      },
      prepare: (host) => (sampleRules(host.snapshot) ? undefined : 'skip'),
      done: (now) =>
        now.runningRuleIds.includes(SAMPLE_IDS.b) && !now.runningRuleIds.includes(SAMPLE_IDS.a),
      actionHint: (now) => `等你按一下 ${sampleKeys(now)[1]}…`,
    },
    {
      id: 'interrupt',
      target: 'group-header',
      title: '长按插队',
      body: (host: TourHost) => {
        const [, , hold] = sampleKeys(host.snapshot);
        return p(
          `按住 ${hold} 别松：在跑的宏变成 ⏸，${hold} 开始连发。`,
          '松手后被暂停的宏自己接着跑，不用再按一次。',
        );
      },
      prepare: (host) => {
        pausedSeen = undefined;
        return sampleRules(host.snapshot) ? undefined : 'skip';
      },
      done: (now) => {
        const toggles: string[] = [SAMPLE_IDS.a, SAMPLE_IDS.b];
        pausedSeen ??= toggles.find((id) => now.pausedRuleIds.includes(id));
        return (
          !!pausedSeen &&
          now.runningRuleIds.includes(pausedSeen) &&
          !now.runningRuleIds.includes(SAMPLE_IDS.hold)
        );
      },
      actionHint: (now) => `按住 ${sampleKeys(now)[2]} 再松开…`,
    },
    {
      id: 'finish',
      title: '会了',
      body: (host: TourHost) =>
        createElement(
          Fragment,
          null,
          p(
            '长按规则不放进组，就和切换规则同时跑、互不影响；放进组才会插队。',
            '示例组用不上了可以一键删掉。',
          ),
          sampleRules(host.snapshot)
            ? button('删除示例组', () => host.deleteGroupRules(SAMPLE_GROUP))
            : undefined,
        ),
    },
  ],
};
