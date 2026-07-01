# Self-validation harness: rebuild mercs2_engine, launch it with the given args, capture the
# engine window 3x at 1s intervals to <OutDir>\shot1..3.png, then kill it. Lets the agent SEE the
# render/animation and iterate without a human in the loop.
#   powershell -File engine_shots.ps1 -EngineArgs "--wad --model 0xA3C1FABC --animate --clip 0x24F8C8E6" -OutDir <dir>
param(
  [string]$EngineArgs = "--wad --model 0xA3C1FABC --poseoracle",
  [string]$OutDir = "$PSScriptRoot\..\..\..\_shots",
  [int]$WarmupSec = 3,
  [int]$Shots = 3
)

$ErrorActionPreference = "Continue"
$repo = Resolve-Path "$PSScriptRoot\.."
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

Add-Type -AssemblyName System.Drawing
$sig = @'
[DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
[DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
[DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr hWnd);
[DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr after, int X, int Y, int cx, int cy, uint flags);
[StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
'@
Add-Type -MemberDefinition $sig -Name Win -Namespace Native

# 1) kill any running engine so the exe can be replaced
Get-Process mercs2_engine -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 500

# 2) rebuild (stderr flows to console; check exit code, don't capture with 2>&1 in PS5.1)
Push-Location $repo
& cargo build -p mercs2_engine
$buildCode = $LASTEXITCODE
Pop-Location
if ($buildCode -ne 0) {
  Write-Output "BUILD FAILED (exit $buildCode)"
  exit 1
}

# 3) launch (-NoNewWindow: no separate console window, so the winit render window is the MainWindow)
$exe = Join-Path $repo "target\debug\mercs2_engine.exe"
$p = Start-Process -FilePath $exe -ArgumentList $EngineArgs -PassThru -NoNewWindow

# poll for the window to appear (WAD load + animgroup scan + wgpu init can take 20-40s)
$h = 0
for ($t = 0; $t -lt 60; $t++) {
  Start-Sleep -Seconds 1
  $q = Get-Process mercs2_engine -ErrorAction SilentlyContinue | Select-Object -First 1
  if (-not $q) { Write-Output "engine exited before a window appeared"; break }
  $q.Refresh()
  if ($q.MainWindowHandle -ne 0) { $h = $q.MainWindowHandle; Write-Output "window up after $($t+1)s"; break }
}
Start-Sleep -Seconds $WarmupSec  # let it render a few frames

# 4) capture N shots 1s apart (handle locked from polling; the render window doesn't change)
for ($i = 1; $i -le $Shots; $i++) {
  if ($h -ne 0) {
    # force the render window above the modkit/VSCode: TOPMOST then back to normal-top
    [Native.Win]::ShowWindow($h, 9) | Out-Null   # SW_RESTORE
    [Native.Win]::SetWindowPos($h, [IntPtr](-1), 0, 0, 0, 0, 0x43) | Out-Null  # HWND_TOPMOST, NOMOVE|NOSIZE|SHOWWINDOW
    [Native.Win]::BringWindowToTop($h) | Out-Null
    [Native.Win]::SetForegroundWindow($h) | Out-Null
    Start-Sleep -Milliseconds 500
    $r = New-Object Native.Win+RECT
    [Native.Win]::GetWindowRect($h, [ref]$r) | Out-Null
    $w = $r.Right - $r.Left; $ht = $r.Bottom - $r.Top
    if ($w -gt 0 -and $ht -gt 0) {
      $bmp = New-Object System.Drawing.Bitmap $w, $ht
      $g = [System.Drawing.Graphics]::FromImage($bmp)
      $g.CopyFromScreen($r.Left, $r.Top, 0, 0, (New-Object System.Drawing.Size($w, $ht)))
      $bmp.Save((Join-Path $OutDir "shot$i.png"))
      $g.Dispose(); $bmp.Dispose()
      Write-Output "shot$i : ${w}x${ht}"
    } else { Write-Output "shot$i : window rect empty" }
  } else { Write-Output "shot$i : no window handle" }
  Start-Sleep -Seconds 1
}

# 5) kill
Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
Write-Output "done -> $OutDir"
