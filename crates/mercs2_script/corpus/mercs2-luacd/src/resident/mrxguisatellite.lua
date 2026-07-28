import("MrxGui")
import("MrxGuiBase")
import("MrxPmc")
import("MrxGuiManager")
import("MrxUtil")
_bUseMinigame = true
_nMinigameTime = 1
_nMinigameTimeIncrease = 1.2
_nMinigameMaxTime = 7
_nMoneyCost = 5000
_tDefaultSectorData = false

function Init()
  _tDefaultSectorData = {
    {-30, 30},
    {150, 210}
  }
end

function UseMinigame()
  return _bUseMinigame
end

function HandleSatelliteStateChangeEvent(oOverlay, tEvent)
  oOverlay:SetActivated(tEvent.bActivate, tEvent.bAdvanced, tEvent.bMinigame)
end

function SetActivated(oOverlay, bActivate, bAdvanced, bMinigame)
  if oOverlay.CustomData.bActivated == bActivate then
    return
  end
  local tChildren = oOverlay:GetChildren()
  if bActivate then
    oOverlay.CustomData.bMinigameOn = bMinigame
    oOverlay.CustomData.bTargettingSuccess = nil
    oOverlay.CustomData.oBackground:Show(true, 255)
    oOverlay:SetVisible(true)
    for nIndex, oChild in pairs(tChildren) do
      MrxGui.AddWidgetWithChildren(oChild)
    end
    oOverlay.CustomData.oFound:SetVisible(false)
    oOverlay.CustomData.oReadout:SetText(" ")
    oOverlay.CustomData.oCompass:SetRotation(Player.GetCameraXZHeading(oOverlay:GetOwner()))
    _OpenMinigame(oOverlay, _bUseMinigame)
    Sound.CueSound(0, "ui_SatDes_Turn_On")
    Sound.CueSound(0, "ui_SatDes_BG_Loop")
    Event.Post("Satellite Targetting Start", {
      uPlayer = oOverlay:GetOwner()
    })
    Player.SetScopeEnabled(oOverlay:GetOwner(), false)
    MrxGuiManager.ToggleHud(oOverlay:GetOwner(), false, "satellite")
    Event.Create(Event.TimerRelative, {0.1, true}, _ActivateStaticEffect, {oOverlay})
  else
    Sound.CueSound(0, "ui_SatDes_Turn_Off")
    Sound.StopSound(0, "ui_SatDes_BG_Loop")
    _Cleanup(oOverlay)
    Graphics.SetBoundaryEffect(0)
    MrxGuiManager.ToggleHud(oOverlay:GetOwner(), true, "satellite")
    Player.SetScopeEnabled(oOverlay:GetOwner(), true)
    if not oOverlay.CustomData.bTargettingSuccess then
      Event.Post("Satellite Targetting Cancelled", {
        uPlayer = oOverlay:GetOwner()
      })
    end
  end
  oOverlay.CustomData.bActivated = bActivate
end

function _ActivateStaticEffect(oOverlay)
  if oOverlay.CustomData.bActivated then
    Graphics.SetBoundaryEffect(0.25)
    local oTutorial = MrxGui.GetWidgetByNameAndOwner("tutorial", oOverlay:GetOwner())
    if oTutorial then
      oTutorial:PushToFront()
    end
  end
end

function _Cleanup(oOverlay)
  local tChildren = oOverlay:GetChildren()
  _CleanupMinigame(oOverlay, _bUseMinigame)
  oOverlay.CustomData.oBackground:Show(false, 255)
  oOverlay:SetVisible(false)
  for nIndex, oChild in pairs(tChildren) do
    MrxGui.RemoveWidgetWithChildren(oChild)
  end
end

function _HandleInput(oOverlay, tInput)
  if (tInput.ButtonPress == MrxGuiBase.Joystick.BUTTON_ALT2_1 or tInput.ButtonPress == MrxGuiBase.Joystick.BUTTON_PAD2_D) and oOverlay.CustomData.bMinigameOn and not oOverlay.CustomData.bExiting then
    MrxGuiBase.ReleaseControlFocus(oOverlay)
    BeginMinigame(oOverlay)
  end
end

