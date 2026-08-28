# Windows 测试安装包与升级

## 当前交付方式

当前 Windows 测试版使用 Tauri 2 的 NSIS 安装器：

- 安装范围为当前用户，不要求管理员权限；
- 应用标识固定为 `com.gy.typing`，产品名固定为 `GY Typing`；
- 相同标识的新版本通过安装器覆盖安装；
- 配置、历史和事故恢复数据位于用户配置/本地数据目录，不放在安装目录内，覆盖安装不会主动删除这些数据；
- 打包脚本生成 `release-manifest.json`，保存版本、Git revision、脏工作树标记、文件大小和 SHA-256。

本阶段的安装包未配置 Windows Authenticode 代码签名，其他测试机从浏览器下载时可能看到 SmartScreen 警告。对外公开分发前必须补充可信代码签名并在目标 Windows 环境验证。

## 构建

准备 Windows 10/11、Node.js、Rust MSVC 工具链、Visual Studio C++ 桌面构建工具和 WebView2。安装依赖后运行：

```powershell
npm ci
npm run package:windows
```

只运行发布前门禁、不生成安装包：

```powershell
npm run package:windows:check
```

完整打包会依次执行架构结构检查、ASR 边界检查、凭据扫描、前端测试、Rust 测试和 NSIS 构建。输出位于：

```text
src-tauri/target/release/bundle/nsis/
```

分发时同时提供 `*-setup.exe` 和 `release-manifest.json`。接收方应比对安装包 SHA-256；脏工作树产物只用于可追踪的内部测试。

## 版本规则

每次可安装构建都必须使用递增的 SemVer，并保持以下三个文件一致：

- `package.json`
- `src-tauri/Cargo.toml`
- `src-tauri/tauri.conf.json`

打包脚本会在不一致时终止。修改 Rust package 版本后还需运行一次 Cargo 命令更新 `src-tauri/Cargo.lock` 中的根包版本。

不要改变 `identifier` 或 NSIS 安装范围来发布普通升级；这会让 Windows 把它视为另一套安装身份，破坏稳定的覆盖安装路径。

## 测试矩阵

至少在一台非开发用 Windows 10/11 x64 机器完成：

1. 全新安装：普通用户安装、启动、托盘和 WebView2 正常。
2. 数据初始化：写入非敏感测试配置、历史和快捷键设置。
3. 覆盖安装：安装更高版本，确认应用版本升级且测试数据仍存在。
4. 运行中升级：应用正在运行时启动新安装器，确认退出/替换行为明确且没有残留旧进程。
5. 卸载与重装：确认卸载范围和用户数据保留行为符合测试说明。
6. 常见外部应用：至少验证记事本、浏览器和一个 Electron 应用中的语音输入主流程。

发布级验证需记录旧/新版本、Windows 版本、CPU 架构、安装包 SHA-256、是否签名、执行结果和已知限制。

## 后续自动更新

Tauri Updater 的 Windows 路径是下载经过 Tauri 更新签名校验的完整 NSIS/MSI 更新产物并覆盖安装，不是二进制差分补丁。接入前必须先确认：

- 更新发布源：GitHub Releases、对象存储/CDN 或动态更新服务；
- 稳定版与测试版是否需要独立 channel；
- 离线保管的 Tauri updater 私钥及其灾难恢复方案；
- Windows Authenticode 证书或受控签名服务；
- 应用内的检查、下载进度、安装确认和失败恢复体验。

实现顺序建议为：先建立受签名的发布流水线和静态 `latest.json`，再接入 updater plugin，最后用两个真实版本做端到端覆盖升级。Tauri 更新私钥不得写入仓库或 `.env`；构建环境通过 secret 注入。Windows 安装更新时应用会退出，因此接入前还需复核语音会话、待交付文本和本地数据库的安全关闭边界。
