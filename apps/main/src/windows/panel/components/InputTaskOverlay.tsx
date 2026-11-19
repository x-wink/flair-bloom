import { useLayoutEffect, type KeyboardEventHandler, type ReactNode, type RefObject } from 'react';
import Overlay from './Overlay';

interface Props {
  dialogRef: RefObject<HTMLDivElement | null>;
  headingId: string;
  onKeyDown: KeyboardEventHandler<HTMLDivElement>;
  children: ReactNode;
}

export default function InputTaskOverlay({ dialogRef, headingId, onKeyDown, children }: Props) {
  useLayoutEffect(() => {
    const root = document.getElementById('root');
    const wasInert = root?.inert;
    const previousFocus = document.activeElement;
    const focusTask = () =>
      (
        dialogRef.current?.querySelector<HTMLElement>('[data-task-focus]') ?? dialogRef.current
      )?.focus();
    if (root) root.inert = true;
    focusTask();
    const containFocus = (event: FocusEvent) => {
      if (event.target instanceof Node && !dialogRef.current?.contains(event.target)) focusTask();
    };
    const containPointer = (event: PointerEvent) => {
      if (event.target instanceof Node && !dialogRef.current?.contains(event.target)) {
        event.preventDefault();
        focusTask();
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
  }, [dialogRef]);

  return (
    <Overlay open mask closeOnBackdrop={false} className="recording-overlay-layer">
      <div
        ref={dialogRef}
        className="recording-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={headingId}
        tabIndex={-1}
        onKeyDown={onKeyDown}
      >
        {children}
      </div>
    </Overlay>
  );
}
