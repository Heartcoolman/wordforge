# 生产部署检查清单

适用:`install.sh` 安装的 systemd + `$INSTALL_DIR/.env` 拓扑。`.env` 首次安装从 `.env.example` 播种、更新时保留,以下项装机后必须人工核对。

## 网络拓扑相关(当前生产为明文 HTTP 直连,升级 HTTPS 时必改)

- [ ] 部署在 nginx 等反代之后时:`TRUST_PROXY=true`(否则资源包下载 URL 恒为 `http://` 触发 mixed-content,且限频/设备封禁/审计拿到的全是反代 IP)。
- [ ] 站点走 HTTPS 时:`COOKIE_SECURE=true`。
- [ ] 只想修下载 URL、不改 IP 信任链:改设 `RESOURCE_PACK_BASE_URL=https://<域名>`(优先级高于 TRUST_PROXY 推导)。
- [ ] nginx 参照 `deploy/nginx/wordforge.conf.sample`(已含 `X-Forwarded-Proto` 转发)。

## 资源包签名密钥(红线)

- [ ] **备份 `<DATABASE_URL 所在目录>/keys/`**(`ed25519_resource_pack.key` + `.pub`)。该目录丢失后服务会自动生成新密钥,而三端客户端已将公钥硬编码为信任锚 —— 新密钥签出的所有资源包会被全部客户端拒收,且只能靠发新客户端版本恢复。
- [ ] 核对生产公钥与客户端信任锚一致:
  ```sh
  base64 < <INSTALL_DIR>/data/keys/ed25519_resource_pack.pub
  # 应等于三端硬编码值(iOS/Android PackSignatureVerifier、web resourcePack/manager.ts):
  # fr+eALsS/N3gz4AZmpSm/wDbtDCh596WjapwVPtHn6s=
  ```

## 常规

- [ ] `JWT_SECRET` / `REFRESH_JWT_SECRET` / `ADMIN_JWT_SECRET` 均为强随机且互不相同(install.sh 已自动生成,升级迁移的旧 .env 需自查)。
- [ ] `CORS_ORIGIN` 改为生产站点来源。
