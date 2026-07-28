import("LevelBootstrap")
import("MrxSoundBootstrap")
import("MrxGuiShellBootstrap")
import("MrxGuiBase")
tMovies = {
  {"Pandemic", -1},
  {"EA", -1}
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
    Start()
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

function Init()
  Sys.SetLuaSaveVersion(GetSaveDataVersion())
  Graphics.SetGamma(0, 0.8, 1)
  if Sys.FinishedShell and Sys.FinishedShell() then
    Debug.Printf("##@ GameBootstrap - bailing because finished shell")
    return
  end
  if not Sys.PlayIntroMovies() then
    Start()
    return
  end
  _oIntroMovieWidget = MrxGuiBase.MovieWidget:new()
  _oIntroMovieWidget:SetFullscreen("Letterbox")
  _oIntroMovieWidget:SetIgnoresPause(true)
  MrxGuiBase.AddWidget(_oIntroMovieWidget)
  _PlayMovie()
end

function Start()
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
      LevelBootstrap.LoadLevel(Sys.GetLevelName(), Sys.GetMasterScriptName())
    end
  else
    MrxGuiShellBootstrap.EnterShell()
  end
end

function GetSaveDataVersion()
  return 3
end
