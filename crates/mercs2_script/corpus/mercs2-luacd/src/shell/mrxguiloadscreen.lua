import("MrxGui")
import("MrxGuiBase")
_gLoadFlashFile = "loadingscreen"

function HandleInit(oLoadScreen)
  oLoadScreen:SetUseImmortalEvents(true)
  oLoadScreen:SetFullscreen(true)
  oLoadScreen.CustomData.bActive = false
  oLoadScreen.CustomData.bFlashLoaded = false
  oLoadScreen.SetActive = _SetActive
  local tChildren = oLoadScreen:GetChildren()
  tChildren[1]:SetFullscreen(true)
  local oFlash = MrxGui.FlashWidget:new()
  oFlash:SetTransient(false)
  oFlash:SetFullscreen(true)
  oFlash:SetIgnoresPause(true)
  oLoadScreen.CustomData.oFlash = oFlash
  oLoadScreen:AddChild(oFlash)
  MrxGui.AddWidget(oFlash)
  oLoadScreen.nAnalogInputHeld = 0
  oLoadScreen:SetEventHandler("ControllerInput", HandleInput)
  oLoadScreen:SetVisible(false)
  InitSaveIcon()
end

function HandleStateChangeEvent(oLoadScreen, tData)
  oLoadScreen:SetActive(tData.bLoading)
end

function _SetActive(oLoadScreen, bActive)
  if not bActive and oLoadScreen.CustomData.bActive then
    if oLoadScreen.CustomData.bFlashLoaded then
      oLoadScreen.CustomData.oFlash:CallActionScriptCallback("closeLoadingScreen", {})
    end
    oLoadScreen.CustomData.bActive = false
    oLoadScreen:SetVisible(false)
    oLoadScreen.CustomData.oFlash:SetSwfFile(nil)
    oLoadScreen.CustomData.bFlashLoaded = false
    oLoadScreen.CustomData.bFlashLoading = false
    MrxGuiBase.ReleaseControlFocus(oLoadScreen)
  elseif bActive and not oLoadScreen.CustomData.bActive then
    oLoadScreen.CustomData.bActive = true
    oLoadScreen:SetVisible(true)
    if not oLoadScreen.CustomData.bFlashLoaded and not oLoadScreen.CustomData.bFlashLoading then
      oLoadScreen.CustomData.oFlash:SetSwfFile(_gLoadFlashFile, _CompleteFlashLoad, {oLoadScreen})
      oLoadScreen.CustomData.bFlashLoading = true
    end
  end
end

function _CompleteFlashLoad(oLoadScreen)
  if not oLoadScreen.CustomData.bActive then
    oLoadScreen.CustomData.oFlash:SetSwfFile(nil)
    oLoadScreen.CustomData.bFlashLoaded = false
  else
    oLoadScreen.CustomData.bFlashLoaded = true
    MrxGuiBase.GetControlFocus(oLoadScreen)
  end
  oLoadScreen.CustomData.bFlashLoading = false
end

function HandleInput(oLoadScreen, tInput)
  local oLoadFlash = oLoadScreen.CustomData.oFlash
  local bTesselationWasOn = 0 == oLoadScreen.nAnalogInputHeld
  for sKey, nValue in pairs(tInput) do
    if IsAnalog(tonumber(nValue)) then
      if string.find(sKey, "ButtonPress") then
        oLoadScreen.nAnalogInputHeld = oLoadScreen.nAnalogInputHeld + 1
      elseif string.find(sKey, "ButtonReleased") then
        oLoadScreen.nAnalogInputHeld = oLoadScreen.nAnalogInputHeld - 1
      end
    end
  end
  local bTesselationIsOn = 0 == oLoadScreen.nAnalogInputHeld
  if bTesselationWasOn ~= bTesselationIsOn and bTesselationIsOn then
    oLoadFlash:CallActionScriptCallback("leftAnalog", {0, 0})
  end
  if 0 > oLoadScreen.nAnalogInputHeld then
    oLoadScreen.nAnalogInputHeld = 0
  end
  oLoadFlash.EventHandlers.ControllerInput(oLoadFlash, tInput)
  if tInput.LeftAnalogX or tInput.LeftAnalogY then
    oLoadFlash:CallActionScriptCallback("leftAnalog", {
      tInput.LeftAnalogX or 0,
      tInput.LeftAnalogY or 0
    })
  else
    oLoadFlash:CallActionScriptCallback("leftAnalog", {0, 0})
  end
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

_knSaveIconSize = 64
_knSaveIconTime = 0.5

function InitSaveIcon()
  local oContainer = MrxGui.Widget:new()
  oContainer:SetLocation(64, 48, 264, 48 + _knSaveIconSize)
  oContainer:SetIgnoresPause(true)
  local oIcon = MrxGui.ImageWidget:new()
  oIcon:SetLocation(64, 48, 64 + _knSaveIconSize, 48 + _knSaveIconSize)
  local nX1, nY1, nX2, nY2 = oIcon:GetLocation()
  local nXCenter = (nX1 + nX2) * 0.5
  oIcon.CustomData.nOpenPoint = oIcon:AddAnimationPoint({x = nX1, x2 = nX2})
  oIcon.CustomData.nClosePoint = oIcon:AddAnimationPoint({x = nXCenter, x2 = nXCenter})
  oIcon.CustomData.bReversed = false
  oIcon:SetTexture("global_loading_skull")
  oIcon:SetIgnoresPause(true)
  oContainer:AddChild(oIcon)
  local oText = MrxGui.TextWidget:new()
  oText:SetLocation(64 + _knSaveIconSize + 0, 68)
  oText:SetFont("english_18")
  oText:SetText("[SHELL.LoadSave.Saving]")
  oContainer:AddChild(oText)
  oContainer:SetEventHandler("ShowSaveIcon", HandleSaveIconShow)
  oContainer:SetEventHandler("HideSaveIcon", HandleSaveIconHide)
  oContainer:SetVisible(false)
  oContainer.CustomData.oIcon = oIcon
  oContainer.CustomData.oText = oText
  MrxGui.AddWidget(oContainer)
  MrxGui.AddWidget(oIcon)
  MrxGui.AddWidget(oText)
end

function _SaveIconAnimationComplete(oIcon)
  if oIcon.CustomData.bReversed then
    oIcon:SetTextureCoordinates(0, 0, 1, 1)
  else
    oIcon:SetTextureCoordinates(1, 0, 0, 1)
  end
  oIcon.CustomData.bReversed = not oIcon.CustomData.bReversed
  oIcon:AnimateToPoint(oIcon.CustomData.nOpenPoint, _knSaveIconTime, true)
  oIcon:AnimateToPoint(oIcon.CustomData.nClosePoint, _knSaveIconTime, false, _SaveIconAnimationComplete, {})
end

function HandleSaveIconShow(oContainer, tEvent)
  oContainer:SetVisible(true)
  local oIcon = oContainer.CustomData.oIcon
  local oText = oContainer.CustomData.oText
  MrxGui.PushWidgetToFront(oIcon)
  MrxGui.PushWidgetToFront(oText)
  oIcon:AnimateToPoint(oIcon.CustomData.nClosePoint, _knSaveIconTime, true, _SaveIconAnimationComplete, {})
end

function HandleSaveIconHide(oContainer, tEvent)
  oContainer:SetVisible(false)
  local oIcon = oContainer.CustomData.oIcon
  oIcon:AnimateToPoint(oIcon.CustomData.nOpenPoint, 0, true)
  oIcon.CustomData.bReversed = false
  oIcon:SetTextureCoordinates(0, 0, 1, 1)
end
