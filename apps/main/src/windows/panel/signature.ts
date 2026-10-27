export const ENT_SIGNATURE = 'flair.signature';
/** 与后端 `qzh_profile::MAX_SIGNATURE_CHARS` 一致；后端按字符截断，这里只是输入框的提示上限。 */
export const SIGNATURE_MAX_CHARS = 16;

/** 后端 `get_license_status` 的返回；`expires_at` 为 null 即永久（serde 的 `Option` 序列化形态）。 */
export interface LicenseStatus {
  id: string;
  entitlements: string[];
  issued_at: number;
  expires_at: number | null;
}

export function grantsSignature(license: LicenseStatus | undefined): boolean {
  return license?.entitlements.includes(ENT_SIGNATURE) ?? false;
}

/**
 * 面板根节点 `data-signature` 的取值。没有权益或当前配置没写署名时返回 undefined，
 * 属性不渲染，CSS 回落默认署名——字段本身照样留在配置里。
 */
export function watermarkSignature(
  license: LicenseStatus | undefined,
  profiles: readonly { meta: { name: string; signature?: string } }[],
  activeName: string,
): string | undefined {
  if (!grantsSignature(license)) return undefined;
  const signature = profiles.find((p) => p.meta.name === activeName)?.meta.signature;
  return signature || undefined;
}
