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
  'mercs2_mesh',      # carved out of formats - needs formats
  'mercs2_luac',      # carved out of formats - needs formats
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
function Get-LastPublish($c) {        # timestamp of the crate's most recent publish ($null if none)
  try { (Invoke-RestMethod -Headers $UA -Uri "https://crates.io/api/v1/crates/$c" -ErrorAction Stop).crate.updated_at }
  catch { $null }
}
function Get-CommitsSince($c, $since) {  # commits touching crates/<c>/ since that instant
  $iso = ([datetime]$since).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
  @(git log --oneline --since=$iso -- "crates/$c/" 2>$null).Count
}

$burst = 0        # NEW-crate publishes done in THIS run (server burst budget = 5)
$needsBump = @()  # live-but-changed crates: a release is due but the version was never bumped

foreach ($c in $Order) {
  $v = Get-TargetVersion $c
  # Already live at the target version. Two very different reasons, and conflating them is how a
  # crate with real changes gets left un-released: either nothing changed since the last publish
  # (a genuine no-op skip), or someone forgot to bump. Distinguish by commit count, and make the
  # second case loud - cargo cannot re-publish an existing version, so silence would look like success.
  if (Test-VersionLive $c $v) {
    $since = Get-LastPublish $c
    $n = 0
    if ($since) { $n = Get-CommitsSince $c $since }
    if ($n -gt 0) {
      Write-Host "!! NEEDS BUMP  $c $v is live but has $n commit(s) since that release - NOT published."
      $needsBump += "$c ($n commits)"
    } else {
      Write-Host "== skip  $c $v (up to date - live, no commits since)"
    }
    continue
  }
  $isNew = -not (Test-CrateExists $c)

  if ($isNew -and $burst -ge 5) {
    Write-Host "-- rate limit: new-crate burst spent; sleeping 10 min before $c ..."
    if (-not $DryRun) { Start-Sleep -Seconds 600 }
  }
  Write-Host "== publish $c $v  (new=$isNew)"
  if ($DryRun) { if ($isNew) { $burst++ }; continue }

  # Retry on a rate-limit rejection (10-min backoff); treat 'already uploaded' as done;
  # abort on any other error (fix it, then re-run - live crates are skipped on resume).
  #
  # cargo writes ALL its progress and diagnostics to stderr. Do NOT use `2>&1` here: in PS 5.1
  # that merges a native exe's stderr into the OBJECT pipeline as ErrorRecords, so every ordinary
  # "Updating crates.io index" line is re-rendered as a NativeCommandError block (complete with
  # "At line:char" and CategoryInfo) and that rendering lands in $out - which is the text the
  # rate-limit / already-uploaded regexes below are matched against. It also sets $? to false on
  # a perfectly clean exit. Capture stderr to a FILE instead and re-join it as plain text.
  while ($true) {
    $errPath = [IO.Path]::GetTempFileName()
    $out = (& cargo publish -p $c 2>$errPath | Out-String)
    $rc = $LASTEXITCODE
    $out += (Get-Content $errPath -Raw)
    try { Remove-Item $errPath -Force -ErrorAction Stop } catch {}
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

if ($needsBump.Count -gt 0) {
  Write-Host ""
  Write-Host "!! $($needsBump.Count) crate(s) have commits since their last release but were NOT bumped,"
  Write-Host "!! so nothing was published for them:"
  foreach ($x in $needsBump) { Write-Host "!!   $x" }
  Write-Host "!! Bump the version in crates/<name>/Cargo.toml (and [workspace.dependencies]) and re-run."
}

Write-Host "All done. Live versions:"
foreach ($c in $Order) {
  try { $mv = (Invoke-RestMethod -Headers $UA -Uri "https://crates.io/api/v1/crates/$c" -ErrorAction Stop).crate.max_version }
  catch { $mv = '<none>' }
  '{0,-18} {1}' -f $c, $mv | Write-Host
}

# Non-zero if any crate was skipped despite having changes, so a forgotten bump cannot pass as
# success in CI or in a scrollback. Everything else already published fine; re-running is safe.
if ($needsBump.Count -gt 0) { exit 1 }
