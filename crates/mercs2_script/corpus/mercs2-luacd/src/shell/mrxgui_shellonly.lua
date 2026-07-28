import("MrxGuiBase")
DisplayDialogBox = 0
DisplayNumericBox = 0
LoadGuiFile = 0
UnloadGuiFile = 0
LoadGUIFile = 0
RemoveAllWidgets = 0
RemoveAllWidgetsInLayout = 0
DeleteTransientWidgets = 0
ReAddAllWidgets = 0
HideAllWidgets = 0
ShowAllWidgets = 0
SetAllWidgetsSleep = 0
AssignLayoutToPlayer = 0
DuplicateLayout = 0
PushAllTextToFront = 0
AddWidget = 0
AddWidgetWithChildren = 0
RemoveWidget = 0
RemoveWidgetWithChildren = 0
RemoveEverySingleWidget = 0
PushWidgetToFront = 0
PushWidgetToBack = 0
GetWidgetByName = 0
GetAllWidgetsByName = 0
GetWidgetByNameAndOwner = 0
RemoveAllWidgetsInLayout = 0
Widget = 0
ImageWidget = 0
TextWidget = 0
FlashWidget = 0
SpriteWidget = 0
MovieWidget = 0
MinimapWidget = 0
_fObjectiveInformationCallback = false
_tObjectiveInformationCallbackData = false

function GetObjectiveDescription(uGuid)
  if not _fObjectiveInformationCallback then
    return nil
  end
  if "userdata" ~= type(uGuid) then
    return nil
  end
  local tData = {}
  if "table" == type(_tObjectiveInformationCallbackData) then
    for k, v in pairs(_tObjectiveInformationCallbackData) do
      tData[k] = v
    end
  end
  table.insert(tData, uGuid)
  return _fObjectiveInformationCallback(unpack(tData))
end

function SetObjectiveInformationCallback(fCallback, tCallbackData)
  if "function" == type(fCallback) then
    _fObjectiveInformationCallback = fCallback
    if "table" == type(tCallbackData) then
      _tObjectiveInformationCallbackData = tCallbackData
    else
      _tObjectiveInformationCallbackData = false
    end
  end
end

SendEvent = 0
_sFadeWidgetName = "Fullscreen Fade Effect Widget"
_oGlobalScreenFadeWidget = false
_tGlobalFadeStack = false

function FadeToColor(nTime, uPlayerGuid, nRed, nGreen, nBlue, nAlpha)
  if "number" ~= type(nTime) then
    nTime = 1
  end
  if "userdata" ~= type(uPlayerGuid) then
    uPlayerGuid = nil
  end
  if "number" ~= type(nRed) then
    nRed = 0
  end
  if "number" ~= type(nGreen) then
    nGreen = 0
  end
  if "number" ~= type(nBlue) then
    nBlue = 0
  end
  if "number" ~= type(nAlpha) then
    nAlpha = 255
  end
  local oScreenWidget
  if uPlayerGuid then
    oScreenWidget = GetWidgetByNameAndOwner(_sFadeWidgetName, uPlayerGuid)
  else
    oScreenWidget = _oGlobalScreenFadeWidget
  end
  if not oScreenWidget then
    oScreenWidget = ImageWidget:new()
    oScreenWidget:SetLocation(0, 0, 640, 480)
    oScreenWidget:SetName(_sFadeWidgetName)
    oScreenWidget:SetFullscreen(true)
    oScreenWidget:SetTransient(false)
    oScreenWidget.CustomData.nFadeToTransparentPoint = oScreenWidget:AddAnimationPoint({TranslucencyLevel = 0})
    oScreenWidget.CustomData.nFadeFromTransparentPoint = oScreenWidget:AddAnimationPoint({TranslucencyLevel = nAlpha})
    if not uPlayerGuid then
      _oGlobalScreenFadeWidget = oScreenWidget
    else
      oScreenWidget:SetOwner(uPlayerGuid)
    end
    AddWidget(oScreenWidget)
  end
  if uPlayerGuid or 0 == #_tGlobalFadeStack then
    oScreenWidget:SetColor(nRed, nGreen, nBlue)
    oScreenWidget:SetTranslucency(0)
    oScreenWidget:SetVisible(true)
    if 0 < nTime then
      oScreenWidget:SetAnimationPoint(oScreenWidget.CustomData.nFadeFromTransparentPoint, {TranslucencyLevel = nAlpha})
      oScreenWidget:AnimateToPoint(oScreenWidget.CustomData.nFadeFromTransparentPoint, nTime, true, nil)
    else
      oScreenWidget:SetTranslucency(nAlpha)
      oScreenWidget:SetAnimationPoint(oScreenWidget.CustomData.nFadeFromTransparentPoint, {TranslucencyLevel = nAlpha})
      oScreenWidget:AnimateToPoint(oScreenWidget.CustomData.nFadeFromTransparentPoint, 0, true)
    end
  end
  if not uPlayerGuid then
    local tFadeData = {
      nTime = nTime,
      nRed = nRed,
      nGreen = nGreen,
      nBlue = nBlue,
      nAlpha = nAlpha
    }
    table.insert(_tGlobalFadeStack, tFadeData)
  end
