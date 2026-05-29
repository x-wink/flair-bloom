import './Kbd.css';

interface Props {
  children: string;
  label?: string;
}

export default function Kbd({ children, label }: Props) {
  return (
    <kbd className="fb-kbd">
      {children}
      {label && <span className="fb-kbd-label">{label}</span>}
    </kbd>
  );
}
