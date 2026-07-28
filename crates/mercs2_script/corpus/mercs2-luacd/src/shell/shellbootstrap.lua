import("MrxSoundShellBootstrap")
import("MrxGuiShellBootstrap")
import("MrxGuiBase")
import("MrxSound")
tMovies = {
  {"EA", -1},
  {"Pandemic", -1}
}
_oIntroMovieWidget = nil

function _PlayMovie()
  _nMovie = _nMovie or 0
  _nMovie = _nMovie + 1
  Debug.Printf("Attempting to play movie " .. _nMovie)
  if not (tMovies[_nMovie] and tMovies[_nMovie][1]) or not _oIntroMovieWidget then
    Debug.Printf("All movies complete")
    _nMovie = nil
    MrxGuiBase.RemoveWidget(_oIntroMovieWidget)
    _oIntroMovieWidget:delete()
    _oIntroMovieWidget = nil
    StartPrecache()
    return
  end
  local sMovie = tMovies[_nMovie][1]
  local nTime = tMovies[_nMovie][2]
  local bLoop = -1 < nTime
  Debug.Printf("Playing " .. sMovie .. " for " .. nTime .. " seconds...")
  
  local function _EndMovie(sMovie)
    Debug.Printf("Movie " .. sMovie .. " time up...")
    _oIntroMovieWidget:Stop()
    _PlayMovie()
  end
  
  _oIntroMovieWidget:SetMovie(sMovie)
  _oIntroMovieWidget:SetEndCallback(_EndMovie, {sMovie})
  _oIntroMovieWidget:Play()
end

IsFinishedPrecache = 0

function FinishedPrecache()
  IsFinishedPrecache = 1
end

function _WaitPrecache()
  Debug.Printf("Waiting for precache to finish...")
  if IsFinishedPrecache > 0 then
    Start()
    return
  end
  Debug.Printf("  Creating Wait Precache Event.")
  Event.Create(Event.TimerRelative, {2}, _WaitPrecache)
end

function StartPrecache()
  Debug.Printf(" StartPrecache()")
  if Sys.LTIGetPrecacheBypass() > 0 then
    Start()
    return
  end
  _WaitPrecache()
  MrxGuiShellBootstrap.EnterPrecache()
end

function Init()
  Debug.Printf("Top of ShellBootstrap::Init()")
  Sys.SetLuaSaveVersion(GetSaveDataVersion())
  Graphics.SetGamma(0, 0.8, 1)
  if not Sys.PlayIntroMovies() then
    Start()
    return
  end
  _oIntroMovieWidget = MrxGuiBase.MovieWidget:new()
  _oIntroMovieWidget:SetFullscreen("Letterbox")
  _oIntroMovieWidget:SetIgnoresPause(true)
  MrxGuiBase.AddWidget(_oIntroMovieWidget)
  if Sound.OverrideUserMusic then
    Sound.OverrideUserMusic()
  end
  _PlayMovie()
end

function Start()
  if Sound.RestoreUserMusic then
    Sound.RestoreUserMusic()
  end
  if Net.AutoClient() then
    MrxGuiShellBootstrap.LoadShell()
    Net.ConnectToServer()
  elseif Net.AutoLobby() then
    MrxGuiShellBootstrap.LoadShell()
    Net.EnterLobby()
  elseif Sys.AutoLoad() then
    if Net.AutoServer() then
      MrxGuiShellBootstrap.LoadShell()
      Net.StartServer(Net.GetHostName(), Sys.GetLevelName(), Sys.GetMasterScriptName())
    else
      MrxGuiShellBootstrap.LoadShell()
    end
  else
    MrxGuiShellBootstrap.EnterShell()
  end
end

function GetSaveDataVersion()
  return 3
end

function ResetSingleton()
  MrxSoundShellBootstrap.PreExitShell()
  Event.Create(Event.TimerRelative, {
    MrxSoundShellBootstrap.EXITSHELL_FADELENGTH + 0.05,
    true
  }, ShellExitComplete)
end

function ShellExitComplete()
  MrxSoundShellBootstrap.ExitShell()
  MrxGuiShellBootstrap.Reset()
  MrxGuiShellBootstrap.ExitShell()
  Pg.ResetSingletonDone()
end
