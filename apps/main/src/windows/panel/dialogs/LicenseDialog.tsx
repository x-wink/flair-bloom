import { invoke } from '@tauri-apps/api/core';
import { useState } from 'react';
import Button from '../components/Button';
import {
  activationErrorText,
  type ActivationError,
  type ActivationResult,
  entitlementInfo,
  type EntitlementInfo,
  type LicenseStatus,
  type LicenseSummary,
  type MembershipAgreement,
} from '../license';
import DialogShell from './DialogShell';
import './LicenseDialog.css';

type EntitlementTarget = NonNullable<EntitlementInfo['action']>['target'];

interface Props {
  summary: LicenseSummary | undefined;
  /** 上次激活时同意过的会员服务协议版本；现行版本没变就默认勾选、不再拦一次。 */
  agreedVersion: string | undefined;
  onActivated: (summary: LicenseSummary, agreedVersion: string | undefined) => void;
  onOpenTarget: (target: EntitlementTarget) => void;
  onClose: () => void;
}

type View = { kind: 'form' } | { kind: 'success'; granted: string[] } | { kind: 'status' };

function formatDate(secs: number): string {
  return new Date(secs * 1000).toLocaleDateString('zh-CN');
}

function expiryText(expiresAt: number | null): string {
  if (expiresAt === null) return '永久';
  const days = Math.ceil((expiresAt * 1000 - Date.now()) / 86_400_000);
  return `${formatDate(expiresAt)} 到期（剩 ${Math.max(days, 0)} 天）`;
}

function EntitlementCard({
  entKey,
  expiresAt,
  onOpenTarget,
}: {
  entKey: string;
  expiresAt?: number | null;
  onOpenTarget: Props['onOpenTarget'];
}) {
  const info = entitlementInfo(entKey);
  return (
    <li className="license-ent">
      <div className="license-ent-head">
        <span className="license-ent-name">{info.name}</span>
        {expiresAt !== undefined && (
          <span className="license-ent-expiry">{expiryText(expiresAt)}</span>
        )}
      </div>
      <p className="license-ent-desc">{info.description}</p>
      {info.action && (
        <Button size="sm" variant="outline" onClick={() => onOpenTarget(info.action!.target)}>
          {info.action.label}
        </Button>
      )}
    </li>
  );
}

