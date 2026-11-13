import { useEffect, useId, useRef } from 'react';
import type { OutputStep } from '../outputSequence';
import Overlay from './Overlay';
import Button from './Button';
import Kbd from './Kbd';
import { keyLabel } from './KeyCapture';
import { CloseIcon, RecordIcon, StopIcon } from './icons';

interface Props {
  recording: boolean;
  pending: boolean;
  closing: boolean;
  steps: OutputStep[];
  message?: string | null;
  onStop: () => void;
  onAdopt: () => void;
  onRerecord: () => void;
  onDiscard: () => void;
}

export default function RecordingOverlay({
  recording,
  pending,
  closing,
  steps,
  message,
  onStop,
  onAdopt,
  onRerecord,
  onDiscard,
}: Props) {
  const headingId = useId();
  const dialogRef = useRef<HTMLDivElement>(null);
  const previewRef = useRef<HTMLOListElement>(null);

  useEffect(() => {
    const root = document.getElementById('root');
    const wasInert = root?.inert;
    const previousFocus = document.activeElement;
    if (root) root.inert = true;
    dialogRef.current?.focus();
    const containFocus = (event: FocusEvent) => {
      if (event.target instanceof Node && !dialogRef.current?.contains(event.target))
        dialogRef.current?.focus();
    };
    const containPointer = (event: PointerEvent) => {
      if (event.target instanceof Node && !dialogRef.current?.contains(event.target)) {
        event.preventDefault();
        dialogRef.current?.focus();
      }
    };
    document.addEventListener('focusin', containFocus);
    document.addEventListener('pointerdown', containPointer, true);
    return () => {
      document.removeEventListener('focusin', containFocus);
      document.removeEventListener('pointerdown', containPointer, true);
      if (root) root.inert = wasInert ?? false;
      if (previousFocus instanceof HTMLElement && previousFocus.isConnected) previousFocus.focus();
    };
  }, []);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (dialog && !dialog.contains(document.activeElement)) dialog.focus();
  }, [recording, pending, closing]);

  useEffect(() => {
    if (!recording) return;
    const onStopKey = (event: KeyboardEvent) => {
      if (event.code === 'F12' && !event.repeat) {
        event.preventDefault();
        onStop();
      }
    };
    window.addEventListener('keydown', onStopKey);
    return () => window.removeEventListener('keydown', onStopKey);
  }, [recording, onStop]);

  useEffect(() => {
    if (previewRef.current) previewRef.current.scrollTop = previewRef.current.scrollHeight;
  }, [steps.length]);

  return (
    <Overlay open mask closeOnBackdrop={false} className="recording-overlay-layer">
      <div
        ref={dialogRef}
        className="recording-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={headingId}
        tabIndex={-1}
        onKeyDown={(event) => {
          if (recording) {
            event.preventDefault();
            return;
          }
          event.stopPropagation();
          if (event.key === 'Escape') {
            event.preventDefault();
            if (!closing) onDiscard();
            return;
          }
          if (event.key !== 'Tab') return;
          const buttons =
            dialogRef.current?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)');
          if (!buttons?.length) return;
          event.preventDefault();
          const index = [...buttons].findIndex((button) => button === document.activeElement);
          const next =
            index < 0
              ? event.shiftKey
                ? buttons.length - 1
                : 0
              : (index + (event.shiftKey ? -1 : 1) + buttons.length) % buttons.length;
          buttons[next].focus();
        }}
      >
        <div className="recording-heading">
          <h2 id={headingId}>
            <RecordIcon size={15} />
            录制宏
          </h2>
          <Button
            size="sm"
            iconOnly
            variant="ghost"
            tone="neutral"
            aria-label="关闭并放弃录制"
            title="关闭并放弃录制"
            disabled={closing}
            onClick={onDiscard}
          >
            <CloseIcon size={14} />
          </Button>
        </div>
        <ol ref={previewRef} className="sequence-recording-preview" aria-label="宏序列预览">
          {steps.map((step, index) => (
            <li key={index}>
              <Kbd>{keyLabel(step.key)}</Kbd>
              <span>{step.interval_ms}ms</span>
            </li>
          ))}
        </ol>
        {message && (
          <p className="recording-notice" role="status">
            {message}
          </p>
        )}
        <div className="sequence-actions recording-actions">
          {recording ? (
            <Button
              size="sm"
              tone="neutral"
              variant="outline"
              prependIcon={<StopIcon size={12} />}
              kbd="F12"
              disabled={closing}
              onClick={onStop}
            >
              结束录制
            </Button>
          ) : (
            <>
              <Button
                className="recording-rerecord"
                size="sm"
                tone="neutral"
                variant="outline"
                prependIcon={<RecordIcon size={12} />}
                loading={pending}
                disabled={closing}
                onClick={onRerecord}
              >
                重录
              </Button>
              <Button
                size="sm"
                disabled={steps.length === 0 || pending || closing}
                onClick={onAdopt}
              >
                采用
              </Button>
            </>
          )}
        </div>
      </div>
    </Overlay>
  );
}
