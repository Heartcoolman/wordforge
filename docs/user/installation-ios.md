# iOS 客户端安装

> 三端安装：[Web 客户端](./installation-web) · **iOS 客户端**（本页） · [Android 客户端](./installation-android)
>
> 当前版本：**v1.2.3** · 发布日期 2026-06-17

WordForge iOS 客户端目前通过 **TestFlight** 分发，尚未上架 App Store。

## TestFlight 安装（推荐）

1. 在 iPhone 上安装 [TestFlight](https://apps.apple.com/app/testflight/id899247664)（Apple 官方测试分发平台）
2. 打开邀请链接（由管理员通过邮件或内部渠道下发）
3. 点击"接受"并安装
4. 安装完成后，打开 App → 输入服务器地址和账号即可使用

> TestFlight 构建有 90 天有效期，到期后需重新安装最新版本。TestFlight 会在到期前推送提醒。

## 配置服务器地址

首次打开 App 时，需要填写后端服务地址：

```
https://your-server.example.com
```

若是本地调试，使用局域网 IP：

```
http://192.168.x.x:3000
```

> **注意**：iOS 要求 HTTPS 才能访问外网地址（ATS 策略）。本地局域网调试可以暂时豁免，但生产环境必须配置有效的 TLS 证书。

## 自行编译（开发者）

源码位于独立仓库 [wordforge-web](https://github.com/Heartcoolman/wordforge-web)（React Native 或 Web App，具体参见该仓库说明）。

自编译前置条件：
- Xcode 16+（macOS 15+）
- Apple Developer Program 账号（用于真机运行）

```bash
git clone https://github.com/Heartcoolman/wordforge-web.git
cd wordforge-web
# 参考该仓库 README 中的构建步骤
```

## 常见问题

**TestFlight 显示"此 App 暂不接受测试人员"**
- 邀请名额已满或链接过期，联系管理员重新邀请。

**登录后提示"无法连接到服务器"**
- 确认服务器地址无多余空格，末尾无 `/`
- 确认服务端正在运行（`GET /health` 应返回 200）
- 检查防火墙是否放行对应端口
