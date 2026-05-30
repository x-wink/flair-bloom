import {
  type ButtonHTMLAttributes,
  type HTMLAttributes,
  type ReactNode,
  type KeyboardEvent,
  type MouseEvent,
} from 'react';
import './CardList.css';

type CardListColumns = 'one' | 'two' | 'three';

interface CardListProps extends HTMLAttributes<HTMLDivElement> {
  columns?: CardListColumns;
  children: ReactNode;
}

interface CardListButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  active?: boolean;
  children: ReactNode;
}

interface CardListItemProps extends HTMLAttributes<HTMLDivElement> {
  active?: boolean;
  disabled?: boolean;
  interactive?: boolean;
  children: ReactNode;
}

function cx(parts: Array<string | false | null | undefined>): string {
  return parts.filter(Boolean).join(' ');
}

export function CardList({ columns = 'one', className, children, ...rest }: CardListProps) {
  return (
    <div className={cx(['fb-card-list', `fb-card-list--${columns}`, className])} {...rest}>
      {children}
    </div>
  );
}

export function CardListButton({
  active = false,
  className,
  children,
  type = 'button',
  ...rest
}: CardListButtonProps) {
  return (
    <button
      type={type}
      className={cx(['fb-card-list-item', active && 'fb-card-list-item--active', className])}
      {...rest}
    >
      {children}
    </button>
  );
}

export function CardListItem({
  active = false,
  disabled = false,
  interactive = true,
  className,
  children,
  onClick,
  onKeyDown,
  ...rest
}: CardListItemProps) {
  return (
    <div
      className={cx([
        'fb-card-list-item',
        active && 'fb-card-list-item--active',
        !interactive && 'fb-card-list-item--static',
        className,
      ])}
      aria-disabled={disabled ? 'true' : undefined}
      onClick={(e: MouseEvent<HTMLDivElement>) => {
        if (disabled) {
          e.preventDefault();
          return;
        }
        onClick?.(e);
      }}
      onKeyDown={(e: KeyboardEvent<HTMLDivElement>) => {
        if (disabled) {
          e.preventDefault();
          return;
        }
        onKeyDown?.(e);
      }}
      {...rest}
    >
      {children}
    </div>
  );
}
