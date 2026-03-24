$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$sidecarRoot = Join-Path $repoRoot "sidecar"
$venvConfigPath = Join-Path $sidecarRoot ".venv\\pyvenv.cfg"
$runtimeRoot = Join-Path $sidecarRoot ".python-runtime"
$runtimeSitePackagesPath = Join-Path $runtimeRoot "Lib\\site-packages"

if (-not (Test-Path $venvConfigPath)) {
  throw "Missing sidecar virtualenv metadata at $venvConfigPath. Create sidecar/.venv before building installers."
}

$venvConfig = Get-Content $venvConfigPath
$pythonHomeLine = $venvConfig | Where-Object { $_ -like "home = *" } | Select-Object -First 1
if (-not $pythonHomeLine) {
  throw "Couldn't find the base Python home in $venvConfigPath."
}

$pythonHome = $pythonHomeLine.Substring("home = ".Length).Trim()
if (-not (Test-Path $pythonHome)) {
  throw "Base Python home '$pythonHome' does not exist. Recreate sidecar/.venv with Python 3.12 before building."
}

if (Test-Path $runtimeRoot) {
  Remove-Item $runtimeRoot -Recurse -Force
}

New-Item -ItemType Directory -Path $runtimeRoot | Out-Null

$excludedDirectories = @(
  (Join-Path $pythonHome "Lib\\site-packages"),
  (Join-Path $pythonHome "Scripts"),
  (Join-Path $pythonHome "Doc"),
  (Join-Path $pythonHome "Tools")
)

$robocopyArgs = @(
  $pythonHome,
  $runtimeRoot,
  "/E",
  "/R:1",
  "/W:1",
  "/NFL",
  "/NDL",
  "/NJH",
  "/NJS",
  "/XD"
) + $excludedDirectories

& robocopy @robocopyArgs | Out-Null
if ($LASTEXITCODE -ge 8) {
  throw "robocopy failed while staging the portable Python runtime (exit code $LASTEXITCODE)."
}

$pythonExe = Join-Path $runtimeRoot "python.exe"
if (-not (Test-Path $pythonExe)) {
  throw "Expected a staged python.exe at $pythonExe, but it was not copied."
}

$sitePackagesPath = Join-Path $sidecarRoot ".venv\\Lib\\site-packages"
if (-not (Test-Path $sitePackagesPath)) {
  throw "Missing sidecar site-packages at $sitePackagesPath. Install sidecar dependencies before building."
}

New-Item -ItemType Directory -Path $runtimeSitePackagesPath -Force | Out-Null

$sitePackagesRobocopyArgs = @(
  $sitePackagesPath,
  $runtimeSitePackagesPath,
  "/E",
  "/R:1",
  "/W:1",
  "/NFL",
  "/NDL",
  "/NJH",
  "/NJS",
  "/XF",
  "*.lib"
)

& robocopy @sitePackagesRobocopyArgs | Out-Null
if ($LASTEXITCODE -ge 8) {
  throw "robocopy failed while staging sidecar site-packages (exit code $LASTEXITCODE)."
}

$runtimeSizeMb = [math]::Round(((Get-ChildItem $runtimeRoot -Recurse -Force | Measure-Object Length -Sum).Sum / 1MB), 2)
Write-Host "Staged portable Python runtime from $pythonHome"
Write-Host "Destination: $runtimeRoot"
Write-Host "Runtime size: $runtimeSizeMb MB"
