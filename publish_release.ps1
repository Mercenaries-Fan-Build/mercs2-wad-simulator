<#
  publish_release.ps1 - publish the wad_simulator workspace crates to crates.io in
  dependency order, respecting crates.io rate limits, and RESUMABLE.

  crates.io limits:
    * brand-new crate:  burst of 5, then 1 every 10 minutes
    * new version of an existing crate: burst of 30, then 1 per minute

  The NEW crates are the bottleneck. This script skips anything already live,
  publishes in a dependency-valid order, and sleeps 10 min before each new-crate
  publish once the burst-of-5 is spent - so a single run just works, and a
  crash/429 can be resumed by re-running it.

  Prereqs: `cargo login` (or $env:CARGO_REGISTRY_TOKEN) done first.
  Usage:   .\publish_release.ps1            # real publish
           .\publish_release.ps1 -DryRun    # print the plan, publish nothing
#>
[CmdletBinding()]
param([switch]$DryRun)
Set-Location $PSScriptRoot

# Dependency-valid publish order. Each crate's internal deps appear earlier.
$Order = @(
  'mercs2_formats',   # existing 2.0.0 - foundational
  'mercs2_core',      # NEW - foundational
  'loadprobe',        # existing 2.0.0 - standalone
  'ucfx_byteswap',    # existing 2.0.0 - needs formats
  'mercs2_audio','mercs2_ai','mercs2_anim','mercs2_combat','mercs2_decal','mercs2_faction',
  'mercs2_jobs','mercs2_net','mercs2_physics','mercs2_player','mercs2_population',
  'mercs2_vehicle','mercs2_water','mercs2_ui',              # NEW - need core+formats
  'mercs2_smuggler','wad_builder',                          # NEW - need formats
  'mercs2_reassemble',                                      # NEW - standalone
  'mercs2_script',                                          # NEW - needs ui,core
  'wad_simulator',                                          # existing 2.0.0 - needs formats,audio,ucfx
  'mercs2_engine',                                          # NEW - needs all subsystems + script
  'mercs2_probe','mercs2_game','mercs2_workshop'            # NEW - need engine
)

$UA = @{ 'User-Agent' = 'publish_release (mercs2 workspace)' }

function Get-TargetVersion($c) {
  $m = Select-String -Path "crates/$c/Cargo.toml" -Pattern '^version = "(.*)"' | Select-Object -First 1
  $m.Matches[0].Groups[1].Value
}
function Test-VersionLive($c, $v) {   # exact version already on crates.io?
  try { Invoke-RestMethod -Headers $UA -Uri "https://crates.io/api/v1/crates/$c/$v" -ErrorAction Stop | Out-Null; $true }
  catch { $false }
}
function Test-CrateExists($c) {       # crate has ANY published version (i.e. NOT brand-new)?
  try { Invoke-RestMethod -Headers $UA -Uri "https://crates.io/api/v1/crates/$c" -ErrorAction Stop | Out-Null; $true }
  catch { $false }
}

$burst = 0   # NEW-crate publishes done in THIS run (server burst budget = 5)

foreach ($c in $Order) {
  $v = Get-TargetVersion $c
  if (Test-VersionLive $c $v) { Write-Host "== skip  $c $v (already live)"; continue }
  $isNew = -not (Test-CrateExists $c)

  if ($isNew -and $burst -ge 5) {
    Write-Host "-- rate limit: new-crate burst spent; sleeping 10 min before $c ..."
    if (-not $DryRun) { Start-Sleep -Seconds 600 }
  }
  Write-Host "== publish $c $v  (new=$isNew)"
  if ($DryRun) { if ($isNew) { $burst++ }; continue }

  # Retry on a rate-limit rejection (10-min backoff); treat 'already uploaded' as done;
  # abort on any other error (fix it, then re-run - live crates are skipped on resume).
  while ($true) {
    $out = (cargo publish -p $c 2>&1 | Out-String)
    $rc = $LASTEXITCODE
    Write-Host $out
    if ($rc -eq 0 -or $out -match '(?i)already (uploaded|exists)') { break }
    if ($out -match '(?i)rate limit|429|too many requests') {
      Write-Host "-- rate limited on $c; waiting 10 min and retrying ..."
      Start-Sleep -Seconds 600; continue
    }
    Write-Host "!! $c failed for a non-rate-limit reason (see above). Fix and re-run."
    exit 1
  }
  if ($isNew) { $burst++ }
}

Write-Host "All done. Live versions:"
foreach ($c in $Order) {
  try { $mv = (Invoke-RestMethod -Headers $UA -Uri "https://crates.io/api/v1/crates/$c" -ErrorAction Stop).crate.max_version }
  catch { $mv = '<none>' }
  '{0,-18} {1}' -f $c, $mv | Write-Host
}
