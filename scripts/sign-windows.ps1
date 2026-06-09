<#
.SYNOPSIS
    Authenticode-sign one or more Windows files (inlook.exe, the MSI).

.DESCRIPTION
    Code signing is what lets InLook pass winget's security gate and Windows
    SmartScreen without a "Trojan:Win32/Sprisky.U!cl"-style false positive (see
    packaging/winget/README.md). An unsigned Rust binary trips Defender
    heuristics; a signed, reputable binary does not.

    The certificate is supplied as a base64-encoded PFX in the
    WINDOWS_CERT_PFX_BASE64 environment variable (a GitHub Actions secret), with
    its password in WINDOWS_CERT_PASSWORD. If WINDOWS_CERT_PFX_BASE64 is empty
    (e.g. forks, or before a cert is configured) the script is a no-op so the
    release build still succeeds — it just ships unsigned.

.PARAMETER Path
    One or more files to sign.

.EXAMPLE
    ./scripts/sign-windows.ps1 -Path target/release/inlook.exe
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string[]]$Path
)

$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($env:WINDOWS_CERT_PFX_BASE64)) {
    Write-Host "WINDOWS_CERT_PFX_BASE64 is not set - skipping code signing (unsigned build)."
    exit 0
}

# Materialise the PFX from the base64 secret into a temp file.
$tempDir = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { $env:TEMP }
$pfxPath = Join-Path $tempDir 'inlook-codesign.pfx'
[IO.File]::WriteAllBytes($pfxPath, [Convert]::FromBase64String($env:WINDOWS_CERT_PFX_BASE64))

# Locate the newest x64 signtool.exe from the installed Windows SDK(s).
$signtool = Get-ChildItem -Path "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Recurse -Filter 'signtool.exe' -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match '\\x64\\' } |
    Sort-Object FullName -Descending |
    Select-Object -First 1 -ExpandProperty FullName
if (-not $signtool) {
    throw "signtool.exe not found - install the Windows SDK or adjust this script."
}
Write-Host "Using signtool: $signtool"

# RFC 3161 timestamp so signatures stay valid after the cert expires.
$timestampUrl = 'http://timestamp.digicert.com'

try {
    foreach ($file in $Path) {
        $resolved = (Resolve-Path -LiteralPath $file).Path
        Write-Host "Signing $resolved"
        & $signtool sign `
            /f $pfxPath `
            /p $env:WINDOWS_CERT_PASSWORD `
            /fd SHA256 `
            /tr $timestampUrl `
            /td SHA256 `
            /d 'InLook' `
            $resolved
        if ($LASTEXITCODE -ne 0) { throw "signtool sign failed for $resolved (exit $LASTEXITCODE)" }

        & $signtool verify /pa /v $resolved
        if ($LASTEXITCODE -ne 0) { throw "signtool verify failed for $resolved (exit $LASTEXITCODE)" }
    }
}
finally {
    Remove-Item -LiteralPath $pfxPath -Force -ErrorAction SilentlyContinue
}
