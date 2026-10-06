# temporary stub (#479): the installer moved to tool/tasks/installer.ps1. the
# README and the published pages still point here until the next release, so
# this fetches the moved script and runs it with the same parameters. removed
# in the first release after the move.
param(
    [string]$Version = "",
    [switch]$Uninstall
)

$ErrorActionPreference = "Stop"
$Url = "https://raw.githubusercontent.com/hoffresearch/urna/main/tool/tasks/installer.ps1"
& ([scriptblock]::Create((Invoke-RestMethod $Url))) @PSBoundParameters