export default function LicenseDialog({
  summary,
  agreedVersion,
  onActivated,
  onOpenTarget,
  onClose,
}: Props) {
  const hasLicense = (summary?.licenses.length ?? 0) > 0;
  const [view, setView] = useState<View>(hasLicense ? { kind: 'status' } : { kind: 'form' });
  const [code, setCode] = useState('');
  // undefined = 还没问过 uc；null = 线上没有已发布的协议，激活不带版本（D14）
  const [agreement, setAgreement] = useState<MembershipAgreement | null | undefined>(undefined);
  const [agreed, setAgreed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | undefined>(undefined);
  // 本机已激活过这张码：等用户确认覆盖（覆盖会占用一次重新激活次数）
  const [duplicate, setDuplicate] = useState<{ license: LicenseStatus | null } | undefined>(
    undefined,
  );

  async function activate(overwrite = false) {
    if (busy || !code.trim()) return;
    setBusy(true);
    setError(undefined);
    setDuplicate(undefined);
    try {
      // 协议版本到点「激活」时才问：不点激活的人一个请求都不发
      let current = agreement;
      let accepted = agreed;
      if (current === undefined) {
        current = await invoke<MembershipAgreement | null>('get_membership_agreement');
        setAgreement(current);
        if (current && current.version === agreedVersion) {
          accepted = true;
          setAgreed(true);
        }
      }
      if (current && !accepted) {
        setError('请先阅读并勾选会员服务协议');
        return;
      }
      const result = await invoke<ActivationResult>('activate_license', {
        code,
        agreementVersion: current?.version,
        overwrite,
      });
      onActivated(result.summary, current?.version);
      setCode('');
      setView({ kind: 'success', granted: result.granted });
    } catch (e) {
      const err = e as ActivationError;
      if (err?.kind === 'already_activated') {
        setDuplicate({ license: err.license });
        return;
      }
      if (err?.kind === 'agreement_changed') {
        setAgreement((prev) => ({
          title: prev?.title ?? '会员服务协议',
          version: err.current_version ?? prev?.version ?? '',
        }));
        setAgreed(false);
      }
      setError(activationErrorText(err));
    } finally {
      setBusy(false);
    }
  }

  let footer;
  if (view.kind === 'form') {
    footer = duplicate ? (
      <>
        <Button variant="outline" tone="neutral" onClick={() => setDuplicate(undefined)}>
          取消
        </Button>
        <Button onClick={() => void activate(true)} disabled={busy}>
          {busy ? '激活中…' : '覆盖'}
        </Button>
      </>
    ) : (
      <>
        {hasLicense && (
          <Button variant="outline" tone="neutral" onClick={() => setView({ kind: 'status' })}>
            返回
          </Button>
        )}
        <Button onClick={() => void activate()} disabled={busy || !code.trim()}>
          {busy ? '激活中…' : '激活'}
        </Button>
      </>
    );
  } else {
    footer = (
      <Button variant="outline" tone="neutral" onClick={onClose}>
        关闭
      </Button>
    );
  }

  const subtitle =
    view.kind === 'form'
      ? '激活需要联网一次，之后离线可用'
      : view.kind === 'success'
        ? '激活成功'
        : `已激活 ${summary?.licenses.length ?? 0} 张兑换码`;

  return (
    <DialogShell title="兑换码" subtitle={subtitle} footer={footer}>
      {view.kind === 'form' && (
        <div className="license-form">
          <input
            type="text"
            className="license-code-input"
            value={code}
            placeholder="输入兑换码"
            aria-label="兑换码"
            autoFocus
            spellCheck={false}
            disabled={duplicate !== undefined}
            onChange={(e) => setCode(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && !duplicate) void activate();
            }}
          />
          {agreement && (
            <label className="license-agreement">
              <input
                type="checkbox"
                checked={agreed}
                onChange={(e) => setAgreed(e.target.checked)}
              />
              <span>
                我已阅读并同意
                <button
                  type="button"
                  className="license-link"
                  onClick={() => {
                    invoke('open_link', { target: 'membership' }).catch(() => {});
                  }}
                >
                  《xwink {agreement.title}》
                </button>
                （版本 {agreement.version}）
              </span>
            </label>
          )}
          {duplicate && (
            <div className="license-confirm">
              <p>
                这张兑换码已经激活过
                {duplicate.license &&
                  `（${duplicate.license.entitlements.map((e) => entitlementInfo(e).name).join('、')}）`}
                。覆盖会重新激活一次，占用这张码的一次重新激活次数，确定覆盖吗？
              </p>
            </div>
          )}
          {error && <p className="license-error">{error}</p>}
          <p className="license-note">
            只在你点「激活」时联网：先读取协议版本，再提交兑换码换回许可证。之后校验都在本机完成。
            多张兑换码的权益会叠加。
          </p>
        </div>
      )}

      {view.kind === 'success' && (
        <>
          <p className="license-lead">已获得以下权益：</p>
          <ul className="license-ents">
            {view.granted.map((key) => (
              <EntitlementCard key={key} entKey={key} onOpenTarget={onOpenTarget} />
            ))}
          </ul>
          <button
            type="button"
            className="license-link"
            onClick={() => setView({ kind: 'status' })}
          >
            查看全部权益
          </button>
        </>
      )}

      {view.kind === 'status' && summary && (
        <>
          <ul className="license-ents">
            {summary.entitlements.map((g) => (
              <EntitlementCard
                key={g.key}
                entKey={g.key}
                expiresAt={g.expires_at}
                onOpenTarget={onOpenTarget}
              />
            ))}
          </ul>
          <ul className="license-status">
            {summary.licenses.map((l) => (
              <li key={l.id}>
                <span className="license-id">{l.id}</span>
                <span className="license-key">
                  {formatDate(l.issued_at)} 激活 · {expiryText(l.expires_at)}
                </span>
              </li>
            ))}
          </ul>
          <button type="button" className="license-link" onClick={() => setView({ kind: 'form' })}>
            再兑换一张
          </button>
        </>
      )}
    </DialogShell>
  );
}
