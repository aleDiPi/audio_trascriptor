# Installa Visual Studio Build Tools (workload C++) e Rust tramite winget, solo se mancanti.
# Uso:  powershell -ExecutionPolicy Bypass -File scripts\setup-windows.ps1

$ErrorActionPreference = 'Stop'

if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
    throw 'winget non trovato. Aggiorna "Programma di installazione app" dal Microsoft Store e riprova.'
}

# --- Visual Studio Build Tools (strumenti C++ richiesti dal linker di Rust) ---
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$vsOk = $false
if (Test-Path $vswhere) {
    $path = & $vswhere -products * -latest -property installationPath `
        -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64
    $vsOk = [bool]$path
}
if ($vsOk) {
    Write-Host 'Build Tools C++: gia'' installati.'
} else {
    Write-Host 'Installo Visual Studio Build Tools (workload C++)... puo'' richiedere diversi minuti.'
    winget install --id Microsoft.VisualStudio.2022.BuildTools -e --accept-package-agreements --accept-source-agreements `
        --override '--wait --passive --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended'
}

# --- Rust (rustup) ---
$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if (-not $cargo) { $cargo = Test-Path (Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe') }
if ($cargo) {
    Write-Host 'Rust: gia'' installato.'
} else {
    Write-Host 'Installo Rust (rustup)...'
    winget install --id Rustlang.Rustup -e --accept-package-agreements --accept-source-agreements
}

Write-Host ''
Write-Host 'Fatto. Chiudi e riapri il terminale, poi esegui:  cargo run --release'
