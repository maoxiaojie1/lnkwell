# Install Android SDK components and accept licenses
$sdk = if ($env:ANDROID_HOME) { $env:ANDROID_HOME }
       elseif ($env:ANDROID_SDK_ROOT) { $env:ANDROID_SDK_ROOT }
       else { Join-Path $env:LOCALAPPDATA 'Android\Sdk' }
$sdkmanager = Join-Path $sdk 'cmdline-tools\latest\bin\sdkmanager.bat'
if (-not $env:JAVA_HOME) {
    $studioJbr = 'C:\Program Files\Android\Android Studio\jbr'
    if (Test-Path $studioJbr) { $env:JAVA_HOME = $studioJbr }
}

Write-Output "sdkmanager path: $sdkmanager"
Write-Output "exists: $(Test-Path $sdkmanager)"
if (-not (Test-Path $sdkmanager)) { exit 1 }

# Accept licenses: pipe many "y" lines, keep output visible for diagnostics
1..300 | ForEach-Object { 'y' } | & $sdkmanager --licenses 2>&1 | Select-Object -Last 3
Write-Output "licenses exit=$LASTEXITCODE"

& $sdkmanager "platform-tools" "platforms;android-34" "build-tools;34.0.0" "ndk;26.1.10909125" "emulator" "system-images;android-34;google_apis;x86_64"
Write-Output "exit=$LASTEXITCODE"
