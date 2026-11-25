$ErrorActionPreference = 'Stop'
$version = $env:GST_VERSION
if ($version -ne '1.26.11') { throw 'Unexpected GStreamer SDK version' }
$base = "https://gstreamer.freedesktop.org/pkg/windows/$version/msvc"
$root = 'C:\gstreamer\1.0\msvc_x86_64'
foreach ($name in @("gstreamer-1.0-msvc-x86_64-$version.msi", "gstreamer-1.0-devel-msvc-x86_64-$version.msi")) {
  $file = Join-Path $env:RUNNER_TEMP $name
  Invoke-WebRequest "$base/$name" -OutFile $file
  Invoke-WebRequest "$base/$name.sha256sum" -OutFile "$file.sha256sum"
  $expected = ((Get-Content "$file.sha256sum" -Raw).Trim() -split '\s+')[0]
  if ($expected -notmatch '^[0-9a-fA-F]{64}$' -or (Get-FileHash $file -Algorithm SHA256).Hash -ne $expected) {
    throw "SDK checksum mismatch: $name"
  }
  $p = Start-Process msiexec.exe -Wait -PassThru -ArgumentList @('/i', ('"' + $file + '"'), '/qn', '/norestart', 'ADDLOCAL=ALL', "INSTALLDIR=$root")
  if ($p.ExitCode -notin @(0,3010)) { throw "MSI install failed: $($p.ExitCode)" }
}
if (!(Test-Path "$root\bin\pkg-config.exe")) { throw "GStreamer pkg-config not found at $root" }
"$root\bin" | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append
"PKG_CONFIG=$root\bin\pkg-config.exe" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
"PKG_CONFIG_PATH=$root\lib\pkgconfig" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
"GSTREAMER_1_0_ROOT_MSVC_X86_64=$root" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
