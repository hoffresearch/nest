# nest installer for windows (issue #75). one-liner:
#
#   irm https://get.hoffresearch.com/nest.ps1 | iex
#
# mirrors scripts/install.sh: downloads the windows release zip, verifies
# its sha256 against the release checksum file, installs the binary to
# ~\.local\bin, and lays down the offline embedder payload under
# $env:LOCALAPPDATA\nest. after install the product never touches the
# network; only this script does.
#
# flags:
#   -Version vX.Y.Z   pin a release (default: latest)
#   -Uninstall        remove the binary and the payload
#
# env overrides (used by tests and custom setups):
#   NEST_RELEASE_BASE  url prefix that serves the release artifacts
#   NEST_BIN_DIR       binary install dir   (default ~\.local\bin)
#   NEST_DATA_DIR      payload parent dir   (default $env:LOCALAPPDATA)

param(
    [string]$Version = "",
    [switch]$Uninstall
)

$ErrorActionPreference = "Stop"

$Repo = "hoffresearch/nest"
$BinDir = if ($env:NEST_BIN_DIR) { $env:NEST_BIN_DIR } else { Join-Path $HOME ".local\bin" }
$DataDir = if ($env:NEST_DATA_DIR) { $env:NEST_DATA_DIR } elseif ($env:LOCALAPPDATA) { $env:LOCALAPPDATA } else { Join-Path $HOME ".local\share" }
$PayloadDir = Join-Path $DataDir "nest"

if ($Uninstall) {
    Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $BinDir "nest.exe")
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $PayloadDir
    Write-Output "nest-install: removed $BinDir\nest.exe and $PayloadDir"
    exit 0
}

# released windows builds are x86_64 only (issue #75 matrix).
if ($env:PROCESSOR_ARCHITECTURE -ne "AMD64") {
    Write-Error "nest-install: error: unsupported arch: $env:PROCESSOR_ARCHITECTURE (only x86_64 windows builds ship today)"
    exit 1
}
$Target = "x86_64-pc-windows-msvc"

if ($env:NEST_RELEASE_BASE) {
    $Base = $env:NEST_RELEASE_BASE
} elseif ($Version) {
    if (-not $Version.StartsWith("v")) { $Version = "v$Version" }
    $Base = "https://github.com/$Repo/releases/download/$Version"
} else {
    $Base = "https://github.com/$Repo/releases/latest/download"
}

$Archive = "nest-cli-$Target.zip"
$Payload = "nest-embedder-payload.tar.gz"

$Tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("nest-install-" + [System.IO.Path]::GetRandomFileName())
New-Item -ItemType Directory -Force $Tmp | Out-Null
try {
    Write-Output "nest-install: fetching $Archive + payload ($Target)"
    Invoke-WebRequest -Uri "$Base/$Archive" -OutFile (Join-Path $Tmp $Archive)
    Invoke-WebRequest -Uri "$Base/$Archive.sha256" -OutFile (Join-Path $Tmp "$Archive.sha256")
    Invoke-WebRequest -Uri "$Base/$Payload" -OutFile (Join-Path $Tmp $Payload)
    Invoke-WebRequest -Uri "$Base/$Payload.sha256" -OutFile (Join-Path $Tmp "$Payload.sha256")

    # verify both sha256 sums before anything touches the install dirs.
    foreach ($f in @($Archive, $Payload)) {
        $want = (Get-Content (Join-Path $Tmp "$f.sha256") -Raw).Trim().Split(" ")[0]
        $got = (Get-FileHash -Algorithm SHA256 (Join-Path $Tmp $f)).Hash.ToLower()
        if ($got -ne $want) {
            Write-Error "nest-install: error: sha256 mismatch for ${f}: got $got want $want"
            exit 1
        }
    }
    Write-Output "nest-install: checksums verified"

    New-Item -ItemType Directory -Force $BinDir | Out-Null
    New-Item -ItemType Directory -Force $DataDir | Out-Null
    Expand-Archive -Force (Join-Path $Tmp $Archive) $Tmp
    Copy-Item (Join-Path $Tmp "nest-cli-$Target\nest.exe") (Join-Path $BinDir "nest.exe")
    # the payload is a .tar.gz; tar ships with windows 10 1803+.
    tar -xzf (Join-Path $Tmp $Payload) -C $DataDir

    Write-Output "nest-install: installed $BinDir\nest.exe"
    Write-Output "nest-install: embedder payload at $PayloadDir"
    if (-not ($env:PATH -split ";" -contains $BinDir)) {
        Write-Output "nest-install: note: $BinDir is not on your PATH"
    }
    Write-Output "nest-install: run ``nest doctor`` to validate the install (offline)"
} finally {
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $Tmp
}
