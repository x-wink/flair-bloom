#!/usr/bin/env bash
# 服务器侧部署：切换产品页版本、更新镜像同步脚本。以部署用户运行，不需要 sudo。
# nginx 片段与 systemd 单元不由 CI 下发：能改它们的密钥等同 root，而它们几乎不变，
# 开通与变更走人工守护式下发，见 apps/site/README.md。
set -euo pipefail

VERSION="${1:?用法: deploy.sh <version>}"
ROOT_DIR="/opt/flair-bloom"
UPLOAD_DIR="${ROOT_DIR}/upload"
RELEASES_DIR="${ROOT_DIR}/site/releases"
NEXT_DIR="${RELEASES_DIR}/${VERSION}.new"
RELEASE_DIR="${RELEASES_DIR}/${VERSION}"
CURRENT_LINK="${ROOT_DIR}/www/flair-bloom"
PREVIOUS_LINK="${ROOT_DIR}/site/previous"
MIRROR_APP_DIR="${ROOT_DIR}/mirror-app"

if [[ ! -f "${NEXT_DIR}/index.html" ]]; then
  echo "错误：${NEXT_DIR} 缺少 index.html" >&2
  exit 1
fi

mkdir -p "${RELEASES_DIR}" "$(dirname "${CURRENT_LINK}")"
OLD_CURRENT="$(readlink -f "${CURRENT_LINK}" 2>/dev/null || true)"

if [[ "${OLD_CURRENT}" == "${RELEASE_DIR}" ]]; then
  RELEASE_DIR="${RELEASES_DIR}/${VERSION}-rerun-$(date +%Y%m%d%H%M%S)"
fi
rm -rf "${RELEASE_DIR}"
mv "${NEXT_DIR}" "${RELEASE_DIR}"
ln -sfn "${RELEASE_DIR}" "${CURRENT_LINK}.new"
mv -Tf "${CURRENT_LINK}.new" "${CURRENT_LINK}"
if [[ -n "${OLD_CURRENT}" && "${OLD_CURRENT}" != "${RELEASE_DIR}" ]]; then
  ln -sfn "${OLD_CURRENT}" "${PREVIOUS_LINK}"
fi

# 镜像脚本整目录换名：定时器恰好触发时读到的要么全是旧文件、要么全是新文件
rm -rf "${MIRROR_APP_DIR}.new" "${MIRROR_APP_DIR}.old"
cp -R "${UPLOAD_DIR}/mirror" "${MIRROR_APP_DIR}.new"
rm -f "${MIRROR_APP_DIR}.new"/*.test.ts
[[ -d "${MIRROR_APP_DIR}" ]] && mv "${MIRROR_APP_DIR}" "${MIRROR_APP_DIR}.old"
mv "${MIRROR_APP_DIR}.new" "${MIRROR_APP_DIR}"
rm -rf "${MIRROR_APP_DIR}.old"

CURRENT_TARGET="$(readlink -f "${CURRENT_LINK}")"
PREVIOUS_TARGET="$(readlink -f "${PREVIOUS_LINK}" 2>/dev/null || true)"
for directory in "${RELEASES_DIR}"/*; do
  [[ -d "${directory}" ]] || continue
  [[ "${directory}" == "${CURRENT_TARGET}" || "${directory}" == "${PREVIOUS_TARGET}" ]] && continue
  rm -rf "${directory}"
done

echo "部署完成：flair-bloom 产品页 @ ${VERSION}"
