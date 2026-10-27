export const ENT_SIGNATURE = 'flair.signature';
/** 与后端 `qzh_profile::MAX_SIGNATURE_CHARS` 一致；后端按字符截断，这里只是输入框的提示上限。 */
export const SIGNATURE_MAX_CHARS = 16;

/** 一张许可证；`expires_at` 为 null 即永久（serde 的 `Option` 序列化形态）。 */
export interface LicenseStatus {
  id: string;
  entitlements: string[];
  issued_at: number;
  expires_at: number | null;
}

/** 一项权益合并后的有效期：授予它的许可证里最晚的到期时间，有一张永久即永久。 */
export interface EntitlementGrant {
  key: string;
  expires_at: number | null;
}

/** 后端 `get_license_status` 的返回：许可证可叠加，权益取全部有效许可证的并集。 */
export interface LicenseSummary {
  entitlements: EntitlementGrant[];
  licenses: LicenseStatus[];
}

/** 后端 `activate_license` 的返回。 */
export interface ActivationResult {
  /** 这张码带来的权益，激活后据此提示怎么用。 */
  granted: string[];
  summary: LicenseSummary;
}

/** 权益点说明：是什么、在哪用；`action` 是「前往」按钮跳到的入口。 */
export interface EntitlementInfo {
  name: string;
  description: string;
  action?: { label: string; target: 'settings-profiles' };
}

const ENTITLEMENTS: Record<string, EntitlementInfo> = {
  [ENT_SIGNATURE]: {
    name: '自定义水印署名',
    description:
      '把面板背景的默认署名换成你自己写的一段字，跟着配置走。在 设置 → 配置文件 → 水印署名 里填写。',
    action: { label: '去填写', target: 'settings-profiles' },
  },
};

/** 新版 billing 发了旧版应用还不认识的权益时，提示先更新而不是静默忽略。 */
export function entitlementInfo(key: string): EntitlementInfo {
  return (
    ENTITLEMENTS[key] ?? {
      name: key,
      description: '当前版本还不认识这项权益，更新应用后即可使用。',
    }
  );
}

/**
 * 前端也按到期时间判：汇总是打开面板时取的，有期限的权益在应用运行中到期后，
 * 不等下次取状态就该退回默认署名（后端每次查询都会重新验签，两边一致）。
 */
export function grantsSignature(summary: LicenseSummary | undefined, now = Date.now()): boolean {
  return (
    summary?.entitlements.some(
      (g) => g.key === ENT_SIGNATURE && (g.expires_at === null || g.expires_at * 1000 > now),
    ) ?? false
  );
}

/** 按 Unicode 字符截断，与后端按标量计数一致；输入框的 maxLength 按 UTF-16 计，emoji 会占两格。 */
export function clampSignature(value: string): string {
  return Array.from(value).slice(0, SIGNATURE_MAX_CHARS).join('');
}

/**
 * 面板根节点 `data-signature` 的取值。没有权益或当前配置没写署名时返回 undefined，
 * 属性不渲染，CSS 回落默认署名——字段本身照样留在配置里。
 */
export function watermarkSignature(
  license: LicenseSummary | undefined,
  profiles: readonly { meta: { name: string; signature?: string } }[],
  activeName: string,
): string | undefined {
  if (!grantsSignature(license)) return undefined;
  const signature = profiles.find((p) => p.meta.name === activeName)?.meta.signature;
  return signature || undefined;
}

/** 后端 `get_membership_agreement` 的返回。 */
export interface MembershipAgreement {
  title: string;
  version: string;
}

/** 后端 `ActivationError`，按 `kind` 区分；带字段的几种是 serde 的结构体变体。 */
export type ActivationError =
  | { kind: 'invalid_code' | 'void' | 'expired' | 'license_expired' | 'reissue_limit' }
  | { kind: 'already_activated'; license: LicenseStatus | null }
  | { kind: 'agreement_changed'; current_version: string | null }
  | { kind: 'unavailable' | 'network' | 'clock_skew' | 'invalid_license' }
  | { kind: 'rate_limited'; retry_after_secs: number | null }
  | { kind: 'failed'; message: string; trace_id: string | null };

export function activationErrorText(err: ActivationError | undefined): string {
  switch (err?.kind) {
    case 'invalid_code':
      return '兑换码不对，检查一下有没有输错';
    case 'void':
      return '这张兑换码已作废';
    case 'expired':
      return '这张兑换码已过期';
    case 'license_expired':
      return '这张兑换码的许可证已到期，不能再重新激活';
    case 'reissue_limit':
      return '这张兑换码的重新激活次数已用完';
    case 'already_activated':
      return '这张兑换码已经激活过';
    case 'agreement_changed':
      return '会员服务协议有更新，请重新阅读并勾选后再激活';
    case 'unavailable':
      return '激活服务暂时不可用，请稍后再试';
    case 'network':
      return '连不上激活服务，检查一下网络';
    case 'clock_skew':
      return '许可证已保存，但电脑的系统时间不对，校正时间后即可生效';
    case 'invalid_license':
      return '许可证已保存，但校验失败，请联系作者';
    case 'rate_limited':
      return err.retry_after_secs
        ? `尝试太频繁，请 ${err.retry_after_secs} 秒后再试`
        : '尝试太频繁，请稍后再试';
    case 'failed':
      return `激活失败：${err.message}`;
    default:
      return '激活失败，请稍后再试';
  }
}
