#!/usr/bin/env bash
# 服务器侧部署：切换产品页版本。以部署用户运行，不需要 sudo。
# nginx 片段不由 CI 下发：能改它的密钥等同 root，而它几乎不变，
# 开通与变更走人工守护式下发，见 apps/site/README.md。
set -euo pipefail

VERSION="${1:?用法: deploy.sh <version>}"
ROOT_DIR="/opt/flair-bloom"
RELEASES_DIR="${ROOT_DIR}/site/releases"
NEXT_DIR="${RELEASES_DIR}/${VERSION}.new"
RELEASE_DIR="${RELEASES_DIR}/${VERSION}"
CURRENT_LINK="${ROOT_DIR}/www/flair-bloom"
PREVIOUS_LINK="${ROOT_DIR}/site/previous"

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

CURRENT_TARGET="$(readlink -f "${CURRENT_LINK}")"
PREVIOUS_TARGET="$(readlink -f "${PREVIOUS_LINK}" 2>/dev/null || true)"
for directory in "${RELEASES_DIR}"/*; do
  [[ -d "${directory}" ]] || continue
  [[ "${directory}" == "${CURRENT_TARGET}" || "${directory}" == "${PREVIOUS_TARGET}" ]] && continue
  rm -rf "${directory}"
done

echo "部署完成：flair-bloom 产品页 @ ${VERSION}"
