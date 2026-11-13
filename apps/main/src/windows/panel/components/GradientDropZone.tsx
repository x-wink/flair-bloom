import type { DragEventHandler, ReactNode } from 'react';
import './GradientDropZone.css';

interface Props {
  active: boolean;
  label: string;
  ariaLabel: string;
  icon?: ReactNode;
  hint?: ReactNode;
  tone?: 'primary' | 'danger';
  size?: 'sm' | 'md';
  onDragOver: DragEventHandler<HTMLDivElement>;
  onDrop: DragEventHandler<HTMLDivElement>;
  onDragLeave?: DragEventHandler<HTMLDivElement>;
}

export default function GradientDropZone({
  active,
  label,
  ariaLabel,
  icon,
  hint,
  tone = 'primary',
  size = 'md',
  onDragOver,
  onDrop,
  onDragLeave,
}: Props) {
  return (
    <div
      className={`gradient-drop-zone gradient-drop-zone--${tone} gradient-drop-zone--${size}${active ? ' is-over' : ''}`}
      role="region"
      aria-label={ariaLabel}
      onDragOver={(event) => {
        event.preventDefault();
        event.stopPropagation();
        event.dataTransfer.dropEffect = 'move';
        onDragOver(event);
      }}
      onDrop={(event) => {
        event.preventDefault();
        event.stopPropagation();
        onDrop(event);
      }}
      onDragLeave={onDragLeave}
    >
      {icon}
      <span aria-live="polite">{label}</span>
      {hint && <small className="gradient-drop-zone-hint">{hint}</small>}
    </div>
  );
}
