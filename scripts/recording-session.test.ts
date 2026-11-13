import assert from 'node:assert/strict';
import test from 'node:test';
import {
  beginRecording,
  prepareRecordingSteps,
} from '../apps/main/src/windows/panel/recordingSession.ts';
import { DEFAULT_INTERVAL_MS } from '../apps/main/src/windows/panel/ruleDefaults.ts';

function fixture(enabled = false) {
  const calls: string[] = [];
  let current = true;
  const snapshot = { session_id: 12, recording: true, steps: [] };
  const host = {
    isCurrent: () => current,
    getGlobalEnabled: async () => {
      calls.push('check');
      return enabled;
    },
    confirmStop: async () => {
      calls.push('confirm');
      return true;
    },
    stopGlobal: async () => {
      calls.push('stop-global');
    },
    start: async () => {
      calls.push('start');
      return snapshot;
    },
    stop: async (id: number) => {
      calls.push(`stop-session:${id}`);
    },
  };
  return {
    host,
    calls,
    snapshot,
    invalidate: () => {
      current = false;
    },
  };
}

test('全局未开启时直接开始录制，不弹确认或额外切换全局开关', async () => {
  const state = fixture();
  assert.equal(await beginRecording(state.host), state.snapshot);
  assert.deepEqual(state.calls, ['check', 'start']);
});

test('全局开启时获得确认并等全局停止完成后才开始录制', async () => {
  const state = fixture(true);
  let release!: () => void;
  state.host.stopGlobal = () =>
    new Promise<void>((resolve) => {
      release = resolve;
    });
  const result = beginRecording(state.host);
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(state.calls, ['check', 'confirm']);
  release();
  assert.equal(await result, state.snapshot);
  assert.deepEqual(state.calls, ['check', 'confirm', 'start']);
});

test('取消开始确认保留当前运行且不创建录制会话', async () => {
  const state = fixture(true);
  state.host.confirmStop = async () => false;
  assert.equal(await beginRecording(state.host), undefined);
  assert.deepEqual(state.calls, ['check']);
});

test('全局查询或停止失败时不得进入录制', async () => {
  const query = fixture();
  query.host.getGlobalEnabled = async () => {
    throw new Error('查询失败');
  };
  await assert.rejects(beginRecording(query.host), /查询失败/);
  assert.deepEqual(query.calls, []);
  const stopping = fixture(true);
  stopping.host.stopGlobal = async () => {
    throw new Error('停止失败');
  };
  await assert.rejects(beginRecording(stopping.host), /停止失败/);
  assert.deepEqual(stopping.calls, ['check', 'confirm']);
});

test('确认返回时编辑器已卸载，不停止原有运行', async () => {
  const state = fixture(true);
  state.host.confirmStop = async () => {
    state.invalidate();
    return true;
  };
  assert.equal(await beginRecording(state.host), undefined);
  assert.deepEqual(state.calls, ['check']);
});

test('查询或停止期间卸载编辑器时不开始录制', async () => {
  const checking = fixture(true);
  checking.host.getGlobalEnabled = async () => {
    checking.invalidate();
    return true;
  };
  assert.equal(await beginRecording(checking.host), undefined);
  assert.deepEqual(checking.calls, []);
  const stopping = fixture(true);
  stopping.host.stopGlobal = async () => {
    stopping.invalidate();
  };
  assert.equal(await beginRecording(stopping.host), undefined);
  assert.deepEqual(stopping.calls, ['check', 'confirm']);
});

test('开始响应迟到时仅清理该次会话，不把已卸载的编辑器重新激活', async () => {
  const state = fixture();
  state.host.start = async () => {
    state.invalidate();
    return state.snapshot;
  };
  assert.equal(await beginRecording(state.host), undefined);
  assert.deepEqual(state.calls, ['check', 'stop-session:12']);
});

test('预览和采用保留录制顺序与中间间隔，末步使用统一连发默认值', () => {
  const steps = [
    { key: { kind: 'keyboard' as const, code: 81 }, interval_ms: 120 },
    { key: { kind: 'keyboard' as const, code: 81 }, interval_ms: 250 },
    { key: { kind: 'keyboard' as const, code: 69 }, interval_ms: 1000 },
  ];
  const original = structuredClone(steps);
  const result = prepareRecordingSteps(steps);
  assert.deepEqual(result, [
    original[0],
    original[1],
    { ...original[2], interval_ms: DEFAULT_INTERVAL_MS },
  ]);
  assert.deepEqual(steps, original);
  assert.notEqual(result[0], steps[0]);
  assert.equal(prepareRecordingSteps([steps[0]])[0].interval_ms, DEFAULT_INTERVAL_MS);
  assert.deepEqual(prepareRecordingSteps([]), []);
});
