# 产品页 app.xwink.fun/flair-bloom

气质花的对外门面：下载安装包、看更新公告、找支持入口。纯静态 SSG，挂在 app.xwink.fun 的路径下；域名、证书与 `app.xwink.fun` 的 server 块归 xwink-console 仓的 app-site 单元，本目录只负责自己这一段。

它同时是 `@xwink/*` 私有制品的仓外消费者：按精确版本从私有 registry 安装，不引用 xwink-console 源码。

## 组成

| 目录      | 作用                                                                                                 |
| --------- | ---------------------------------------------------------------------------------------------------- |
| `app/`    | Nuxt 页面。公告 Markdown 解析与门派色板直接引用 `apps/main` 的同一份源码，网站与应用不会漂           |
| `mirror/` | 发布镜像同步脚本，服务器上以 Node 24 原生运行 TypeScript，无依赖                                     |
| `deploy/` | 部署脚本、nginx 片段、systemd 单元、服务器主机公钥基线                                               |

为什么不并入仓库根 workspace：根 workspace 的 Tauri 发版流水线会跑 `pnpm install`，并进来就得给那条流水线也配私有制品库凭据。

## 发布镜像

国内直连 GitHub 下载很慢，服务器每 10 分钟同步一次：

- 跟随 GitHub 的 **Latest 指针**，不按发布时间取最新。`docs/RELEASE_ROLLBACK.md` 的止血动作是把 Latest 拨回好版本，镜像最迟一轮后跟上。
- 镜像 Latest 所指版本与最近 3 个稳定版本的 exe、msi，公告保留最近 30 个版本。
- 每个安装包按 GitHub 给的 sha256 校验后才落盘；全部就绪才原子替换 `releases.json` 与 `latest.json`，任一失败整轮放弃、保留上一轮数据。
- `latest.json` 里的下载地址改写为镜像地址。签名只覆盖安装包内容，改写不影响更新器校验。
- 服务器实测从 GitHub 下载约 20 KB/s，新版本首轮就绪要十分钟级；这期间页面与更新器仍指向上一版。

## 本地开发

```sh
cd apps/site
# 私有制品库凭据写用户级 ~/.npmrc（pnpm 11 不展开项目级 .npmrc 里的环境变量）
pnpm config set --location=user //npm.cnb.cool/x-wink/playground/npm/-/packages/:_authToken <令牌>
pnpm install
pnpm mirror:dev   # 把真实发布数据同步到 .mirror，开发服务按线上路径提供
pnpm dev          # http://localhost:3900/flair-bloom/
pnpm test         # 镜像脚本单测
pnpm typecheck && pnpm build
```

## 发布

推 `site-v<semver>` tag 触发 `.github/workflows/site.yml`：单测、类型检查、构建、上传、原子切换版本、验收。产品页发版不经 Tauri updater，不触达应用用户。回滚在服务器上把 `/opt/flair-bloom/www/flair-bloom` 指回 `/opt/flair-bloom/site/previous` 所指目录。

## 服务器开通（一次性，需要 root）

前提：xwink-console 的 app-site 单元已上线，`/etc/nginx/vhost.d/app.xwink.fun/` 存在。

1. 建部署用户与目录。CI 用这个用户上传与切换版本，它没有 sudo：

   ```sh
   useradd --system --create-home --shell /bin/bash flair-bloom
   install -d -o flair-bloom -g flair-bloom /opt/flair-bloom /opt/flair-bloom/mirror
   # 把 CI 部署公钥写进 /home/flair-bloom/.ssh/authorized_keys
   ```

2. 守护式下发 nginx 片段与 systemd 单元。它们几乎不变，不交给 CI：能改它们的密钥就等同 root。

   ```sh
   winkops edit /etc/nginx/vhost.d/app.xwink.fun/flair-bloom.conf --file apps/site/deploy/flair-bloom.conf \
     --validate 'nginx -t' --reload 'systemctl reload nginx' -c <连接配置>
   # 两个单元文件复制到 /etc/systemd/system/ 后：
   systemctl daemon-reload && systemctl enable --now flair-bloom-mirror.timer
   ```

3. GitHub 仓库配置 `production` 环境与 secrets：`SITE_DEPLOY_HOST`、`SITE_DEPLOY_PORT`、`SITE_DEPLOY_USER`（`flair-bloom`）、`SITE_DEPLOY_SSH_KEY`、`XWINK_NPM_TOKEN`（私有制品库只读令牌）。

4. 推第一个 `site-v*` tag。首次部署后镜像定时器会在两分钟内开始首轮同步。