function Initialize(oOverlay)
  local tChildren = oOverlay:GetChildren()
  local oBackground = tChildren[2]
  oBackground:SetFullscreen(true)
  oOverlay.CustomData.oBackground = oBackground
  oBackground.CustomData.oWipe = oBackground:GetChildren()[1]
  oBackground.CustomData.nWipeAlpha = oBackground.CustomData.oWipe:GetTranslucency()
  oBackground.Show = _ShowBackground
  local oReticle = tChildren[3]
  oOverlay.CustomData.oReticle = oReticle
  oReticle.CustomData.nHighPoint = oReticle:AddAnimationPoint({TranslucencyLevel = 256})
  oReticle.CustomData.nLowPoint = oReticle:AddAnimationPoint({TranslucencyLevel = 64})
  LoopToHigh(oReticle, 0.75)
  local oReadout = tChildren[9]
  oOverlay.CustomData.oReadout = oReadout
  oReadout:SetText(" ")
  local oDesc = tChildren[11]
  oReadout.CustomData.oDesc = oDesc
  oOverlay.CustomData.oCompass = tChildren[4]
  _InitializeMinigame(oOverlay, _bUseMinigame)
  local oFound = tChildren[12]
  _GuiInternal.SetWidgetUseNewRescale(oFound.BasicData.uId, true)
  oFound.CustomData.nBigPoint = oFound:AddAnimationPoint({
    x = 20,
    y = -60,
    x2 = 620,
    y2 = 540,
    TranslucencyLevel = 32
  })
  oFound.CustomData.nSetPoint = oFound:AddAnimationPoint({
    x = 0,
    y = 0,
    x2 = 128,
    y2 = 128
  })
  oFound:SetVisible(false)
  oFound:AnimateToPoint(oFound.CustomData.oBigPoint, 0, true)
  oOverlay.CustomData.oFound = oFound
  oOverlay.CustomData.oHelpText = tChildren[13]
  oOverlay.SetHelpText = SetHelpText
  oOverlay.CustomData.bActivated = false
  oOverlay.SetActivated = SetActivated
  _Cleanup(oOverlay)
  oOverlay.SetSuccessCallback = SetSuccessCallback
  oOverlay:SetEventHandler("SetSatelliteBackground", HandleBackgroundMessage)
  oOverlay:SetEventHandler("ScanFoundGuid", HandleGuidFound)
end

function SetSuccessCallback(oOverlay, fCallback, tData)
  oOverlay.CustomData.fCallback = fCallback
  oOverlay.CustomData.tCallbackData = tData
end

function InitializeHelpField(oField)
  oField:Wrap()
end

function SetHelpText(oOverlay, sText)
  oOverlay.CustomData.oHelpText:SetText(sText)
end

function HandleGuidFound(oOverlay, tEvent)
  local oFound = oOverlay.CustomData.oFound
  oFound:SetVisible(true)
  local nX1, nY1, nX2, nY2 = Gui.FindGuiLocation(oOverlay:GetOwner(), tEvent.uGuid)
  if nX1 and nY1 and not nX2 and not nY2 then
    nX2 = nX1
    nY2 = nY1
  end
  local nX = (nX1 + nX2) * 0.5
  local nY = (nY1 + nY2) * 0.5
  nX1 = nX - 48
  nY1 = nY - 48
  nX2 = nX + 48
  nY2 = nY + 48
  oFound:SetAnimationPoint(oFound.CustomData.nSetPoint, {
    x = nX1,
    y = nY1,
    x2 = nX2,
    y2 = nY2,
    TranslucencyLevel = 255
  })
  oFound:AnimateToPoint(oFound.CustomData.nBigPoint, 0, true, _FinishFoundAnimation, {
    tEvent.fCallback,
    tEvent.tCallbackData
  })
end

function _FinishFoundAnimation(oFound, fCallback, tData)
  oFound:AnimateToPoint(oFound.CustomData.nSetPoint, 0.25, true, _FinishFoundAnimation2, {fCallback, tData})
end

function _FinishFoundAnimation2(oFound, fCallback, tData)
  oFound:AnimateToPoint(oFound.CustomData.nSetPoint, 0.5, true, _CallFoundCallback, {fCallback, tData})
end

function _CallFoundCallback(oFound, fCallback, tData)
  oFound:SetVisible(false)
  if "function" == type(fCallback) then
    tData = tData or {}
    fCallback(unpack(tData))
  end
end

function HandleReadoutUpdate(oReadout, tEvent)
  if not tEvent.sName then
    oReadout:SetText(" ")
    oReadout.CustomData.oDesc:SetText(" ")
  elseif oReadout.CustomData.sDisplayString ~= tEvent.sName then
    oReadout:SetText(tEvent.sName)
    oReadout.CustomData.sDisplayString = tEvent.sName
    if tEvent.uTargetGuid then
      local sDesc = MrxGui.GetObjectiveDescription(tEvent.uTargetGuid)
      oReadout.CustomData.oDesc:SetText(sDesc or " ")
    end
  end
end

function HandleFactionUpdate(oFaction, tEvent)
  if not tEvent.FactionTexture then
    oFaction:SetVisible(false)
  elseif oFaction.CustomData.PrevTexture ~= tEvent.FactionTexture then
    oFaction:SetTexture(tEvent.FactionTexture)
    oFaction:SetVisible(true)
    oFaction.CustomData.sDisplayString = tEvent.FactionTexture
  end
end

