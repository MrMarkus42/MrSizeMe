# Signs the files of a DRAFT release with your secret key and uploads the
# signatures. Run on your own computer after the release workflow made the
# draft:  .\scripts\sign-release.ps1 v1.0.0
#
# Needs: minisign (winget install jedisct1.minisign) and the GitHub CLI (gh),
# signed in with your account. Publishing stays a separate step you do yourself.

param([Parameter(Mandatory = $true)][string]$Tag)

$ErrorActionPreference = 'Stop'
$repo = 'MrMarkus42/MrSizeMe'
$secret = Join-Path $env:USERPROFILE '.minisign\mrsizeme.key'
foreach ($tool in 'minisign', 'gh') {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { Write-Host "$tool fehlt."; exit 1 }
}
if (-not (Test-Path $secret)) { Write-Host "Kein geheimer Schlüssel unter $secret"; exit 1 }

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("mrsizeme-sign-" + [guid]::NewGuid())
New-Item -ItemType Directory $work | Out-Null
try {
    & gh release download $Tag --repo $repo --dir $work --pattern '*' --skip-existing
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    $files = Get-ChildItem $work -File | Where-Object { $_.Extension -ne '.minisig' }
    Write-Host 'Zu signieren:'
    $files | ForEach-Object { Write-Host "  $($_.Name)" }
    # One call for all files: the password is asked once.
    & minisign -S -s $secret -t "MrSizeMe $Tag" -m @($files.FullName)
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    $sigs = Get-ChildItem $work -Filter '*.minisig'
    & gh release upload $Tag --repo $repo --clobber @($sigs.FullName)
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    Write-Host ''
    Write-Host 'Signaturen hochgeladen. Bitte den Entwurf auf GitHub ansehen und dann selbst veröffentlichen:'
    Write-Host "  gh release edit $Tag --repo $repo --draft=false"
} finally {
    Remove-Item -Recurse -Force $work
}
