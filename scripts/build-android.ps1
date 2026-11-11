#requires -Version 5.1

<#
.SYNOPSIS
    构建 Android APK，可选自动安装到连接的设备/模拟器。
.DESCRIPTION
    封装 tauri android build：设置构建环境变量，构建后把 APK 复制到项目根目录，
    可选（-Install）自动检测连接设备并用 adb 安装对应架构的 APK。
    默认：release 正式签名版，arm64-v8a + armeabi-v7a，split-per-abi。
    正式签名依赖 src-tauri/gen/android/keystore.properties（已 gitignore）。
.EXAMPLE
    .\scripts\build-android.ps1
    构建 release 正式签名版（arm64 + armv7），复制到项目根目录。
.EXAMPLE
    .\scripts\build-android.ps1 -Debug -Install
    构建 debug 版并自动安装到连接的设备/模拟器。
.EXAMPLE
    .\scripts\build-android.ps1 -Install
    构建 release 版，检测设备架构后安装对应的 APK。
.EXAMPLE
    .\scripts\build-android.ps1 -Targets aarch64 -NoCopy
    只构建 arm64，不复制 APK。
#>

param(
    [switch]$Debug,
    [ValidateSet('aarch64', 'armv7', 'i686', 'x86_64')]
    [string[]]$Targets = @('aarch64', 'armv7'),
    [switch]$NoCopy,
    [switch]$Install
)

# 注意：不要设置 $ErrorActionPreference = 'Stop'。
# Gradle/npx 会往 stderr 写大量日志（Info/exception 等），在 PowerShell 5.1 下
# Stop 会把 stderr 当作 NativeCommandError 抛异常中断构建。改为依赖 $LASTEXITCODE 判断。

# ---- 构建环境：优先读环境变量，未设置时退回 Android Studio 默认位置 ----
$AndroidSdk = if ($env:ANDROID_HOME) { $env:ANDROID_HOME }
              elseif ($env:ANDROID_SDK_ROOT) { $env:ANDROID_SDK_ROOT }
              else { Join-Path $env:LOCALAPPDATA 'Android\Sdk' }

$JavaHome = if ($env:JAVA_HOME) { $env:JAVA_HOME }
            else { 'C:\Program Files\Android\Android Studio\jbr' }

$NdkVersion = if ($env:NDK_VERSION) { $env:NDK_VERSION } else { '26.1.10909125' }

if (-not (Test-Path $AndroidSdk)) {
    Write-Error "未找到 Android SDK：$AndroidSdk（请设置 ANDROID_HOME）"
    exit 1
}

$env:ANDROID_HOME     = $AndroidSdk
$env:ANDROID_SDK_ROOT = $AndroidSdk
$env:JAVA_HOME        = $JavaHome
$env:NDK_HOME         = Join-Path $AndroidSdk "ndk\$NdkVersion"

$Adb = Join-Path $AndroidSdk 'platform-tools\adb.exe'

# ---- 定位项目根目录（脚本位于 scripts/ 下）----
$ProjectRoot = Split-Path -Parent $PSScriptRoot
Set-Location $ProjectRoot

# ---- target -> (abiDir, abiName) 映射 ----
$TargetInfo = @{
    'aarch64' = @{ Dir = 'arm64';   Name = 'arm64-v8a' }
    'armv7'   = @{ Dir = 'arm';     Name = 'armeabi-v7a' }
    'i686'    = @{ Dir = 'x86';     Name = 'x86' }
    'x86_64'  = @{ Dir = 'x86_64';  Name = 'x86_64' }
}

# ---- 组装构建命令 ----
$tauriArgs = @('tauri', 'android', 'build', '--apk')
if ($Debug) { $tauriArgs += '--debug' }
foreach ($t in $Targets) { $tauriArgs += '--target'; $tauriArgs += $t }
# debug 用 universal 单包（快速测试）；release 才 split-per-abi（按架构分包）
if (-not $Debug) { $tauriArgs += '--split-per-abi' }

$modeLabel = if ($Debug) { 'debug' } else { 'release (正式签名)' }
Write-Host "==> 模式: $modeLabel" -ForegroundColor Cyan
Write-Host "==> 架构: $($Targets -join ', ')" -ForegroundColor Cyan
Write-Host "==> 命令: npx $($tauriArgs -join ' ')" -ForegroundColor DarkGray

& npx @tauriArgs 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Error "构建失败 (exit=$LASTEXITCODE)"
    exit $LASTEXITCODE
}

