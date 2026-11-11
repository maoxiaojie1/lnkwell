# 开发与构建

本文记录构建、调试、打包的具体步骤和踩过的坑。产品能力与设计取舍见 [design.md](design.md)，任务规划见 [improvement-plan.md](improvement-plan.md)。

## 环境准备

| 工具 | 版本 |
|---|---|
| Node.js | ≥ 20 |
| Rust | ≥ 1.77.2 |
| Android SDK | platforms;android-36、ndk;26.1.10909125 |

```bash
npm install
npm run check          # svelte-check 类型检查
npm run dev            # 仅前端（无 Tauri API，部分功能报错，用于调样式）
npm run tauri dev      # 完整桌面应用

cd src-tauri
cargo test             # Rust 单测 + fixtures 回归
cargo check --target aarch64-linux-android --lib   # Android 专属代码的类型检查
```

检索层的 SQL（FTS5 外部内容表、触发器、迁移）无法在 TS 侧断言，用脚本验证：

```bash
python scripts/verify-fts.py   # 24 项断言，含老库迁移
```

## Windows 打包

```bash
npm run tauri build                    # exe + 全部安装包
npm run tauri build -- --bundles nsis  # 只做 NSIS 安装包
npx tauri bundle --bundles msi         # 复用已编译 exe，只重新打包（不重编译）
```

产物：

| 路径 | 说明 |
|---|---|
| `src-tauri/target/release/lnkwell.exe` | 便携版，双击即用 |
| `src-tauri/target/release/bundle/nsis/Lnkwell_*.exe` | NSIS 安装包 |
| `src-tauri/target/release/bundle/msi/Lnkwell_*_zh-CN.msi` | MSI 安装包 |

三个踩过的坑：

1. **Tauri 的 JS 包与 Rust crate 必须同 minor**，否则 CLI 直接拒绝构建。
   当前锁定 `@tauri-apps/api@2.11.1` ↔ `tauri` 2.11.5、`@tauri-apps/plugin-opener@2.5.5` ↔ crate 2.5.5。
   升级任一侧时记得同步另一侧。
2. **WiX 默认 en-US（代码页 1252）装不下中文**：`bundle.fileAssociations` 里的中文描述会让
   `light.exe` 报 `LGHT0311`。已在 `tauri.conf.json` 设 `bundle.windows.wix.language = "zh-CN"`。
3. **首次打包要下载 NSIS 工具链**（GitHub），而 CLI 不读 Windows 系统代理，
   需显式设置 `HTTP_PROXY` / `HTTPS_PROXY` 环境变量。

## Android 构建

`src-tauri/gen/` 不进仓库，`src-tauri/.cargo/config.toml` 也不进（含本机 NDK 绝对路径）。
新机器上按下面顺序补齐：

```powershell
# 1. 生成 Android 工程（不带 --ci 会卡在交互式提问）
npx tauri android init --ci

# 2. NDK 交叉编译配置
Copy-Item src-tauri\.cargo\config.toml.example src-tauri\.cargo\config.toml
# 然后把里面的 <NDK_BIN> 换成本机路径，例如
#   %LOCALAPPDATA%\Android\Sdk\ndk\26.1.10909125\toolchains\llvm\prebuilt\windows-x86_64\bin

# 3. 构建（脚本会自动读 ANDROID_HOME / JAVA_HOME，未设置时用 Android Studio 默认位置）
.\scripts\build-android.ps1              # release 签名版，arm64 + armv7
.\scripts\build-android.ps1 -Debug -Install   # debug 版并安装到已连接设备
```

### 本地调试环境（实测记录）

模拟器 + 真机都能跑，但要把环境补齐（这几步在干净机器上各踩过一次坑）：

1. **工程生成**：`npx tauri android init --ci`。
2. **SDK 组件**：需要 `platforms;android-36`（模板的 compileSdk 是 36）、
   `ndk;26.1.10909125`、以及一个**完整**的 system image。判断完整性看
   `system-images/.../x86_64/system.img` 是否存在——下载中断会只剩 kernel/ramdisk。
   官方源国内常见 200 KB/s，可换腾讯镜像（约 20 MB/s）：
   `https://mirrors.cloud.tencent.com/AndroidSDK/sys-img/google_apis_playstore/x86_64-35_r09.zip`
   （注意压缩包内还套了一层 `x86_64/`，解压后要把内容上移一层）。
3. **Gradle 代理**：若 `~/.gradle/gradle.properties` 里配了代理而代理没开，
   依赖会全部解析失败。`src-tauri/gen/` 已在 .gitignore 中，可直接在
   `gen/android/gradle.properties` 加 `systemProp.http.nonProxyHosts=*` 绕过。
4. **模拟器启动**：宿主 Vulkan 加载器异常时 `-gpu swiftshader_indirect` 会崩
   （`vkCreateInstance not found`），改用软件渲染：
   `emulator -avd <AVD> -no-window -gpu off -feature -Vulkan`。
5. **构建 / 安装**：

```powershell
$env:ANDROID_HOME="$env:LOCALAPPDATA\Android\Sdk"
$env:NDK_HOME="$env:ANDROID_HOME\ndk\26.1.10909125"
npx tauri android build --apk --debug --target x86_64
adb install -r src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
adb shell am start -n com.lnkwell.app/.MainActivity
```

