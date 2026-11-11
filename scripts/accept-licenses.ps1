# Write known Android SDK license hashes to accept licenses non-interactively.
# These hashes are the official, fixed license markers used by sdkmanager.
$sdk = if ($env:ANDROID_HOME) { $env:ANDROID_HOME }
       elseif ($env:ANDROID_SDK_ROOT) { $env:ANDROID_SDK_ROOT }
       else { Join-Path $env:LOCALAPPDATA 'Android\Sdk' }
$licDir = Join-Path $sdk 'licenses'
New-Item -ItemType Directory -Force -Path $licDir | Out-Null

$licenses = @{
    'android-sdk-license'         = @('24333f8a63b6825ea9c5514f83c2829b004d1fee', '8933bad161af4178b1185d1a37fbf41ea5269c55', 'd56f5187479451eabf01fb78af6dfcb131a6481e')
    'android-sdk-preview-license' = @('84831b9409646a918e30573bab4c9c91346d8abd')
    'android-ndk-license'         = @('601085b94cd77f0b54ff86406957099ebe79c4d6')
    'intel-android-extra-license' = @('d975f751698a77b662f1254ddbeed3901e976f5a')
}

foreach ($name in $licenses.Keys) {
    $path = Join-Path $licDir $name
    Set-Content -Path $path -Value ($licenses[$name] -join "`n") -Encoding Ascii
    Write-Output "wrote $name"
}
Write-Output 'done'
