# Android 客户端安装

> 三端安装：[Web 客户端](./installation-web) · [iOS 客户端](./installation-ios) · **Android 客户端**（本页）
>
> 当前版本：**v1.3.1**（versionCode 7） · 发布日期 2026-06-17

WordForge Android 客户端目前通过 **GitHub Release** 分发 debug APK，尚未上架 Google Play。debug 构建已用自动 debug keystore 签名，可直接侧载（sideload）安装，无需任何密钥。

## 下载并侧载安装（推荐）

每次推送 `v*` tag（如 `v1.3.1`）后，CI 会自动构建可安装的 APK 并挂到对应 Release。

1. 在 Android 手机上打开 [Releases 页面](https://github.com/Heartcoolman/WordForge-Android/releases)
2. 在对应版本（如 `v1.3.1`）的 Assets 中下载：
   - `WordForge-Android-v1.3.1.apk` —— 安装包
   - `WordForge-Android-v1.3.1.apk.sha256` —— 校验和（可选，用于核验完整性）
3. 点击下载好的 APK 安装。首次安装会提示「允许从此来源安装应用」——按提示进入设置，给当前浏览器 / 文件管理器授予「安装未知应用」权限，返回后继续安装
4. 安装完成后打开 App → 填写后端地址 → 登录即可使用

> 校验完整性（可选）：下载后对比 SHA-256。
>
> ```bash
> sha256sum WordForge-Android-v1.3.1.apk
> # 输出应与 WordForge-Android-v1.3.1.apk.sha256 文件中的值一致
> ```

> **关于 debug 签名**：Release 中的 APK 为 debug 构建，证书与正式签名（Google Play / 生产 keystore）不同。debug 与正式签名的包**签名不一致，无法互相覆盖升级**；如果之前装过其他来源的同名应用，可能需先卸载再安装。

## 配置后端地址

App 内置默认后端地址为生产环境：

```
http://8.135.57.148/
```

首次打开即可直接登录使用；如需连接自建服务器，在「设置」中修改后端地址：

```
https://your-server.example.com
```

本地局域网调试可填 IP：

```
http://192.168.x.x:3000
```

> 地址会自动补全末尾 `/`，前后空格也会被去除。修改后建议重新登录一次以确保连接到正确的服务器。

## 系统要求

- Android 8.0（API 26）及以上
- 安装时需允许「安装未知应用」（侧载非应用商店来源的必要权限）
- 联网环境可访问后端地址（生产默认走明文 HTTP，自建 HTTPS 服务器同样支持）

## 登录

1. 打开 App，确认后端地址正确（默认即生产环境）
2. 使用已有账号登录，或先注册账号
3. 登录后学习记录会与服务器同步，换设备登录同一账号即可恢复进度

## 自行编译（开发者）

源码位于独立仓库 [WordForge-Android](https://github.com/Heartcoolman/WordForge-Android)（Kotlin + Jetpack Compose）。

前置条件：JDK 21（可用 Android Studio 自带 JBR）。

```bash
git clone https://github.com/Heartcoolman/WordForge-Android.git
cd WordForge-Android

# macOS 上若用 Android Studio 自带 JDK，先指定 JAVA_HOME：
export JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home"

./gradlew :app:assembleDebug   # 产物在 app/build/outputs/apk/debug/app-debug.apk
```

版本号单一源为 `app/build.gradle.kts` 的 `appVersionName`，自动驱动 `versionName` 与 User-Agent（后端 strict-mode 要求 3 段 semver）。

## 常见问题

**点击 APK 提示"为了你的安全，手机已设置为禁止安装未知来源的应用"**

- 进入「设置 → 应用 → 特殊应用访问 → 安装未知应用」，给你用来打开 APK 的浏览器 / 文件管理器开启权限，再返回继续安装。不同厂商系统的菜单路径略有差异。

**安装时提示"应用未安装"或签名冲突**

- 多为已装过签名不同的同名应用（如换了来源的包）。先卸载旧版本再安装即可；该操作会清除本地未同步缓存，但已上报到服务器的学习记录不受影响。

**登录后提示"无法连接到服务器"**

- 确认后端地址无多余空格、协议（`http://` / `https://`）填写正确
- 确认服务端正在运行：访问 `<后端地址>/health` 应返回含 `status` 字段的 JSON
- 检查手机网络是否能直连后端地址（公司 / 校园网可能限制）

**资源包 / 内容更新没生效**

- 见 [常见问题](./faq) 中「资源包 / 内容更新没生效或失败」一节；资源包机制详见 [资源包热更](/resource-packs)。