6. **截图 / 调试**：`adb exec-out screencap -p` 在 Windows 下会被 CRLF 破坏，
   改用 `adb shell screencap -p /sdcard/s.png` + `adb pull`。
   WebView 内容不进 `uiautomator dump`，界面定位靠截图坐标（注意截图是物理像素
   1080×2400，不是预览尺寸）。

## Android 发布包（签名）

`src-tauri/gen/` 不进仓库，所以签名配置每次重建工程后都要补一遍：

1. **生成 keystore**（放仓库外，**务必留着**——Android 升级安装要求同一个签名）：

```powershell
keytool -genkeypair -keystore "$env:USERPROFILE\.lnkwell\lnkwell-release.jks" -alias lnkwell `
  -keyalg RSA -keysize 2048 -validity 10000 `
  -storepass <口令> -keypass <口令> -dname "CN=Lnkwell,O=Lnkwell,C=CN"
```

2. **写 `src-tauri/gen/android/keystore.properties`**（路径用正斜杠）：

```properties
storeFile=C:/Users/<you>/.lnkwell/lnkwell-release.jks
storePassword=<口令>
keyAlias=lnkwell
keyPassword=<口令>
```

3. **在 `gen/android/app/build.gradle.kts` 补 signingConfigs**（模板默认没有，
   不补的话 release 产出的是 unsigned APK，装不上）：

```kotlin
val keystoreProperties = Properties().apply {
    val f = rootProject.file("keystore.properties")
    if (f.exists()) f.inputStream().use { load(it) }
}
android {
    signingConfigs {
        create("release") {
            if (keystoreProperties.isNotEmpty()) {
                storeFile = file(keystoreProperties.getProperty("storeFile"))
                storePassword = keystoreProperties.getProperty("storePassword")
                keyAlias = keystoreProperties.getProperty("keyAlias")
                keyPassword = keystoreProperties.getProperty("keyPassword")
            }
        }
    }
    buildTypes {
        getByName("release") {
            if (keystoreProperties.isNotEmpty()) {
                signingConfig = signingConfigs.getByName("release")
            }
        }
    }
}
```

4. **构建**：

```powershell
npx tauri android build --apk --target aarch64            # 真机，约 16 MB
npx tauri android build --apk --target aarch64 x86_64     # 通用包（含模拟器），约 29 MB
```

产物在 `gen/android/app/build/outputs/apk/universal/release/app-universal-release.apk`。
换签名后再装到同一台设备上会报 `INSTALL_FAILED_UPDATE_INCOMPATIBLE`，需先卸载。

## Android 图标

`src-tauri/icons/android/` 是安卓图标资源，构建时会被拷进 `gen/android/app/src/main/res`。
两个坑：

1. **不要用 `npx tauri icon src-tauri/icons/icon.png` 原地输出**——输出目录就是源目录时，
   CLI 边读边写会把源图读坏（本次事故：安卓图标被写成一片蓝色圆点）。
   先生成到临时目录 `-o <tmp>`，再把 `android/mipmap-*` 拷回来。
2. **自适应图标要用深色底**：`mipmap-anydpi-v26/ic_launcher.xml` 引用
   `@color/ic_launcher_background`（本项目 `#0f1219`），前景内容缩到画布 66% 居中。
   否则启动器会给非自适应图标套一个白圆，图标显得又小又不像自己。

## Android 安全区（状态栏 / 手势条）

Tauri 模板的 `MainActivity` 调了 `enableEdgeToEdge()`，内容会画到状态栏和导航栏下面；
而 **Android WebView 不支持 `env(safe-area-inset-*)`**（那是 Safari 的），
所以必须由原生把真实 `WindowInsets` 读出来，注入 CSS 变量。

`gen/android/.../MainActivity.kt`（同样不在仓库里，重建工程要补）：

```kotlin
override fun onCreate(savedInstanceState: Bundle?) {
  enableEdgeToEdge()
  super.onCreate(savedInstanceState)
  val root = findViewById<View>(android.R.id.content)
  ViewCompat.setOnApplyWindowInsetsListener(root) { _, insets ->
    val bars = insets.getInsets(
      WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
    )
    val d = resources.displayMetrics.density
    injectSafeArea(bars.top / d, bars.bottom / d)   // → --safe-top / --safe-bottom
    insets
  }
}
override fun onWebViewCreate(webView: WebView) { webViewRef = webView }
```

`injectSafeArea` 用 `evaluateJavascript` 写 `document.documentElement.style`，
并在 0 / 200 / 600 / 1500 / 3000 ms 各重试一次（页面可能还没加载完）。
前端只保留兜底：原生没注入时 `applySafeArea()` 给保守值，且**不覆盖**原生已写入的值。

验证方法（模拟器上模拟高状态栏）：

```powershell
adb shell cmd overlay enable com.android.internal.display.cutout.emulation.tall
adb logcat -s LnkwellInsets        # 应看到 top 从 24.0 变成 48.0，标题栏随之下移
adb shell cmd overlay disable com.android.internal.display.cutout.emulation.tall
adb reboot                          # 关闭挖孔模拟需要重启显示才恢复
```