end

function FadeFromColor(nTime, uPlayerGuid)
  local oScreenWidget
  if "userdata" == type(uPlayerGuid) then
    oScreenWidget = GetWidgetByNameAndOwner(_sFadeWidgetName, uPlayerGuid)
  else
    oScreenWidget = _oGlobalScreenFadeWidget
    if #_tGlobalFadeStack > 0 then
      table.remove(_tGlobalFadeStack, 1)
    end
  end
  if not oScreenWidget then
    return
  end
  if "number" ~= type(nTime) then
    nTime = 1
  end
  if uPlayerGuid or 0 == #_tGlobalFadeStack then
    if 0 < nTime then
      oScreenWidget:AnimateToPoint(oScreenWidget.CustomData.nFadeToTransparentPoint, nTime, false, _HideWhenDone)
    else
      oScreenWidget:AnimateToPoint(oScreenWidget.CustomData.nFadeToTransparentPoint, nTime, 0)
      oScreenWidget:SetTranslucency(0)
      oScreenWidget:SetVisible(false)
    end
  elseif not uPlayerGuid then
    local tData = _tGlobalFadeStack[1]
    oScreenWidget:SetVisible(true)
    if 0 < tData.nTime then
      oScreenWidget:SetAnimationPoint(oScreenWidget.CustomData.nFadeFromTransparentPoint, {
        RedLevel = tData.nRed,
        GreenLevel = tData.nGreen,
        BlueLevel = tData.nBlue,
        TranslucencyLevel = tData.nAlpha
      })
      oScreenWidget:AnimateToPoint(oScreenWidget.CustomData.nFadeFromTransparentPoint, tData.nTime, true, nil)
    else
      oScreenWidget:SetColor(tData.nRed, tData.nGreen, tData.nBlue)
      oScreenWidget:SetTranslucency(nAlpha)
      oScreenWidget:SetAnimationPoint(oScreenWidget.CustomData.nFadeFromTransparentPoint, {
        RedLevel = tData.nRed,
        GreenLevel = tData.nGreen,
        BlueLevel = tData.nBlue,
        TranslucencyLevel = tData.nAlpha
      })
      oScreenWidget:AnimateToPoint(oScreenWidget.CustomData.nFadeFromTransparentPoint, 0, true, nil)
    end
  end
end

function SetFadeEnabled(bEnable)
  if _oGlobalScreenFadeWidget and bEnable ~= _oGlobalScreenFadeWidget.BasicData.bEnabled then
    if not bEnable then
      MrxGuiBase.RemoveWidget(_oGlobalScreenFadeWidget)
    else
      MrxGuiBase.AddWidget(_oGlobalScreenFadeWidget)
    end
  end
end

function _HideWhenDone(oWidget)
  oWidget:SetVisible(false)
end

Joystick = 0

