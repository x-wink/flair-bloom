import { invoke } from '@tauri-apps/api/core';
import { useCallback, useEffect, useRef, useState } from 'react';
import { keyboardKey } from './KeyCapture';
import Button from './Button';
import { useConfirm } from './ConfirmDialog';
import { RecordIcon } from './icons';
import RecordingOverlay from './RecordingOverlay';
import type { OutputStep } from '../outputSequence';
import { beginRecording, prepareRecordingSteps, type RecordingSnapshot } from '../recordingSession';

interface Props {
  onRecorded: (steps: OutputStep[]) => void;
  canAddStep: boolean;
  onAddStep: () => void;
}

const STOP_KEY = keyboardKey(0x7b);

export default function SequenceRecorder({ onRecorded, canAddStep, onAddStep }: Props) {
  const confirm = useConfirm();
  const [snapshot, setSnapshot] = useState<RecordingSnapshot>();
  const [message, setMessage] = useState<string>();
  const [pending, setPending] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [closing, setClosing] = useState(false);
  const owned = useRef<number | undefined>(undefined);
  const starting = useRef(false);
  const ending = useRef(false);
  const mounted = useRef(true);
  const attempt = useRef(0);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      attempt.current += 1;
      if (owned.current !== undefined)
        void invoke('stop_recording', { sessionId: owned.current }).catch(() => {});
    };
  }, []);

  useEffect(() => {
    if (!snapshot?.recording) return;
    let cancelled = false;
    const timer = setInterval(() => {
      if (ending.current) return;
      void invoke<RecordingSnapshot>('get_recording', { sessionId: snapshot.session_id })
        .then((next) => {
          if (!cancelled && !ending.current && owned.current === snapshot.session_id) {
            if (!next.recording) ending.current = true;
            setSnapshot(next);
          }
        })
        .catch((error) => {
          if (!cancelled && !ending.current && owned.current === snapshot.session_id) {
            ending.current = true;
            setMessage(String(error));
            setSnapshot((current) => current && { ...current, recording: false });
            void invoke('stop_recording', { sessionId: snapshot.session_id }).catch(() => {});
          }
        });
    }, 150);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [snapshot?.recording, snapshot?.session_id]);

  const start = async () => {
    if (starting.current) return;
    starting.current = true;
    const ticket = ++attempt.current;
    const isCurrent = () => mounted.current && attempt.current === ticket;
    setPending(true);
    setMessage(undefined);
    try {
      const next = await beginRecording({
        isCurrent,
        getGlobalEnabled: () => invoke<boolean>('get_global_enabled'),
        confirmStop: async () => {
          setConfirming(true);
          try {
            return await confirm({
              title: '开始录制宏',
              description: '当前全局开关已开启，需要结束运行后才能录制。',
              confirmText: '结束运行并录制',
            });
          } finally {
            if (mounted.current) setConfirming(false);
          }
        },
        stopGlobal: () => invoke('set_global_enabled', { enabled: false }),
        start: () => invoke<RecordingSnapshot>('start_recording', { stopKey: STOP_KEY }),
        stop: (sessionId) => invoke('stop_recording', { sessionId }),
      });
      if (next) {
        if (!isCurrent()) {
          void invoke('stop_recording', { sessionId: next.session_id }).catch(() => {});
          return;
        }
        owned.current = next.session_id;
        ending.current = false;
        setSnapshot(next);
      }
    } catch (error) {
      if (isCurrent()) setMessage(String(error));
    } finally {
      starting.current = false;
      if (mounted.current) setPending(false);
    }
  };

  const stop = useCallback(async () => {
    const sessionId = owned.current;
    if (sessionId === undefined || ending.current) return;
    ending.current = true;
    try {
      const next = await invoke<RecordingSnapshot>('stop_recording', { sessionId });
      if (mounted.current && owned.current === sessionId) setSnapshot(next);
    } catch (error) {
      if (mounted.current && owned.current === sessionId) {
        ending.current = false;
        setMessage(String(error));
      }
    }
  }, []);

  const reset = () => {
    ending.current = true;
    owned.current = undefined;
    setSnapshot(undefined);
    setMessage(undefined);
  };

  const discard = async () => {
    if (closing) return;
    attempt.current += 1;
    ending.current = true;
    setClosing(true);
    const sessionId = owned.current;
    if (sessionId !== undefined) await invoke('stop_recording', { sessionId }).catch(() => {});
    if (mounted.current) {
      reset();
      setClosing(false);
    }
  };

  const steps = prepareRecordingSteps(snapshot?.steps ?? []);

  return (
    <section className="sequence-recording" aria-label="录制宏">
      <div className="sequence-actions">
        <Button
          size="sm"
          tone="neutral"
          variant="outline"
          prependIcon={<RecordIcon size={12} />}
          loading={pending}
          disabled={snapshot !== undefined}
          onClick={() => void start()}
        >
          录制宏
        </Button>
        <Button
          size="sm"
          tone="neutral"
          variant="outline"
          disabled={!canAddStep || pending || snapshot !== undefined}
          onClick={onAddStep}
        >
          ＋ 添加步骤
        </Button>
      </div>
      {!snapshot && message && <p role="alert">{message}</p>}
      {snapshot && !confirming && (
        <RecordingOverlay
          recording={snapshot.recording}
          pending={pending}
          closing={closing}
          steps={steps}
          message={message || snapshot.notice}
          onStop={stop}
          onAdopt={() => {
            onRecorded(steps);
            reset();
          }}
          onRerecord={start}
          onDiscard={discard}
        />
      )}
    </section>
  );
}