function HandleXUpdate(oMeter)
  local nX, nY, nZ = Pg.FindPointFromCamera(0, 0)
  oMeter.CustomData.oReadout:SetText(string.format("%4.2f", nX))
  nX = nX - math.floor(nX / 100) * 100
  nX = nX / 100 * 5
  oMeter:SetTextureCoordinates(nX, nil, nX + 10, nil)
end

function HandleZUpdate(oMeter)
  local nX, nY, nZ = Pg.FindPointFromCamera(0, 0)
  oMeter.CustomData.oReadout:SetText(string.format("%4.2f", nZ))
  nZ = nZ - math.floor(nZ / 100) * 100
  nZ = nZ / 100 * 4
  oMeter:SetTextureCoordinates(nil, nZ, nil, nZ + 8)
end

function InitMeter(oMeter)
  local tChildren = oMeter:GetChildren()
  oMeter.CustomData.oPointer = tChildren[1]
  oMeter.CustomData.oReadout = tChildren[2]
end

function WipeUpdate(oWipe, nDeltaTime)
  if not nDeltaTime then
    return
  end
  local nX, nY, nX2, nY2 = oWipe:GetLocation()
  if nY2 < 0 then
    oWipe:SetLocation(nX, 500)
  else
    oWipe:SetLocation(nX, nY - 125 * nDeltaTime)
  end
end

function LoopToHigh(oReticle, nTime)
  oReticle:AnimateToPoint(oReticle.CustomData.nHighPoint, nTime, true, LoopToLow, {nTime})
end

function LoopToLow(oReticle, nTime)
  oReticle:AnimateToPoint(oReticle.CustomData.nLowPoint, nTime, true, LoopToHigh, {nTime})
end

function HandleBackgroundMessage(oOverlay, tEvent)
  oOverlay.CustomData.oBackground:Show(tEvent.bActivate, tEvent.fAlpha)
end

function _ShowBackground(oBackground, bShow, fAlpha)
  fAlpha = fAlpha or 255
  if bShow then
    MrxGui.AddWidgetWithChildren(oBackground)
    oBackground:SetVisible(true)
    oBackground:SetTranslucency(fAlpha)
    oBackground.CustomData.oWipe:SetTranslucency(oBackground.CustomData.nWipeAlpha * (fAlpha / 255))
  else
    MrxGui.RemoveWidgetWithChildren(oBackground)
    oBackground:SetVisible(false)
    oBackground:SetTranslucency(255)
    oBackground.CustomData.oWipe:SetTranslucency(oBackground.CustomData.nWipeAlpha)
  end
end

nBaseBgR = 255
nBaseBgG = 255
nBaseBgB = 255
nSliceR = 172
nSliceG = 172
nSliceB = 172

