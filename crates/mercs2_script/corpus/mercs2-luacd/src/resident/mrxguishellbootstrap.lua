import("MrxGui")
import("MrxGuiBase")
oPrecacheModule = nil
oShellModule = nil
nPlayersSelected = 1
bNeedsReloading = false

function Init()
  MrxGuiBase.LoadGUIFile("MrxGuiLoadLayout", LoadMovieLayouts, {})
end

function LoadMovieLayouts()
  MrxGui._InitFadeFlash()
  MrxGuiBase.LoadGUIFile("MrxGuiAttractLayout")
  MrxGuiBase.LoadGUIFile("MrxGuiCinematicLayout")
end

function Reset()
  if oShellModule then
    MrxGuiBase.RemoveAllWidgetsInLayout(oShellModule)
    oShellModule = nil
  end
  MrxGui:CleanupFadeFlash()
end

function EnterPrecache()
  Debug.Printf("MrxGuiLTIPrecache.lua EnterPrecache")
  MrxGuiBase.LoadGUIFile("MrxGuiLTIPrecacheLayout", PrecacheScreenLoaded)
end

function PrecacheScreenLoaded(PrecacheScreenModule)
  Debug.Printf("MrxGuiLTIPrecache.lua PrecacheScreenLoaded")
  oPrecacheModule = PrecacheScreenModule
  local oWidget = MrxGuiBase.GetWidgetByName("LTI_precache")
  oWidget:SetVisible(true)
  oWidget.CustomData.oFlash:Restart()
  oWidget.CustomData.oFlash:Play()
  Sys.RequestGameState("LTI_precache")
end

function LoadPrecache()
  Debug.Printf("MrxGuiLTIPrecache.lua LoadPrecache!!!!!")
  MrxGuiBase.LoadGUIFile("MrxGuiLTIPrecacheLayout", ClosePrecacheOnLoad)
end

function ClosePrecacheOnLoad(PrecacheScreenModule)
  Debug.Printf("MrxGuiLTIPrecache.lua ClosePrecacheOnLoad@@@")
  oPrecacheModule = PrecacheScreenModule
  local oWidget = MrxGuiBase.GetWidgetByName("LTI_precache")
  oWidget:SetVisible(false)
  Debug.Printf("Calling EnterShell()")
  EnterShell()
end

function EnterShell()
  MrxGuiBase.LoadGUIFile("MrxGuiShellLayout", ShellScreenLoaded)
end

function ExitShell()
  if oShellModule then
    oShellModule:Close()
    oShellModule = nil
  end
  MrxGui:CleanupFadeFlash()
end

function ShellScreenLoaded(ShellScreenModule)
  Sys.RequestGameState("Shell")
  oShellModule = ShellScreenModule
end

function LoadShell()
  MrxGuiBase.LoadGUIFile("MrxGuiShellLayout", CloseShellOnLoad)
end

function CloseShellOnLoad(ShellScreenModule)
  oShellModule = ShellScreenModule
  local oWidget = MrxGuiBase.GetWidgetByName("Shell")
  oWidget:SetVisible(false)
  for nIndex in pairs(oWidget:GetChildren()) do
    oWidget:GetChildren()[nIndex]:SetEnabled(false)
  end
end

function SetUpSingleplayer()
  if "function" == type(fEnterSingleplayerCallbackFunction) then
    fEnterSingleplayerCallbackFunction(unpack(tEnterSingleplayerCallbackArguments))
  end
end

function SetUpMultiplayer()
  if "function" == type(fEnterMultiplayerCallbackFunction) then
    fEnterMultiplayerCallbackFunction(unpack(tEnterMultiplayerCallbackArguments))
  end
end

function ExitMultiplayer()
  if "function" == type(fExitMultiplayerCallbackFunction) then
    fExitMultiplayerCallbackFunction(unpack(tExitMultiplayerCallbackArguments))
  end
end

_sSelectedCharacter = false

function SetSelectedCharacter(sCharacter)
  _sSelectedCharacter = sCharacter
end

function GetSelectedCharacter()
  return _sSelectedCharacter or nil
end

function SetEnterSingleplayerCallback(fFunction, tArguments)
  fEnterSingleplayerCallbackFunction = fFunction
  tEnterSingleplayerCallbackArguments = tArguments
end

function SetExitSingleplayerCallback(fFunction, tArguments)
  fExitSingleplayerCallbackFunction = fFunction
  tExitSingleplayerCallbackArguments = tArguments
end

function SetEnterMultiplayerCallback(fFunction, tArguments)
  fEnterMultiplayerCallbackFunction = fFunction
  tEnterMultiplayerCallbackArguments = tArguments
end

function SetExitMultiplayerCallback(fFunction, tArguments)
  fExitMultiplayerCallbackFunction = fFunction
  tExitMultiplayerCallbackArguments = tArguments
end

fEnterSingleplayerCallbackFunction = nil
tEnterSingleplayerCallbackArguments = {}
fExitSingleplayerCallbackFunction = nil
tExitSingleplayerCallbackArguments = {}
fEnterMultiplayerCallbackFunction = nil
tEnterMultiplayerCallbackArguments = {}
fExitMultiplayerCallbackFunction = nil
tExitMultiplayerCallbackArguments = {}
