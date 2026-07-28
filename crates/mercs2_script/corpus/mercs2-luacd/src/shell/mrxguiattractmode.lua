import("MrxGui")
import("MrxGuiBase")
import("MrxSound")
_tMovies = false

function Init()
  _tMovies = {"attract"}
end

function HandleInit(oWidget)
  oWidget:SetUseImmortalEvents(true)
  oWidget.Open = _Open
  oWidget.Close = _Close
  oWidget:GetChildren()[1]:SetFullscreen(true)
  local oMovie = MrxGuiBase.MovieWidget:new()
  oMovie:SetTransient(false)
  oWidget:AddChild(oMovie)
  oMovie:SetFullscreen("Letterbox")
  oWidget.CustomData.oMovie = oMovie
  oWidget:SetEventHandler("GuiGameStateChange", HandleGameStateChangeEvent)
  oWidget:SetEventHandler("ControllerInput", HandleInput)
  oWidget.CustomData.bActive = true
  oWidget.CustomData.nMovieNum = 1
  oWidget:Close()
end

function HandleGameStateChangeEvent(oWidget, sStateName, sStateAction)
  if "Attract" == sStateName then
    if "Enter" == sStateAction then
      oWidget:Open()
    elseif "Exit" == sStateAction then
      oWidget:Close()
    end
  end
end

function HandleInput(oWidget, tEvent)
  if not oWidget.CustomData.bClosing then
    Sys.RequestGameState("Shell")
    oWidget.CustomData.bClosing = true
  end
end

function _Open(oWidget)
  if oWidget.CustomData.bActive then
    return
  end
  oWidget.CustomData.bActive = true
  oWidget:SetVisible(true)
  local tChildren = oWidget:GetChildren()
  for nIndex, oChild in pairs(tChildren) do
    MrxGuiBase.AddWidgetWithChildren(oChild)
  end
  if not _tMovies[oWidget.CustomData.nMovieNum] then
    oWidget.CustomData.nMovieNum = 1
  end
  local sMovieFile = _tMovies[oWidget.CustomData.nMovieNum]
  oWidget.CustomData.nMovieNum = oWidget.CustomData.nMovieNum + 1
  local oMovie = oWidget.CustomData.oMovie
  oMovie:SetMovie(sMovieFile)
  oMovie:SetEndCallback(Sys.RequestGameState, {"shell"})
  oMovie:Play()
  MrxGuiBase.GetControlFocus(oWidget)
  Debug.Printf("BLACKSCREEN - FadeFromColor called from MrxGuiAttractMode._Open")
  MrxGui.FadeFromColor(0)
  MrxSound.EnterAttractState()
end

function _Close(oWidget)
  if not oWidget.CustomData.bActive then
    return
  end
  oWidget.CustomData.bActive = false
  oWidget.CustomData.bClosing = false
  local oMovie = oWidget.CustomData.oMovie
  oMovie:Stop()
  oMovie:SetMovie(nil)
  oWidget:SetVisible(false)
  local tChildren = oWidget:GetChildren()
  for nIndex, oChild in pairs(tChildren) do
    MrxGuiBase.RemoveWidgetWithChildren(oChild)
  end
  MrxGuiBase.ReleaseControlFocus(oWidget)
  MrxSound.ExitAttractState()
end