function _InitializeMinigame(oOverlay, bUse)
  local oMinigame = oOverlay:GetChildren()[1]
  oOverlay.CustomData.oMinigame = oMinigame
  if bUse then
    local tChildren = oMinigame:GetChildren()
    local oCursor = tChildren[5]
    local oFeedback = tChildren[6]
    local oButton = tChildren[7]
    local oBg = tChildren[1]
    local oCost = oOverlay:GetChildren()[8]:GetChildren()[2]
    local oTime = oOverlay:GetChildren()[7]:GetChildren()[2]
    oCost.CustomData.oMinigame = oMinigame
    oTime.CustomData.oMinigame = oMinigame
    oMinigame.CustomData.oCursor = oCursor
    oMinigame.CustomData.oBg = oBg
    oMinigame.CustomData.oCost = oCost
    oMinigame.CustomData.oTime = oTime
    oMinigame.CustomData.oButton = oButton
    oMinigame.ParentWidget = oOverlay
    oCursor.CustomData.nEndPoint = oCursor:AddAnimationPoint({nRotation = 359, nRotationDirection = 1})
    oCursor.CustomData.nStartPoint = oCursor:AddAnimationPoint({nRotation = 0, nRotationDirection = -1})
    oOverlay.SetMinigameCallback = _SetMinigameCallback
    oOverlay.SetMinigameSectors = _SetMinigameSectors
    oOverlay.SetMinigameCost = _SetMinigameCost
    oMinigame.CustomData.tSectorWidgets = {}
    oMinigame.CustomData.tSectorWidgets[1] = tChildren[2]
    oMinigame.CustomData.tSectorWidgets[2] = tChildren[3]
    oMinigame.CustomData.tSectorWidgets[3] = tChildren[4]
    for nIndex, oSector in pairs(oMinigame.CustomData.tSectorWidgets) do
      oSector.CustomData.nBaseColor = oSector:AddAnimationPoint({
        RedLevel = nSliceR,
        GreenLevel = nSliceG,
        BlueLevel = nSliceB
      })
      oSector.CustomData.nLitColor = oSector:AddAnimationPoint({
        RedLevel = 255,
        GreenLevel = 255,
        BlueLevel = 255
      })
      oSector:SetColor(0, 0, 255)
      oSector:AnimateToPoint(oSector.CustomData.nBaseColor, 0, true)
      oSector:SetRotation(0)
    end
    _InitializeDefaultSectors(oMinigame)
    local nX1, nY1, nX2, nY2 = oBg:GetLocation()
    local nCenterX = (nX1 + nX2) * 0.5
    local nCenterY = (nY1 + nY2) * 0.5
    oMinigame.CustomData.nCenterX = nCenterX
    oMinigame.CustomData.nCenterY = nCenterY
    for nIndex, oChild in pairs(tChildren) do
      oChild.CustomData.nOpenPoint = oChild:AddAnimationPoint({
        x1 = nX1,
        y1 = nY1,
        x2 = nX2,
        y2 = nY2
      })
      oChild.CustomData.nClosePoint = oChild:AddAnimationPoint({
        x1 = nCenterX,
        y1 = nCenterY,
        x2 = nCenterX,
        y2 = nCenterY
      })
      oChild:SetLocation(nCenterX, nCenterY, nCenterX, nCenterY)
    end
    oBg.CustomData.nBaseColor = oBg:AddAnimationPoint({
      RedLevel = nBaseBgR,
      GreenLevel = nBaseBgG,
      BlueLevel = nBaseBgB
    })
    oBg.CustomData.nRedColor = oBg:AddAnimationPoint({
      RedLevel = 200,
      GreenLevel = 0,
      BlueLevel = 0
    })
    oBg.CustomData.nGreenColor = oBg:AddAnimationPoint({
      RedLevel = 0,
      GreenLevel = 255,
      BlueLevel = 0
    })
    oFeedback.CustomData.nBasePoint = oFeedback:AddAnimationPoint({TranslucencyLevel = 255})
    oFeedback.CustomData.nFadePoint = oFeedback:AddAnimationPoint({TranslucencyLevel = 0})
    oMinigame.CustomData.oFeedback = oFeedback
    if Sys.IsConfirmOnCircle and Sys.IsConfirmOnCircle() then
      oButton:SetTexture("icon_hijack_button_B")
    end
  end
end

function _OpenMinigame(oOverlay, bUse)
  local oMinigame = oOverlay.CustomData.oMinigame
  if bUse then
    oOverlay.CustomData.bExiting = false
    oMinigame:SetColor(255, 255, 255, 255)
    local oBg = oMinigame.CustomData.oBg
    oBg:AnimateToPoint(oBg.CustomData.nBaseColor, 0, true)
    oBg:SetColor(nBaseBgR, nBaseBgG, nBaseBgB, 255)
    local oCursor = oMinigame.CustomData.oCursor
    oCursor:SetColor(255, 255, 255, 255)
    oOverlay:SetEventHandler("ControllerInput", _HandleInput)
    MrxGuiBase.GetControlFocus(oOverlay, false)
    oMinigame.CustomData.nCost = 0
    oMinigame.CustomData.nCashPerSecond = oMinigame.CustomData.nCashPerSecond or _nMoneyCost
    oMinigame.CustomData.oCost:SetText(MrxUtil.FormatMoney(oMinigame.CustomData.nCost))
    oMinigame.CustomData.nTime = 0
    oMinigame.CustomData.oTime:SetText("00:00:00")
    oMinigame.CustomData.oCost:SetEventHandler("GuiUpdate", _CostUpdate)
    oMinigame.CustomData.oCost:SetVisible(true)
    oMinigame.CustomData.oTime:SetEventHandler("GuiUpdate", _TimeUpdate)
    oMinigame.CustomData.oTime:SetVisible(true)
    _CreateWidgetsFromSectorData(oMinigame)
    oMinigame:SetVisible(false)
    local tChildren = oMinigame:GetChildren()
    for nIndex, oChild in pairs(tChildren) do
      oChild:AnimateToPoint(oChild.CustomData.nClosePoint, 0, true)
    end
  else
    oMinigame:SetVisible(false)
  end
end

function _CostUpdate(oCost, nDeltaTime)
  local oMinigame = oCost.CustomData.oMinigame
  oMinigame.CustomData.nCost = oMinigame.CustomData.nCost + oMinigame.CustomData.nCashPerSecond * nDeltaTime
  oCost:SetText(MrxUtil.FormatMoney(oMinigame.CustomData.nCost))
end

function _TimeUpdate(oTime, nDeltaTime)
  local nTime = oTime.CustomData.oMinigame.CustomData.nTime + nDeltaTime
  oTime.CustomData.oMinigame.CustomData.nTime = nTime
  local sMinutes = Math.floor(nTime / 60)
  local sSeconds = Math.floor(nTime - sMinutes * 60)
  local sMseconds = Math.floor((nTime - Math.floor(nTime)) * 100)
  if sMinutes < 10 then
    sMinutes = "0" .. sMinutes
  end
  if sSeconds < 10 then
    sSeconds = "0" .. sSeconds
  end
  if sMseconds < 10 then
    sMseconds = "0" .. sMseconds
  end
  oTime:SetText(sMinutes .. ":" .. sSeconds .. ":" .. sMseconds)
