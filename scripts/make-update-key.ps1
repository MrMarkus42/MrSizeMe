# Makes the key pair for signing updates. Run once, on your own computer.
#
# The secret key is written to your user folder (never into the project) and
# is protected by a password that minisign asks for. Only the public key
# goes into the project: assets/update-key.pub.
#
# Needs minisign:  winget install jedisct1.minisign

$ErrorActionPreference = 'Stop'
if (-not (Get-Command minisign -ErrorAction SilentlyContinue)) {
    Write-Host 'minisign fehlt. Bitte zuerst installieren:  winget install jedisct1.minisign'
    exit 1
}
$secretDir = Join-Path $env:USERPROFILE '.minisign'
$secret = Join-Path $secretDir 'mrsizeme.key'
$public = Join-Path $PSScriptRoot '..\assets\update-key.pub'
if (Test-Path $secret) {
    Write-Host "Es gibt schon einen geheimen Schlüssel: $secret"
    Write-Host 'Nicht überschreiben: alte Versionen prüfen Updates mit dem alten Schlüssel.'
    exit 1
}
New-Item -ItemType Directory -Force $secretDir | Out-Null
# minisign asks for the password itself; it is never stored.
& minisign -G -f -p $public -s $secret
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Host ''
Write-Host "Geheimer Schlüssel: $secret  (bitte sichern, z. B. auf einem USB-Stick; nie hochladen)"
Write-Host "Öffentlicher Schlüssel: $public  (kommt ins Projekt)"
