# Install PTCGL Tracker from the UTM shared folder to a LOCAL drive.
#
# Run this inside the Windows guest (PowerShell). It copies the .exe off the
# network share to C:\Tools\ptcgl-tracker\, unblocks it (SmartScreen), and
# optionally launches it. Never run the .exe straight off the share — a running
# .exe is locked and SmartScreen flags share paths.
#
# Usage (PowerShell, in Windows guest):
#   .\scripts\install-windows.ps1 -Source "\\vmhost\shared\ship-windows\ptcgl-battle-log-capture.exe"
#   .\scripts\install-windows.ps1 -Source "E:\ship-windows\ptcgl-battle-log-capture.exe" -Launch
param(
  [Parameter(Mandatory = $true)]
  [string]$Source,        # Path to the .exe inside the UTM shared folder

  [string]$Destination = "C:\Tools\ptcgl-tracker\ptcgl-battle-log-capture.exe",

  [switch]$Launch
)

$ErrorActionPreference = "Stop"

Write-Host "==> Source: $Source"
Write-Host "==> Destination: $Destination"

if (!(Test-Path $Source)) {
  throw "Source not found: $Source"
}

# Make sure destination folder exists
$destDir = Split-Path -Parent $Destination
New-Item -ItemType Directory -Force -Path $destDir | Out-Null

# Copy to LOCAL drive (not the network share)
Copy-Item -Path $Source -Destination $Destination -Force

# Remove Mark-of-the-Web so SmartScreen doesn't block first launch
$zone = "$Destination:Zone.Identifier"
if (Test-Path $zone) {
  Remove-Item -Path $zone -Force
  Write-Host "==> Unblocked $Destination"
}

Write-Host "==> Installed to $Destination"

if ($Launch) {
  Write-Host "==> Launching..."
  Start-Process -FilePath $Destination
} else {
  Write-Host "==> Run it: & '$Destination'"
}
