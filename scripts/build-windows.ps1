param(
    [ValidateSet("x64", "arm64")]
    [string]$Architecture = "x64"
)

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent $PSScriptRoot
$RustTarget = if ($Architecture -eq "arm64") { "aarch64-pc-windows-msvc" } else { "x86_64-pc-windows-msvc" }
$Runtime = "win-$Architecture"
$Output = Join-Path $ProjectRoot "dist\windows-$Architecture"

Push-Location $ProjectRoot
try {
    rustup target add $RustTarget
    cargo build --release -p miramd-core -p miramd-mcp --target $RustTarget
    dotnet publish windows\MiraMD.Windows.csproj -c Release -r $Runtime -p:Platform=$Architecture -o $Output
    Copy-Item "target\$RustTarget\release\miramd_core.dll" $Output -Force
    Copy-Item "target\$RustTarget\release\miramd-mcp.exe" $Output -Force
    Write-Host "Built $Output"
}
finally {
    Pop-Location
}