function AddMessage(tArgs)
  local sMessage = tArgs.sText or " "
  local nPriority = tArgs.iPriority or 5
  local nDisplayDuration = tArgs.nDuration or 2
  local nFadeDuration = tArgs.nFadeTime or 0.5
  local bClearBuffer = tArgs.bClear or nil
  local bAllowsAppends
  local sType = tArgs.sType or "sText"
  if tArgs.bExclusive then
    bAllowsAppends = false
  else
    bAllowsAppends = true
  end
  MessageBox:AddMessage(tArgs.sText, tArgs.iPriority, tArgs.nDuration, tArgs.nFadeTime, tArgs.bClear, tArgs.bExclusive)
end

function ClearMessages()
  MessageBox:ClearMessages()
end

_bE3HudModeOn = false

function SetE3HudMode(bOn)
  local tNewEvent = {}
  tNewEvent.EventType = "E3HudMode"
  tNewEvent.bOn = bOn
  SendEvent(tNewEvent)
  _bE3HudModeOn = bOn
end

function IsE3HudModeActive()
  return _bE3HudModeOn
end

_PushAllTextToFront = 0

function FindShellWidget()
  local oShell = GetWidgetByName("Shell")
  if oShell and oShell.CustomData.oFlash then
    return oShell.CustomData.oFlash.BasicData.uId
  end
  return nil
end

function GetReticleSize(uPlayer)
  local oReticle = GetWidgetByName("reticle image")
  if oReticle then
    local nX1, nY1, nX2, nY2 = oReticle:GetLocation()
    return nX2 - nX1
  end
  return 48
end

function Init()
  DisplayDialogBox = MrxGuiDialogBox.DisplayDialogBox
  DisplayNumericBox = MrxGuiNumericBox.DisplayNumericBox
  LoadGuiFile = MrxGuiBase.LoadGUIFile
  UnloadGuiFile = MrxGuiBase.UnloadGUIFile
  LoadGUIFile = MrxGuiBase.LoadGUIFile
  RemoveAllWidgets = MrxGuiBase.RemoveAllWidgetsInLayout
  RemoveAllWidgetsInLayout = MrxGuiBase.RemoveAllWidgetsInLayout
  DeleteTransientWidgets = MrxGuiBase.DeleteTransientWidgets
  ReAddAllWidgets = MrxGuiBase.ReAddAllWidgets
  HideAllWidgets = MrxGuiBase.HideAllWidgets
  ShowAllWidgets = MrxGuiBase.ShowAllWidgets
  SetAllWidgetsSleep = MrxGuiBase.SetAllWidgetsSleep
  AssignLayoutToPlayer = MrxGuiBase.AssignLayoutToPlayer
  DuplicateLayout = MrxGuiBase.DuplicateLayout
  PushAllTextToFront = MrxGuiBase.PushAllTextToFront
  AddWidget = MrxGuiBase.AddWidget
  AddWidgetWithChildren = MrxGuiBase.AddWidgetWithChildren
  RemoveWidget = MrxGuiBase.RemoveWidget
  RemoveWidgetWithChildren = MrxGuiBase.RemoveWidgetWithChildren
  RemoveEverySingleWidget = MrxGuiBase.WidgetManager.RemoveAll
  PushWidgetToFront = MrxGuiBase.PushWidgetToFront
  PushWidgetToBack = MrxGuiBase.PushWidgetToBack
  GetWidgetByName = MrxGuiBase.GetWidgetByName
  GetAllWidgetsByName = MrxGuiBase.GetAllWidgetsByName
  GetWidgetByNameAndOwner = MrxGuiBase.GetWidgetByNameAndOwner
  Widget = MrxGuiBase.Widget
  ImageWidget = MrxGuiBase.ImageWidget
  TextWidget = MrxGuiBase.TextWidget
  FlashWidget = MrxGuiBase.FlashWidget
  SpriteWidget = MrxGuiBase.SpriteWidget
  MovieWidget = MrxGuiBase.MovieWidget
  MinimapWidget = MrxGuiBase.MinimapWidget
  SendEvent = MrxGuiBase.SentEvent
  Joystick = MrxGuiBase.Joystick
  _PushAllTextToFront = MrxGuiBase.PushAllTextToFront
  _tGlobalFadeStack = {}
  if Sys.IsDemoMode and Sys.IsDemoMode() then
    SetE3HudMode(true)
  end
end
