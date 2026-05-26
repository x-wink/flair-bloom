import { useState } from 'react';
import './ProfileNameForm.css';

interface Props {
  defaultValue?: string;
  placeholder?: string;
  onChange: (name: string) => void;
}

export default function ProfileNameForm({ defaultValue = '', placeholder, onChange }: Props) {
  const [v, setV] = useState(defaultValue);
  return (
    <input
      autoFocus
      type="text"
      className="profile-name-input"
      maxLength={32}
      placeholder={placeholder}
      value={v}
      onChange={(e) => {
        setV(e.target.value);
        onChange(e.target.value);
      }}
    />
  );
}