end

function _CleanupMinigame(oOverlay, bUse)
  if bUse then
    MrxGuiBase.ReleaseControlFocus(oOverlay)
    local oMinigame = oOverlay.CustomData.oMinigame
    MrxGuiBase.ReleaseControlFocus(oMinigame)
    oOverlay.CustomData.oReticle:SetVisible(true)
    if "number" == type(oMinigame.CustomData.nCost) then
      MrxPmc.AddCashQty(oMinigame.CustomData.nCost * -1, nil, "[Generic.SupportDesignators.Satellite]")
    end
    local oCursor = oMinigame.CustomData.oCursor
    oCursor:AnimateToPoint(oCursor.CustomData.nStartPoint, 0, true)
    local oBg = oMinigame.CustomData.oBg
    oBg:AnimateToPoint(oBg.CustomData.nBaseColor, 0, true)
    for n, tData in pairs(oMinigame.CustomData.tSectorData) do
      local oSector = oMinigame.CustomData.tSectorWidgets[n]
      oSector:AnimateToPoint(oSector.CustomData.nBaseColor, 0, true)
    end
    oMinigame:SetVisible(false)
    oMinigame:SetEventHandler("GuiUpdate", nil)
    oMinigame.CustomData.oCost:SetEventHandler("GuiUpdate", nil)
    oMinigame.CustomData.oTime:SetEventHandler("GuiUpdate", nil)
    oMinigame.CustomData.nCashPerSecond = nil
    if oMinigame.CustomData.BeginEvent then
      Event.Delete(oMinigame.CustomData.BeginEvent)
      oMinigame.CustomData.BeginEvent = nil
    end
    _InitializeDefaultSectors(oMinigame)
  end
end

function _ResetMinigame(oMinigame)
  _ResetSectors(oMinigame)
  for nIndex, oChild in pairs(oMinigame:GetChildren()) do
    oChild:SetVisible(true)
  end
end

function BeginMinigame(oOverlay)
  local oMinigame = oOverlay.CustomData.oMinigame
  if oOverlay.CustomData.bExiting then
    return
  end
  oMinigame:SetVisible(true)
  oOverlay.CustomData.oReticle:SetVisible(false)
  oMinigame.CustomData.oFeedback:SetVisible(false)
  oMinigame.CustomData.oFeedback:SetTexture("icon_success")
  oMinigame.CustomData.oButton:SetVisible(false)
  oMinigame.CustomData.oButton:SetTranslucency(0)
  local oCursor = oMinigame.CustomData.oCursor
  oCursor:SetRotation(0)
  local tChildren = oMinigame:GetChildren()
  local nTime = 0.5
  for nIndex, oChild in pairs(tChildren) do
    oChild:AnimateToPoint(oChild.CustomData.nClosePoint, 0, true)
    oChild:AnimateToPoint(oChild.CustomData.nOpenPoint, nTime, false)
  end
  _ResetSectors(oMinigame)
  for n, oSector in pairs(oMinigame.CustomData.tSectorWidgets) do
    if oSector.CustomData.bInUse then
      MrxGuiBase.PushWidgetToBack(oSector)
    end
  end
  MrxGui.PushWidgetToBack(oMinigame.CustomData.oBg)
  MrxGui.PushWidgetToFront(oMinigame.CustomData.oFeedback)
  oMinigame.CustomData.BeginEvent = Event.Create(Event.TimerRelative, {nTime}, _FinishBeginMinigame, {oMinigame})
  Sound.CueSound(0, "ui_SatDes_Circular_PopUp")
end

function _FinishBeginMinigame(oMinigame)
  local tChildren = oMinigame:GetChildren()
  for nIndex, oChild in pairs(tChildren) do
    oChild:AnimateToPoint(oChild.CustomData.nOpenPoint, 0, true)
  end
  oMinigame.CustomData.BeginEvent = nil
  oMinigame:SetVisible(true)
  oMinigame.CustomData.oFeedback:SetVisible(false)
  oMinigame:SetEventHandler("ControllerInput", _HandleMinigameInput)
  MrxGuiBase.GetControlFocus(oMinigame, false)
  local oCursor = oMinigame.CustomData.oCursor
  oCursor:SetRotation(0)
  oMinigame.CustomData.nTolerance = 360 / _nMinigameTime * 0.1
  oCursor:AnimateToPoint(oCursor.CustomData.nEndPoint, _nMinigameTime, false, _MinigameCycleEnd, {oMinigame, _nMinigameTime})
  local nX, nY, nZ = Player.GetTargetUnderReticle(oMinigame:GetOwner())
  oMinigame.CustomData.nStartX = nX
  oMinigame.CustomData.nStartZ = nZ
  oMinigame:SetEventHandler("GuiUpdate", _HandleMinigameUpdate)
  Event.Post("Satellite Minigame Start", {
    uPlayer = oMinigame:GetOwner()
  })
