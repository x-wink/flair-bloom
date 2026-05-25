import type { RefObject } from 'react';
import Overlay from './Overlay';

export interface ContextMenuItem {
  label: string;
  onClick: () => void;
  danger?: boolean;
}

interface Props {
  open: boolean;
  onClose: () => void;
  target: RefObject<HTMLElement>;
  items: ContextMenuItem[];
  location?: 'bottom-left' | 'bottom-right';
}

export default function ContextMenu({
  open,
  onClose,
  target,
  items,
  location = 'bottom-right',
}: Props) {
  return (
    <Overlay open={open} onClose={onClose} target={target} location={location} mask={false}>
      <div className="ctx-menu">
        {items.map((item, i) => (
          <button
            key={`${i}-${item.label}`}
            className={`ctx-menu-item${item.danger ? ' ctx-menu-item--danger' : ''}`}
            onClick={() => {
              onClose();
              item.onClick();
            }}
          >
            {item.label}
          </button>
        ))}
      </div>
    </Overlay>
  );
}
