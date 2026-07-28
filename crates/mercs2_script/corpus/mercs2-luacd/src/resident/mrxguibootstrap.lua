import("MrxGuiBase")
import("MrxGui")
import("MrxGuiManager")
import("MrxUtil")
import("MrxGuiShellBootstrap")
import("MrxGuiInterface")

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
  MrxGuiManager.ToggleHud(uGuid, bVisible, sContext)
end

function CreatePlayerHud(uPlayerGuid)
  MrxGuiManager.CreateGui(uPlayerGuid)
end

function DeleteHud(uPlayerGuid)
  MrxGuiManager.DeleteGui(uPlayerGuid)
end

function DeleteAllHuds()
  MrxGuiManager.DeleteAddGuis()
end

function GetNumberOfPlayersFromShellSelection()
  return MrxGuiShellBootstrap.nPlayersSelected
end

function SetSatelliteOverlay(uPlayer, bOn, sFaction)
  MrxGuiManager.ToggleSatellite(uPlayer, bOn, sFaction)
end

function SetOnGuiLoadedFunc(fFunc, tArgs)
  MrxGuiManager.SetLoadingCompleteCallback(fFunc, tArgs)
end