end

function _MinigameCycleEnd(oCursor, oMinigame, nTime)
  oMinigame.CustomData.oCursor:SetRotation(0)
  nTime = math.min(nTime * _nMinigameTimeIncrease, _nMinigameMaxTime)
  oMinigame.CustomData.nTolerance = 360 / nTime * 0.1
  oCursor:AnimateToPoint(oCursor.CustomData.nEndPoint, nTime, true, _MinigameCycleEnd, {oMinigame, nTime})
end

function _HandleMinigameUpdate(oMinigame, fDeltaTime)
  local nRotation = oMinigame.CustomData.oCursor:GetRotation()
  local nTime = 0.2
  local nSectorId = _CollideWithSectors(oMinigame, nRotation, 0) or -1
  oMinigame.CustomData.nPreviousSectorId = oMinigame.CustomData.nPreviousSectorId or -1
  local nPreviousSectorId = oMinigame.CustomData.nPreviousSectorId
  if nPreviousSectorId ~= nSectorId and 0 < nSectorId then
    Sound.CueSound(0, "ui_SatDes_Circular_Beep")
  end
  oMinigame.CustomData.nPreviousSectorId = nSectorId
  for nId, oSector in pairs(oMinigame.CustomData.tSectorWidgets) do
    if nSectorId == nId then
      oSector:SetColor(255, 255, 255)
    else
      oSector:SetColor(nSliceR, nSliceG, nSliceB)
    end
  end
  local oButton = oMinigame.CustomData.oButton
  local nTheta, nRadius
  if 0 < nSectorId then
    local tSectorData = oMinigame.CustomData.tSectorData[nSectorId]
    nTheta = (tSectorData[1] + tSectorData[2]) / 2
    nRadius = 80
  end
  if Math.PolarToRect and nTheta and nRadius then
    local nX, nY = Math.PolarToRect(-nTheta + 90, nRadius)
    nX = nX + oMinigame.CustomData.nCenterX
    nY = -nY + oMinigame.CustomData.nCenterY
    oButton:SetLocation(nX - 16, nY - 16, nX + 16, nY + 16)
    oButton:SetVisible(true)
    oButton:SetTranslucency(255)
  else
    oButton:SetVisible(false)
    oButton:SetTranslucency(0)
  end
  local nX, nY, nZ = Player.GetTargetUnderReticle(oMinigame:GetOwner())
  local nStartX = oMinigame.CustomData.nStartX
  local nStartZ = oMinigame.CustomData.nStartZ
  if 25 < (nX - nStartX) * (nX - nStartX) + (nZ - nStartZ) * (nZ - nStartZ) then
    MrxGuiBase.ReleaseControlFocus(oMinigame)
    local oCursor = oMinigame.CustomData.oCursor
    oCursor:SetRotation(0)
    local oBg = oMinigame.CustomData.oBg
    oBg:SetColor(nBaseBgR, nBaseBgG, nBaseBgB)
    _ResetSectors(oMinigame)
    oMinigame:SetVisible(false)
    oMinigame:SetEventHandler("GuiUpdate", nil)
    local tChildren = oMinigame:GetChildren()
    for nIndex, oChild in pairs(tChildren) do
      oChild:AnimateToPoint(oChild.CustomData.nClosePoint, 0, true)
    end
    local oOverlay = oMinigame.ParentWidget
    oOverlay:SetEventHandler("ControllerInput", _HandleInput)
    MrxGuiBase.GetControlFocus(oOverlay, false)
    oOverlay.CustomData.oReticle:SetVisible(true)
  end
end

function _CompleteMinigame(oMinigame)
  local tChildren = oMinigame:GetChildren()
  local nTime = 0.2
  tChildren[1]:AnimateToPoint(tChildren[1].CustomData.nClosePoint, nTime, true, _FinishCompleteMinigame, {oMinigame})
  local oCursor = oMinigame.CustomData.oCursor
  oCursor:SetVisible(false)
  oCursor:AnimateToPoint(oCursor.CustomData.nStartPoint, 0, true)
  oMinigame.ParentWidget.CustomData.bExiting = true
  Event.Post("Satellite Targetting Success", {
    uPlayer = oMinigame:GetOwner()
  })
  oMinigame.ParentWidget.CustomData.bTargettingSuccess = true
end

