# Starts a Linux container whose windows appear on the Windows desktop through WSLg.
# Usage: docker/run-gui.ps1 -Image survivorsforge-dev -AddOns <copy of an AddOns folder> [-Command "..."]
# Never pass the real game folder: the container installs and removes addons in it.
param(
    [string]$Image = "survivorsforge-dev",
    [Parameter(Mandatory = $true)][string]$AddOns,
    [string]$Data = "",
    [string]$Command = "",
    [switch]$X11
)

$repo = (Resolve-Path "$PSScriptRoot\..").Path
$wslg = "/run/desktop/mnt/host/wslg"
$args = @(
    "run", "--rm", "-it",
    "-v", "${wslg}/.X11-unix:/tmp/.X11-unix",
    "-v", "${wslg}:/mnt/wslg",
    "-e", "DISPLAY=:0",
    "-e", "XDG_RUNTIME_DIR=/mnt/wslg/runtime-dir",
    "-e", "PULSE_SERVER=/mnt/wslg/PulseServer",
    "-v", "${repo}:/src",
    "-v", "survivorsforge-target:/target",
    "-v", "survivorsforge-cargo:/usr/local/cargo/registry",
    "-v", "$((Resolve-Path $AddOns).Path):/game/benilla-config/AddOns"
)
if (-not $X11) { $args += @("-e", "WAYLAND_DISPLAY=wayland-0") }
if ($Data -ne "") { $args += @("-v", "$((Resolve-Path $Data).Path):/data", "-e", "SURVIVORSFORGE_DATA_DIR=/data") }
$args += $Image
if ($Command -ne "") { $args += @("bash", "-lc", $Command) }
docker @args
