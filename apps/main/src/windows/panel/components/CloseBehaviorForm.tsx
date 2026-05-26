import { useState } from 'react';
import './CloseBehaviorForm.css';

export type CloseBehavior = 'minimize' | 'exit';

interface Props {
  defaultChoice: CloseBehavior;
  onChange: (choice: CloseBehavior, remember: boolean) => void;
}

export default function CloseBehaviorForm({ defaultChoice, onChange }: Props) {
  const [choice, setChoice] = useState<CloseBehavior>(defaultChoice);
  const [remember, setRemember] = useState(false);

  function update(c: CloseBehavior, r: boolean) {
    setChoice(c);
    setRemember(r);
    onChange(c, r);
  }

  return (
    <>
      <label className="radio-row">
        <input
          type="radio"
          name="close-choice"
          checked={choice === 'minimize'}
          onChange={() => update('minimize', remember)}
        />
        <span>
          <strong>最小化到托盘</strong>
          <small>程序继续在后台运行（推荐）</small>
        </span>
      </label>
      <label className="radio-row">
        <input
          type="radio"
          name="close-choice"
          checked={choice === 'exit'}
          onChange={() => update('exit', remember)}
        />
        <span>
          <strong>直接退出</strong>
          <small>关闭程序与所有连发功能</small>
        </span>
      </label>
      <label className="check-row">
        <input
          type="checkbox"
          checked={remember}
          onChange={(e) => update(choice, e.target.checked)}
        />
        <span>记住我的选择</span>
      </label>
    </>
  );
}
