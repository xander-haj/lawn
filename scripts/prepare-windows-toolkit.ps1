# Downloads and normalizes the Windows toolkit that Tauri bundles as app resources.
# The release workflow runs this before packaging the Windows installer so users get
# Git, Python, TCC, and SDL2 without installing those tools separately.
param(
  [string]$OutputRoot = "src-tauri/bundled-tools/windows",
  [string]$GitUrl = "",
  [string]$PythonUrl = "https://www.nuget.org/api/v2/package/python/3.12.7",
  [string]$Sdl2Url = "https://github.com/libsdl-org/SDL/releases/download/release-2.30.11/SDL2-devel-2.30.11-VC.zip",
  [string]$TccUrl = "https://download.savannah.gnu.org/releases/tinycc/tcc-0.9.27-win64-bin.zip"
)

$ErrorActionPreference = "Stop"

# Resolves paths relative to the repository root even when the script is launched
# from GitHub Actions, PowerShell, or a developer shell.
$repoRoot = Split-Path -Parent $PSScriptRoot
$toolRoot = Join-Path $repoRoot $OutputRoot
$downloadRoot = Join-Path $toolRoot "_downloads"

if ([string]::IsNullOrWhiteSpace($GitUrl)) {
  $GitUrl = "https://github.com/git-for-windows/git/releases/download/v2.54.0.windows.1/" +
    "PortableGit-2.54.0-64-bit.7z.exe"
}

# Recreate only the generated toolkit folder so stale binaries cannot leak between
# release builds with different pinned versions.
if (Test-Path $toolRoot) {
  Remove-Item $toolRoot -Recurse -Force
}

New-Item -ItemType Directory -Force -Path $downloadRoot | Out-Null

# Downloads one pinned package and fails loudly if the source is unavailable.
function Save-ToolArchive {
  param(
    [string]$Url,
    [string]$FileName
  )

  $target = Join-Path $downloadRoot $FileName
  Invoke-WebRequest -Uri $Url -OutFile $target
  return $target
}

# Expands a normal zip/nupkg archive into the target folder.
function Expand-ZipArchive {
  param(
    [string]$Archive,
    [string]$Destination
  )

  New-Item -ItemType Directory -Force -Path $Destination | Out-Null
  Expand-Archive -Path $Archive -DestinationPath $Destination -Force
}

# Extracts Git for Windows' self-extracting 7z package with the runner's 7-Zip.
function Expand-SevenZipArchive {
  param(
    [string]$Archive,
    [string]$Destination
  )

  New-Item -ItemType Directory -Force -Path $Destination | Out-Null
  $sevenZip = "${env:ProgramFiles}\7-Zip\7z.exe"

  if (!(Test-Path $sevenZip)) {
    throw "7-Zip was not found at $sevenZip."
  }

  & $sevenZip x $Archive "-o$Destination" -y | Out-Host
}

# Copies archive contents whether the vendor zip contains files at the root or wraps
# everything in one versioned top-level folder.
function Copy-NormalizedContents {
  param(
    [string]$Source,
    [string]$Destination
  )

  $children = Get-ChildItem -Path $Source
  $contentRoot = $Source

  if ($children.Count -eq 1 -and $children[0].PSIsContainer) {
    $contentRoot = $children[0].FullName
  }

  New-Item -ItemType Directory -Force -Path $Destination | Out-Null
  Copy-Item -Path (Join-Path $contentRoot "*") -Destination $Destination -Recurse -Force
}

$gitArchive = Save-ToolArchive -Url $GitUrl -FileName "PortableGit.7z.exe"
$pythonArchive = Save-ToolArchive -Url $PythonUrl -FileName "python.nupkg"
$sdlArchive = Save-ToolArchive -Url $Sdl2Url -FileName "SDL2-devel-VC.zip"
$tccArchive = Save-ToolArchive -Url $TccUrl -FileName "tcc-win64.zip"

Expand-SevenZipArchive -Archive $gitArchive -Destination (Join-Path $toolRoot "git")
Expand-ZipArchive -Archive $pythonArchive -Destination (Join-Path $toolRoot "python")

$sdlExtract = Join-Path $downloadRoot "sdl2-expanded"
$tccExtract = Join-Path $downloadRoot "tcc-expanded"
Expand-ZipArchive -Archive $sdlArchive -Destination $sdlExtract
Expand-ZipArchive -Archive $tccArchive -Destination $tccExtract
Copy-NormalizedContents -Source $sdlExtract -Destination (Join-Path $toolRoot "sdl2")
Copy-NormalizedContents -Source $tccExtract -Destination (Join-Path $toolRoot "tcc")

Remove-Item $downloadRoot -Recurse -Force

# Validate the exact files the Rust launcher will look for at runtime.
$requiredFiles = @(
  "git/cmd/git.exe",
  "python/tools/python.exe",
  "tcc/tcc.exe",
  "sdl2/include/SDL.h",
  "sdl2/lib/x64/SDL2.dll"
)

foreach ($file in $requiredFiles) {
  $path = Join-Path $toolRoot $file

  if (!(Test-Path $path)) {
    throw "Prepared toolkit is missing required file: $file"
  }
}

Write-Host "Windows toolkit prepared at $toolRoot"
