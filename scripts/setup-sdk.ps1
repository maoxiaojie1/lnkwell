# Install Android SDK components and accept licenses
$sdk = 'C:\Users\xiaojiemao\AppData\Local\Android\Sdk'
$sdkmanager = "$sdk\cmdline-tools\latest\bin\sdkmanager.bat"
$env:JAVA_HOME = 'C:\Program Files\Microsoft\jdk-17.0.20.8-hotspot'

Write-Output "sdkmanager path: $sdkmanager"
Write-Output "exists: $(Test-Path $sdkmanager)"
if (-not (Test-Path $sdkmanager)) { exit 1 }

# Accept licenses: pipe many "y" lines, keep output visible for diagnostics
1..300 | ForEach-Object { 'y' } | & $sdkmanager --licenses 2>&1 | Select-Object -Last 3
Write-Output "licenses exit=$LASTEXITCODE"

& $sdkmanager "platform-tools" "platforms;android-34" "build-tools;34.0.0" "ndk;26.1.10909125" "emulator" "system-images;android-34;google_apis;x86_64"
Write-Output "exit=$LASTEXITCODE"
