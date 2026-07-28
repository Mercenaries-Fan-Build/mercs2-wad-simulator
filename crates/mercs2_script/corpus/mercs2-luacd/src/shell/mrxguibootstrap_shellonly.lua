import("MrxGuiBase")
import("MrxGui")
import("MrxGuiShellBootstrap")

function Init()
  MrxGuiBase.LoadGUIFile("MrxGuiPauseLayout", _PauseScreenLoaded)
  MrxGuiShellBootstrap.SetExitMultiplayerCallback(ExitMultiplayer, {})
end

function Deinit()
end

function _PauseScreenLoaded(PauseScreenModule)
  PauseScreenModule.MrxGuiPauseScreen.ClosePauseScreen(PauseScreenModule.AddedWidgetList[1])
  oPauseModule = PauseScreenModule
end

function ToggleHud(uGuid, bVisible, sContext)
end

function CreatePlayerHud(uPlayerGuid)
end

function DeleteHud(uPlayerGuid)
end

function DeleteAllHuds()
end

function GetNumberOfPlayersFromShellSelection()
  return MrxGuiShellBootstrap.nPlayersSelected
end

function SetSatelliteOverlay(uPlayer, bOn, sFaction)
end

function SetOnGuiLoadedFunc(fFunc, tArgs)
end
