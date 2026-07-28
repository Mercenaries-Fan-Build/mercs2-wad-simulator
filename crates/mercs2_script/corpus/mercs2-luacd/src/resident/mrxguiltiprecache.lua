import("MrxGuiBase")
import("MrxGuiManager")
import("MrxGuiDialogBox")
tArgument = nil

function OpenPrecache(oPrecacheScreen)
  if not oPrecacheScreen.CustomData.bHaveFlash then
    Debug.Printf("OpenPrecache(): bHaveFlash is false, not doing anything!")
    return
  end
  local oWidget = MrxGuiBase.GetWidgetByName("LTI_precache")
  local oFlash = oPrecacheScreen.CustomData.oFlash
  Debug.Printf("OpenPrecache(): Resetting flash and showing precache screen.")
  oFlash:Restart()
  oFlash:Play()
  oPrecacheScreen.CustomData.bActive = true
  oPrecacheScreen:SetVisible(true)
  oWidget:Play()
end

function ClosePrecacheScreen(oPrecacheScreen)
  Debug.Printf("MrxGuiLTIPrecache.lua ClosePrecacheScreen!!!")
  oPrecacheScreen.CustomData.bActive = false
  MrxGuiBase.ReleaseControlFocus(oPrecacheScreen)
  if oPrecacheScreen.CustomData.bHaveFlash then
    oPrecacheScreen.CustomData.oFlash:Pause()
  end
  oPrecacheScreen:SetVisible(false)
  local tChildren = oPrecacheScreen:GetChildren()
  for nIndex, oChild in pairs(tChildren) do
    MrxGuiBase.RemoveWidgetWithChildren(oChild)
  end
end

function HandleStateChangeEvent(oWidget, sStateName, sStateAction)
  Debug.Printf("MrxGuiLTIPrecache.lua HandleStateChangeEvent: " .. tostring(sStateName) .. ", " .. tostring(sStateAction))
  if not sStateName or not sStateAction then
    return
  end
  if "Precache" ~= sStateName then
    return
  end
  if "Exit" == sStateAction then
    ClosePrecacheScreen(oWidget)
  end
end

function HandleInitializationEvent(oWidget, tUnused)
  Debug.Printf("MrxGuiLTIPrecache.lua HandleInitializationEvent")
  oWidget:SetFullscreen("pan and scan")
  ClosePauseScreen(oWidget)
  oWidget.CustomData.tHudStates = {}
end

function _Initialize(oPrecacheScreen)
  Debug.Printf("MrxGuiLTIPrecache.lua _Initialize")
  oPrecacheScreen.CustomData.bActive = true
  local oFlash = MrxGuiBase.FlashWidget:new()
  oFlash:SetOwner(oPrecacheScreen:GetOwner())
  oPrecacheScreen.CustomData.oFlash = oFlash
  oFlash:SetFullscreen(true)
  oFlash:SetLocation(0, 0, 640, 480)
  oFlash:SetAnchoring("center", "center")
  oPrecacheScreen:AddChild(oFlash)
  oFlash:SetFlashEventHandler("precacheDone", _LTIPrecacheDone, {})
  oFlash.CustomData.oParent = oPrecacheScreen
  oFlash.oParentWidget = oPrecacheScreen
  oFlash.nAnalogInputHeld = 0
  oPrecacheScreen.Open = OpenPrecacheScreen
  oPrecacheScreen.Close = ClosePrecacheScreen
  oPrecacheScreen:SetEventHandler("ControllerInput", _HandleInput)
  oPrecacheScreen.CustomData.bLoading = true
  oFlash.CustomData.sFile = "LTI_precache"
  oFlash:SetSwfFile(oFlash.CustomData.sFile, _FinishLoad, {oFlash})
  MrxGuiBase.AddWidget(oFlash)
  oFlash.CustomData.bHasFile = true
  oFlash.CustomData.bLoaded = true
  OpenPrecache(oPrecacheScreen)
  Debug.Printf("MrxGuiLTIPrecache.lua _Initialize Done")
end

function _FinishLoad(oPrecacheScreen)
  Debug.Printf("MrxGuiLTIPrecache.lua _FinishLoad")
  oPrecacheScreen.CustomData.bHaveFlash = true
  _LTIPrecacheSmokeDone2()
end

function _HandleToggleEvent(oPrecacheScreen, tUnused)
  Debug.Printf("MrxGuiLTIPrecache.lua HandleToggleEvent!!!")
  if oPrecacheScreen.CustomData.bActive then
    oPrecacheScreen:Close()
  else
    oPrecacheScreen:Open()
  end
end

function _HandleCloseEvent(oFlash)
  Debug.Printf("MrxGuiLTIPrecache.lua HandleCloseEvent!!!")
  if oPrecacheScreen.CustomData.bActive then
    oPrecacheScreen:Close()
  end
end

function _HandleInput(oPrecacheMenu, tInput)
  local oMap = oPrecacheMenu.CustomData.oFlash
  oMap.EventHandlers.ControllerInput(oMap, tInput)
end

function IsAnalog(nValue)
  if not nValue then
    return false
  end
  if nValue >= MrxGuiBase.Joystick.BUTTON_L_STICK_L and nValue <= MrxGuiBase.Joystick.BUTTON_R_STICK_D then
    return true
  end
  return false
end

function _LTIPrecacheDone(oFlash, iNumber)
  Debug.Printf("MrxGuiLTIPrecache.lua _LTIPrecacheDone")
  LTILibName.LTIPrecacheDone()
end

function _LTIPrecacheDone2()
  Debug.Printf("MrxGuiLTIPrecache.lua _LTIPrecacheDone2")
  LTILibName.LTIPrecacheDone()
end

function _LTIPrecacheSmokeDone(oFlash, iNumber)
  Debug.Printf("MrxGuiLTIPrecache.lua _LTIPrecacheSmokeDone")
  LTILibName.LTIPrecacheSmokeDone()
end

function _LTIPrecacheSmokeDone2()
  Debug.Printf("MrxGuiLTIPrecache.lua _LTIPrecacheSmokeDone2")
  LTILibName.LTIPrecacheSmokeDone()
end

function _LTIUpdateTo(oFlash, iNumber)
  Debug.Printf("MrxGuiLTIPrecache.lua UpdateTo")
  oPrecacheScreen.CustomData.oFlash:CallActionScriptCallback("updateTo", {iNumber})
end
