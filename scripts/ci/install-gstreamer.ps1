$ErrorActionPreference = 'Stop'
$version = $env:GST_VERSION
if ($version -ne '1.26.11') { throw 'Unexpected GStreamer SDK version' }
$base = "https://gstreamer.freedesktop.org/pkg/windows/$version/msvc"
# 1.26 changed the installer layout and default location. Let both installers
# use the same supported default; discover the actual SDK after installation.
foreach ($name in @("gstreamer-1.0-msvc-x86_64-$version.msi", "gstreamer-1.0-devel-msvc-x86_64-$version.msi")) {
  $file = Join-Path $env:RUNNER_TEMP $name
  Invoke-WebRequest "$base/$name" -OutFile $file
  Invoke-WebRequest "$base/$name.sha256sum" -OutFile "$file.sha256sum"
  $expected = ((Get-Content "$file.sha256sum" -Raw).Trim() -split '\s+')[0]
  if ($expected -notmatch '^[0-9a-fA-F]{64}$' -or (Get-FileHash $file -Algorithm SHA256).Hash -ne $expected) {
    throw "SDK checksum mismatch: $name"
  }
  $p = Start-Process msiexec.exe -Wait -PassThru -ArgumentList @('/i', ('"' + $file + '"'), '/qn', '/norestart', 'ADDLOCAL=ALL')
  if ($p.ExitCode -notin @(0,3010)) { throw "MSI install failed: $($p.ExitCode)" }
}
$candidates = @(
  [Environment]::GetEnvironmentVariable('GSTREAMER_1_0_ROOT_MSVC_X86_64','Machine'),
  [Environment]::GetEnvironmentVariable('GSTREAMER_1_0_ROOT_MSVC_X86_64','User'),
  [Environment]::GetEnvironmentVariable('GSTREAMER_ROOT_X86_64','Machine'),
  [Environment]::GetEnvironmentVariable('GSTREAMER_ROOT_X86_64','User'),
  "$env:ProgramFiles\gstreamer\1.0\msvc_x86_64",
  "$env:ProgramFiles\gstreamer",
  'C:\gstreamer\1.0\msvc_x86_64'
)
$valid = { param($path)
  (Test-Path (Join-Path $path 'lib\pkgconfig\gstreamer-1.0.pc')) -and
  (Test-Path (Join-Path $path 'lib\pkgconfig\gstreamer-webrtc-1.0.pc')) -and
  (Test-Path (Join-Path $path 'bin\gst-inspect-1.0.exe'))
}
$root = $candidates | Where-Object { $_ -and (Test-Path $_) -and (& $valid $_) } | Select-Object -First 1
if (!$root) {
  foreach ($dir in @("$env:ProgramFiles\gstreamer", 'C:\gstreamer')) {
    if (!(Test-Path $dir)) { continue }
    $root = Get-ChildItem -Path $dir -Recurse -File -Filter 'gstreamer-1.0.pc' -ErrorAction SilentlyContinue |
      ForEach-Object { $_.Directory.Parent.Parent.FullName } |
      Where-Object { $_ -match 'msvc.*(x86_64|x64)' -and (& $valid $_) } | Select-Object -First 1
    if ($root) { break }
  }
}
if (!$root) { throw 'Could not locate installed MSVC x64 SDK with runtime and WebRTC development files' }
$root = (Resolve-Path $root).Path
$pkg = @("$root\bin\pkg-config.exe", "$root\bin\pkgconf.exe") |
  Where-Object { Test-Path $_ } | Select-Object -First 1
if (!$pkg) {
  # pkgconf is a compatible tool; still restrict discovery to the verified SDK.
  choco install pkgconfiglite --yes --no-progress
  if ($LASTEXITCODE -ne 0) { throw 'pkg-config installation failed' }
  $cmd = Get-Command pkg-config.exe -ErrorAction SilentlyContinue
  if (!$cmd) {
    $pkg = "$env:ChocolateyInstall\bin\pkg-config.exe"
  } else { $pkg = $cmd.Source }
  if (!(Test-Path $pkg)) { throw 'No compatible pkg-config executable found' }
}
Write-Host "Using GStreamer root: $root"
Write-Host "Using pkg-config: $pkg"
"$root\bin" | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append
(Split-Path $pkg) | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append
"PKG_CONFIG=$pkg" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
"PKG_CONFIG_PATH=$root\lib\pkgconfig" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
"PKG_CONFIG_LIBDIR=$root\lib\pkgconfig" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
"GSTREAMER_1_0_ROOT_MSVC_X86_64=$root" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
