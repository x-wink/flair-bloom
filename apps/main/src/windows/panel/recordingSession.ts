import type { OutputStep } from './outputSequence.ts';
import { DEFAULT_INTERVAL_MS } from './ruleDefaults.ts';

export interface RecordingSnapshot {
  session_id: number;
  recording: boolean;
  steps: OutputStep[];
  notice?: string | null;
}

interface RecordingHost {
  isCurrent: () => boolean;
  getGlobalEnabled: () => Promise<boolean>;
  confirmStop: () => Promise<boolean>;
  stopGlobal: () => Promise<void>;
  start: () => Promise<RecordingSnapshot>;
  stop: (sessionId: number) => Promise<unknown>;
}

export async function beginRecording(host: RecordingHost): Promise<RecordingSnapshot | undefined> {
  const enabled = await host.getGlobalEnabled();
  if (!host.isCurrent()) return;
  if (enabled) {
    const approved = await host.confirmStop();
    if (!approved || !host.isCurrent()) return;
    await host.stopGlobal();
    if (!host.isCurrent()) return;
  }
  const snapshot = await host.start();
  if (!host.isCurrent()) {
    await host.stop(snapshot.session_id).catch(() => {});
    return;
  }
  return snapshot;
}

export function prepareRecordingSteps(steps: OutputStep[]): OutputStep[] {
  return steps.map((step, index) => ({
    ...step,
    interval_ms: index === steps.length - 1 ? DEFAULT_INTERVAL_MS : step.interval_ms,
  }));
}