function _FinishCompleteMinigame(oUnused, oMinigame)
  MrxGuiBase.ReleaseControlFocus(oMinigame)
  MrxGuiBase.ReleaseControlFocus(oMinigame.ParentWidget)
  Player.RequestPDAMapModeExit(oMinigame:GetOwner(), _RemoveSatelliteTargettingMode, {oMinigame})
end

function _RemoveSatelliteTargettingMode(oMinigame)
  if oMinigame.CustomData.fCallback and oMinigame.CustomData.tData then
    local nX, nY, nZ, uGuid = Player.GetTargetUnderReticle(oMinigame:GetOwner())
    table.insert(oMinigame.CustomData.tData, uGuid)
    table.insert(oMinigame.CustomData.tData, nX)
    table.insert(oMinigame.CustomData.tData, nY)
    table.insert(oMinigame.CustomData.tData, nZ)
    table.insert(oMinigame.CustomData.tData, 1)
    table.insert(oMinigame.CustomData.tData, uGuid)
    oMinigame.CustomData.fCallback(unpack(oMinigame.CustomData.tData))
    oMinigame.CustomData.fCallback = nil
    oMinigame.CustomData.tData = nil
  end
end

function _HandleMinigameInput(oMinigame, tInput)
  if oMinigame.ParentWidget.CustomData.bExiting then
    return
  end
  if tInput.ButtonPress == MrxGuiBase.Joystick.BUTTON_ALT2_1 or tInput.ButtonPress == MrxGuiBase.Joystick.BUTTON_PAD2_D then
    local oCursor = oMinigame.CustomData.oCursor
    local nRotation = oCursor:GetRotation()
    local nHitSector = _CollideWithSectors(oMinigame, nRotation, oMinigame.CustomData.nTolerance)
    if nHitSector then
      oMinigame.CustomData.tSectorData[nHitSector].bHit = true
      oMinigame.CustomData.tSectorWidgets[nHitSector]:SetVisible(false)
      Sound.CueSound(0, "ui_SatDes_Circular_Timing_Correct")
      Event.Post("Satellite Minigame Sector Hit", {
        uPlayer = oMinigame:GetOwner()
      })
      local oBg = oMinigame.CustomData.oBg
      oBg:AnimateToPoint(oBg.CustomData.nGreenColor, 0, true)
      oBg:AnimateToPoint(oBg.CustomData.nBaseColor, 0.2, false)
      local oFeedback = oMinigame.CustomData.oFeedback
      oFeedback:SetTexture("icon_success")
      oFeedback:SetLocation(oMinigame.CustomData.nCenterX - 64, oMinigame.CustomData.nCenterY - 64, oMinigame.CustomData.nCenterX + 64, oMinigame.CustomData.nCenterY + 64)
      oFeedback:SetVisible(true)
      oFeedback:AnimateToPoint(oFeedback.CustomData.nBasePoint, 0, true)
      oFeedback:AnimateToPoint(oFeedback.CustomData.nFadePoint, 0.5, false, oFeedback.SetVisible, {false})
    else
      _ResetMinigame(oMinigame)
      local oBg = oMinigame.CustomData.oBg
      oBg:AnimateToPoint(oBg.CustomData.nRedColor, 0, true)
      oBg:AnimateToPoint(oBg.CustomData.nBaseColor, 0.2, false)
      Sound.CueSound(0, "ui_SatDes_Circular_Timing_Fail")
      Event.Post("Satellite Minigame Sector Miss", {
        uPlayer = oMinigame:GetOwner()
      })
      local oFeedback = oMinigame.CustomData.oFeedback
      oFeedback:SetTexture("icon_fail")
      oFeedback:SetLocation(oMinigame.CustomData.nCenterX - 64, oMinigame.CustomData.nCenterY - 64, oMinigame.CustomData.nCenterX + 64, oMinigame.CustomData.nCenterY + 64)
      oFeedback:SetVisible(true)
      oFeedback:AnimateToPoint(oFeedback.CustomData.nBasePoint, 0, true)
      oFeedback:AnimateToPoint(oFeedback.CustomData.nFadePoint, 0.5, false, oFeedback.SetVisible, {false})
    end
    if _HaveAllSectorsBeenHit(oMinigame) then
      _CompleteMinigame(oMinigame)
    end
  end
end

function _SetMinigameCallback(oOverlay, fCallback, tData)
  local oMinigame = oOverlay.CustomData.oMinigame
  oMinigame.CustomData.fCallback = fCallback
  oMinigame.CustomData.tData = tData
end

function _SetMinigameSectors(oOverlay, tSectors)
  SetSectorData(oOverlay.CustomData.oMinigame, tSectors)
end

function _SetMinigameCost(oOverlay, nCost)
  oOverlay.CustomData.oMinigame.CustomData.nCashPerSecond = nCost or _nMoneyCost
end

