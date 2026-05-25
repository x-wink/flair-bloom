import { invoke } from '@tauri-apps/api/core';
import { useCallback, useEffect, useRef, useState } from 'react';
import eulaText from '../../../assets/EULA.md?raw';
import './AgreementDialog.css';

interface Props {
  onAgreed: () => void;
}

export default function AgreementPage({ onAgreed }: Props) {
  const [scrolledToBottom, setScrolledToBottom] = useState(false);
  const [agreeing, setAgreeing] = useState(false);
  const [agreeError, setAgreeError] = useState('');
  const contentRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = contentRef.current;
    if (!el) return;
    const checkBottom = () => {
      // 内容不溢出时 scroll 事件永远不会触发，需要主动检查
      if (el.scrollHeight <= el.clientHeight + 16) {
        setScrolledToBottom(true);
        return;
      }
      const bottom = el.scrollHeight - el.scrollTop - el.clientHeight < 16;
      if (bottom) setScrolledToBottom(true);
    };
    checkBottom();
    el.addEventListener('scroll', checkBottom, { passive: true });
    return () => el.removeEventListener('scroll', checkBottom);
  }, []);

  const handleAgree = useCallback(async () => {
    setAgreeing(true);
    try {
      await invoke('agree_license');
      onAgreed();
    } catch {
      setAgreeing(false);
      setAgreeError('操作失败，请重试');
    }
  }, [onAgreed]);

  const handleReject = useCallback(async () => {
    await invoke('exit_app');
  }, []);

  return (
    <div className="agreement-card">
      <h2 className="agreement-title">用户协议</h2>
      <div className="agreement-body" ref={contentRef}>
        <pre className="agreement-text">{eulaText}</pre>
      </div>
      <div className="agreement-hint">
        {agreeError || (scrolledToBottom ? '已阅读完毕' : '请滚动阅读协议全文')}
      </div>
      <div className="agreement-actions">
        <button className="agree-btn reject" onClick={handleReject}>
          不同意并退出
        </button>
        <button
          className="agree-btn accept"
          disabled={!scrolledToBottom || agreeing}
          onClick={handleAgree}
        >
          {agreeing ? '处理中…' : '同意并继续'}
        </button>
      </div>
    </div>
  );
}