# ---- 计算每个 target 对应的 APK 源路径 ----
$ApkBase   = Join-Path $ProjectRoot 'src-tauri\gen\android\app\build\outputs\apk'
$buildType = if ($Debug) { 'debug' } else { 'release' }

# debug 用 universal APK；release 用 split-per-abi APK
$ApkPathByAbiName = @{}
if ($Debug) {
    $src = Join-Path $ApkBase "universal\debug\app-universal-debug.apk"
    if (Test-Path $src) {
        $ApkPathByAbiName['universal'] = $src
    }
} else {
    foreach ($t in $Targets) {
        $info = $TargetInfo[$t]
        $src = Join-Path $ApkBase "$($info.Dir)\release\app-$($info.Dir)-release.apk"
        if (Test-Path $src) {
            $ApkPathByAbiName[$info.Name] = $src
        } else {
            Write-Warning "未找到 APK: $src"
        }
    }
}

# ---- 复制 APK 到项目根目录 ----
if (-not $NoCopy) {
    if ($Debug) {
        if ($ApkPathByAbiName.ContainsKey('universal')) {
            $dst = Join-Path $ProjectRoot 'Lnkwell-debug.apk'
            Copy-Item $ApkPathByAbiName['universal'] $dst -Force
            Write-Host "==> 已复制: $dst" -ForegroundColor Green
        }
    } else {
        $version = '0.1.0'
        $conf = Join-Path $ProjectRoot 'src-tauri\tauri.conf.json'
        if (Test-Path $conf) {
            $j = Get-Content $conf -Raw -Encoding UTF8 | ConvertFrom-Json
            if ($j.version) { $version = $j.version }
        }
        foreach ($t in $Targets) {
            $info = $TargetInfo[$t]
            if ($ApkPathByAbiName.ContainsKey($info.Name)) {
                $dst = Join-Path $ProjectRoot ("Lnkwell-{0}-{1}.apk" -f $version, $info.Name)
                Copy-Item $ApkPathByAbiName[$info.Name] $dst -Force
                Write-Host "==> 已复制: $dst" -ForegroundColor Green
            }
        }
    }
}

# ---- 安装到设备/模拟器 ----
if (-not $Install) {
    Write-Host "==> 完成（跳过安装，加 -Install 可自动安装）" -ForegroundColor Cyan
    exit 0
}

if (-not (Test-Path $Adb)) {
    Write-Error "未找到 adb: $Adb"
    exit 1
}

# 获取在线设备列表（serial）
$devices = @()
foreach ($line in (& $Adb devices 2>&1)) {
    if ($line -match '^(\S+)\s+device\s*$') {
        $devices += $Matches[1]
    }
}
if ($devices.Count -eq 0) {
    Write-Warning "未检测到已连接的设备/模拟器，跳过安装。请先连接手机（USB 调试）或启动模拟器。"
    exit 0
}
if ($devices.Count -gt 1) {
    Write-Host "检测到多个设备: $($devices -join ', ')，将安装到第一个。" -ForegroundColor Yellow
}
$serial = $devices[0]

# 读取设备 ABI，决定装哪个 APK
$abi = (& $Adb -s $serial shell getprop ro.product.cpu.abi 2>&1).Trim()
Write-Host "==> 设备 $serial 架构: $abi" -ForegroundColor Cyan

if ($Debug) {
    $apk = $ApkPathByAbiName['universal']
} else {
    # release split-per-abi：按设备 ABI 选择
    if ($ApkPathByAbiName.ContainsKey($abi)) {
        $apk = $ApkPathByAbiName[$abi]
    } elseif ($abi -eq 'arm64-v8a' -and $ApkPathByAbiName.ContainsKey('armeabi-v7a')) {
        # 极少数情况：设备声明 arm64 但没编 arm64，可回退装 32 位（一般不会走到）
        $apk = $ApkPathByAbiName['armeabi-v7a']
        Write-Host "==> 未构建 $abi，回退安装 armeabi-v7a" -ForegroundColor Yellow
    } else {
        Write-Error "设备架构 '$abi' 与已构建的架构（$($ApkPathByAbiName.Keys -join ', ')）不匹配。请用 -Targets 补充对应架构。"
        exit 1
    }
}

if (-not $apk -or -not (Test-Path $apk)) {
    Write-Error "找不到要安装的 APK: $apk"
    exit 1
}

Write-Host "==> 安装: $apk" -ForegroundColor Cyan
& $Adb -s $serial install -r $apk 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Error "安装失败 (exit=$LASTEXITCODE)"
    exit $LASTEXITCODE
}
Write-Host "==> 安装成功" -ForegroundColor Green
