# Installs the vibefork agent for the current user (no administrator rights): downloads the binaries of
# the release channel, checks their SHA-256 against the channel's manifest, runs `vibefork install`,
# starts the agent and opens the interface.
#
#   irm https://vibefork.me/install.ps1 | iex
#
# $env:VF_UPDATE_URL — release channel (default: https://code.vibefork.me/releases/stable);
# $env:VF_NO_OPEN = "1" — do not open the browser. Later updates are verified by the agent's built-in release key.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$channel = if ($env:VF_UPDATE_URL) { $env:VF_UPDATE_URL.TrimEnd('/') } else { 'https://code.vibefork.me/releases/stable' }
$arch = switch ($env:PROCESSOR_ARCHITECTURE) { 'AMD64' { 'x86_64' } 'ARM64' { 'aarch64' } default { throw "vibefork: unsupported processor $env:PROCESSOR_ARCHITECTURE" } }

$tmp = Join-Path ([IO.Path]::GetTempPath()) ("vibefork-install-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    $manifest = Invoke-RestMethod "$channel/manifest.json"

    # Downloads one binary of the channel and checks it; optional binaries may be absent.
    function Get-Binary([string]$binary, [bool]$optional) {
        $name = "$binary-windows-$arch.exe"
        $expected = $manifest.files.$name
        if (-not $expected) {
            if ($optional) { return }
            throw "vibefork: the release has no build for windows-$arch"
        }
        $path = Join-Path $tmp "$binary.exe"
        Invoke-WebRequest "$channel/$name" -OutFile $path -UseBasicParsing
        if ((Get-FileHash $path -Algorithm SHA256).Hash.ToLower() -ne $expected) {
            throw "vibefork: $name does not match the release manifest"
        }
    }

    Get-Binary 'vibefork' $false
    Get-Binary 'vibefork-tray' $true
    & (Join-Path $tmp 'vibefork.exe') install
    if ($LASTEXITCODE -ne 0) { throw 'vibefork: installation failed' }
    $dir = if ($env:VF_INSTALL_DIR) { $env:VF_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\vibefork' }
    & (Join-Path $dir 'vibefork.exe') start
    if (-not $env:VF_NO_OPEN) { & (Join-Path $dir 'vibefork.exe') open }
    Write-Host "vibefork is installed: $dir\vibefork.exe (try ``vibefork doctor`` in a new terminal)"
}
finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
