import { createElement, Fragment } from 'react';
import Button from '../../components/Button';
import { keyLabel } from '../../components/KeyCapture';
import GroupTimeline from '../components/GroupTimeline';
import type { TourDef, TourHost, TourSnapshot } from '../types';
import { isSampleRule, p, SAMPLE_IDS, sampleRules } from './helpers';

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
 * 用户第 4、5 步按的顺序不同（最后在跑的可能是规则 A 也可能是规则 B）都能过。进入该步时复位。
 */
let pausedSeen: string | undefined;

export const groups: TourDef = {
  id: 'groups',
  title: '规则组',
  summary: '常用于多段宏自动切换',
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
            '放进同一个规则组的规则，同一时间只跑一条。',
            `切换是换人：按 ${b} 规则 B 接班、规则 A 停；长按是插队：按住 ${hold} 时规则 B 让位，松手它自己接着跑。`,
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
            '平时从「我要新建」菜单选择「规则组」，会自动加入按键为 1、2 的两条切换规则，都先停用着，设置好后再启用；也可以把已有规则拖进去。',
            '这次直接建一个现成的「示例规则组」：两条切换、一条长按，都先停用着，键位会避开你已经在用的键。',
          ),
          button('建示例组', () => {
            host.closeMenus();
            host.createSampleGroup();
          }),
        ),
      prepare: (host) => {
        host.setFilter('all');
        if (sampleRules(host.snapshot)) return 'skip';
        host.openCreateMenu();
      },
      done: (now) => !!sampleRules(now),
      actionHint: '等你点「建示例组」…',
    },
    {
      id: 'container',
      target: 'group-latest',
      title: '组容器',
      body: p(
        '规则拖进拖出就是进组出组，卡片右上角 ⋯ 里也能移入组。',
        '解散时，组内规则全部停用会默认勾选「同时删除组内规则」，按钮显示「清空并解散」；只要有规则启用就默认保留，也可自行勾选或取消勾选。',
        '组头显示名称，副标题在全局开启时显示当前运行与暂停的规则，没有规则运行则显示「未在运行」。同组只运行一条，切换替换，长按插队。',
        '箭头朝右表示折叠，朝下表示展开；运行状态统一显示在组头副标题，折叠后仍可见，全局关闭时隐藏。组底放置区仅在拖拽时以渐变覆盖层出现，不增加分组高度。',
      ),
      prepare: (host) => {
        host.setFilter('all');
        return hasAnyGroup(host.snapshot) ? undefined : 'skip';
      },
    },
    {
      id: 'switch-a',
      target: 'group-header',
      title: '让规则 A 跑起来',
      body: (host: TourHost) => {
        const [a] = sampleKeys(host.snapshot);
        return p(
          '要做三件事：把示例组三条规则的开关都勾上；点右下角「开始运行」；',
          `再按一下 ${a}——组头会显示正在运行的按键。保持这个窗口在前面，连发的键会打进前台窗口。`,
        );
      },
      prepare: (host) => (sampleRules(host.snapshot) ? undefined : 'skip'),
      done: (now) => now.runningRuleIds.includes(SAMPLE_IDS.a),
      actionHint: (now) => `等你按一下 ${sampleKeys(now)[0]}…`,
    },
    {
      id: 'switch-b',
      target: 'group-header',
      title: '换成规则 B',
      body: (host: TourHost) => {
        const [, b] = sampleKeys(host.snapshot);
        return p(`按一下 ${b}：规则 A 自动停、规则 B 接着跑，同组规则自动切换。`);
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
            '示例规则用不上了可以一键删掉。',
          ),
          // 只剩部分示例规则时也给清理入口
          host.snapshot.rules.some(isSampleRule)
            ? button('删除示例规则', () => host.deleteSampleGroup())
            : undefined,
        ),
    },
  ],
};
