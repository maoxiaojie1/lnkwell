# 解压 Android 命令行工具到 cmdline-tools/latest
$zip = "$env:TEMP\cmdline-tools.zip"
$extract = "$env:TEMP\cmdline-tools-extract"
$dest = "$env:LOCALAPPDATA\Android\Sdk\cmdline-tools\latest"

if (-not (Test-Path $zip)) { Write-Error "zip not found: $zip"; exit 1 }

Expand-Archive -Path $zip -DestinationPath $extract -Force
New-Item -ItemType Directory -Force -Path $dest | Out-Null
Copy-Item -Recurse -Force "$extract\cmdline-tools\*" $dest

$sdkmanager = Join-Path $dest 'bin\sdkmanager.bat'
if (Test-Path $sdkmanager) {
    Write-Output "OK: sdkmanager at $sdkmanager"
} else {
    Write-Error "sdkmanager not found after install"
    exit 1
}