function SetSectorData(oMinigame, tData)
  for nIndex, tData in pairs(tData) do
    if "table" ~= type(tData) or "number" ~= type(tData[1]) or "number" ~= type(tData[2]) or "number" ~= type(nIndex) then
      Debug.Printf("Bad data for satellite targetting minigame, using default data.")
      _InitializeDefaultSectors(oMinigame)
      return
    end
  end
  local tNewData = {}
  for n, tData in ipairs(tData) do
    tNewData[n] = {}
    tNewData[n][1] = tData[1]
    tNewData[n][2] = tData[2]
  end
  oMinigame.CustomData.tSectorData = tNewData
end

function _InitializeDefaultSectors(oMinigame)
  local tSectorData = _tDefaultSectorData
  oMinigame.CustomData.tSectorData = tSectorData
  _CreateWidgetsFromSectorData(oMinigame)
end

function _CreateWidgetsFromSectorData(oMinigame)
  local tSectorData = oMinigame.CustomData.tSectorData
  local tSectorWidgets = oMinigame.CustomData.tSectorWidgets
  for n, oSector in ipairs(tSectorWidgets) do
    oSector.CustomData.bInUse = nil
    MrxGui.RemoveWidget(oSector)
  end
  for n, tData in ipairs(tSectorData) do
    if tSectorWidgets[n] then
      tSectorWidgets[n]:SetPieSliceRender(tSectorData[n][1], tSectorData[n][2])
      if not tSectorWidgets[n].CustomData.bInUse then
        tSectorWidgets[n].CustomData.bInUse = true
        MrxGuiBase.AddWidget(tSectorWidgets[n])
      end
    else
      local oNewSector = MrxGuiBase.ImageWidget:new()
      local oTemplate = tSectorWidgets[1]
      oNewSector:SetLocation(oTemplate:GetLocation())
      oNewSector:SetColor(oTemplate:GetColor())
      oNewSector:SetOwner(oTemplate:GetOwner())
      oNewSector:SetTexture(oTemplate:GetTexture())
      oNewSector:SetTextureCoordinates(oTemplate:GetTextureCoordinates())
      local tAnimData = oTemplate.AnimationPoints[oTemplate.CustomData.nOpenPoint]
      local nX1 = tAnimData.nX1
      local nY1 = tAnimData.nY1
      local nX2 = tAnimData.nX2
      local nY2 = tAnimData.nY2
      local nCenterX = (nX1 + nX2) * 0.5
      local nCenterY = (nY1 + nY2) * 0.5
      oNewSector.CustomData.nOpenPoint = oNewSector:AddAnimationPoint({
        x1 = nX1,
        y1 = nY1,
        x2 = nX2,
        y2 = nY2
      })
      oNewSector.CustomData.nClosePoint = oNewSector:AddAnimationPoint({
        x1 = nCenterX,
        y1 = nCenterY,
        x2 = nCenterX,
        y2 = nCenterY
      })
      oNewSector.CustomData.nBaseColor = oNewSector:AddAnimationPoint({
        RedLevel = nSliceR,
        GreenLevel = nSliceG,
        BlueLevel = nSliceB
      })
      oNewSector.CustomData.nLitColor = oNewSector:AddAnimationPoint({
        RedLevel = 255,
        GreenLevel = 255,
        BlueLevel = 255
      })
      oNewSector:SetPieSliceRender(tSectorData[n][1], tSectorData[n][2])
      oNewSector.CustomData.bInUse = true
      oNewSector.CustomData.bNewSector = true
      tSectorWidgets[n] = oNewSector
      oMinigame:AddChild(oNewSector)
      MrxGuiBase.AddWidget(oNewSector)
    end
  end
end

function _CollideWithSectors(oMinigame, nAngle, nTolerance)
  nTolerance = nTolerance or 0
  for n, tData in ipairs(oMinigame.CustomData.tSectorData) do
    if not tData.bHit and _DetectCollision(nAngle, tData[1], tData[2] + nTolerance) then
      return n
    end
  end
  return nil
end

function _DetectCollision(nAngle, nLow, nHigh)
  if nHigh < 0 then
    nHigh = nHigh + 360
    if nLow < 0 then
      nLow = nLow + 360
    end
  end
  if nLow < 0 and 0 <= nHigh then
    return nAngle <= nHigh or nAngle >= nLow + 360
  else
    return nAngle >= nLow and nAngle <= nHigh
  end
end

function _ResetSectors(oMinigame)
  for n, tData in ipairs(oMinigame.CustomData.tSectorData) do
    tData.bHit = nil
    local oSector = oMinigame.CustomData.tSectorWidgets[n]
    oSector:SetColor(192, 192, 192)
    oSector:SetVisible(true)
  end
end

function _HaveAllSectorsBeenHit(oMinigame)
  for n, tData in ipairs(oMinigame.CustomData.tSectorData) do
    if not tData.bHit then
      return false
    end
  end
  return true
end
