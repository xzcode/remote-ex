$ErrorActionPreference = 'Stop'
if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted') { throw 'Installer fixture requires a disposable GitHub-hosted runner' }
if ($env:GITHUB_SHA -notmatch '^[0-9a-f]{40}$') { throw 'Invalid build revision' }
$id = $env:GITHUB_SHA.Substring(0,12)
if ($env:REMOTE_EX_BUILD_TAG) {
  if ($env:REMOTE_EX_BUILD_TAG -notmatch '^v[0-9]+\.[0-9]+\.[0-9]+-alpha\.[0-9]+$') { throw 'Invalid release tag' }
  $id = "$env:REMOTE_EX_BUILD_TAG-$id"
}
$nsis = Join-Path ${env:ProgramFiles(x86)} 'NSIS\makensis.exe'
if (!(Test-Path $nsis)) {
  choco install nsis --yes --no-progress
  if ($LASTEXITCODE -ne 0 -or !(Test-Path $nsis)) { throw 'Cannot install NSIS build tool' }
}
$stage = (Resolve-Path stage).Path
$out = Join-Path (Resolve-Path artifacts).Path 'Remote-EX-dev-Windows-x64-Setup.exe'
& $nsis "/DBUILD_ID=$id" "/DSTAGE=$stage" "/DOUTFILE=$out" scripts/ci/windows-installer.nsi
if ($LASTEXITCODE -ne 0 -or !(Test-Path $out)) { throw 'NSIS compilation failed' }
$dir = Join-Path $env:LOCALAPPDATA "Programs\Remote EX\$id"
if (Test-Path $dir) { throw 'Expected unused fixture directory' }
try {
  $p = Start-Process -FilePath $out -ArgumentList '/S' -Wait -PassThru
  if ($p.ExitCode -ne 0) { throw 'Installer fixture failed' }
  python scripts/ci/check_bundle.py verify --dir $dir --platform windows
  if ($LASTEXITCODE -ne 0) { throw 'Installed components mismatch' }
  python scripts/ci/check_bundle.py probe --dir $dir --platform windows
  if ($LASTEXITCODE -ne 0) { throw 'Installed runtime probe failed' }
} finally {
  $uninstaller = Join-Path $dir 'Uninstall.exe'
  if (Test-Path $uninstaller) {
    $p = Start-Process -FilePath $uninstaller -ArgumentList '/S' -Wait -PassThru
    if ($p.ExitCode -ne 0) { throw 'Uninstaller fixture failed' }
    for ($i=0; $i -lt 80 -and (Test-Path (Join-Path $dir 'remote-ex-desktop.exe')); $i++) { Start-Sleep -Milliseconds 250 }
    if (Test-Path (Join-Path $dir 'remote-ex-desktop.exe')) { throw 'Installed executable not removed' }
  } else { throw 'Missing fixture uninstaller' }
}
Write-Host "PASS per-user installer and uninstaller fixture: $id"
