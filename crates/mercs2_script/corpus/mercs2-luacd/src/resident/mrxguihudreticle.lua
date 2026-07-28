import("MrxGui")
_bFloatCrosshair = false
_ksTargettingSound = "ui_HUD_SAM_targeting"

function HandleReticleColorChangeEvent(oWidget, nTargetRelation, nScreenX, nScreenY, nSpreadX, nSpreadY, nHealth, nMaxHealth)
  if nTargetRelation then
    if 0 < nTargetRelation then
      oWidget:SetColor(0, 0, 255)
      oWidget.ParentWidget:SetColor(0, 0, 255, nil, true)
    elseif nTargetRelation < 0 then
      oWidget:SetColor(255, 0, 0)
      oWidget.ParentWidget:SetColor(255, 0, 0, nil, true)
    else
      oWidget:SetColor(255, 255, 255)
      oWidget.ParentWidget:SetColor(255, 255, 255, nil, true)
    end
  else
    oWidget:SetColor(255, 255, 255)
    oWidget.ParentWidget:SetColor(255, 255, 255, nil, true)
  end
  local nAimX, nAimY = Gui.GetReticlePosition(oWidget:GetOwner())
  if _bFloatCrosshair then
    oWidget.CustomData.nSetCenterX = nScreenX or nAimX
    oWidget.CustomData.nSetCenterY = nScreenY or nAimY
  else
    oWidget.CustomData.nSetCenterX = nAimX
    oWidget.CustomData.nSetCenterY = nAimY
  end
  oWidget.CustomData.nSetSpreadX = nSpreadX or 0
  oWidget.CustomData.nSetSpreadY = nSpreadY or 0
  if "number" ~= type(nHealth) then
    nHealth = -1
  end
  if "number" ~= type(nMaxHealth) then
    nMaxHealth = -1
  end
  oWidget.CustomData.oHealth:SetHealth(nHealth, nMaxHealth)
  oWidget.CustomData.nCurHealth = nHealth
  oWidget.CustomData.nMaxHealth = nMaxHealth
end

function HandleReticleGunSwitchEvent(oWidget, tEvent)
  if "string" ~= type(tEvent.sReticleType) then
    return
  end
  if "Homing" == tEvent.sReticleType then
    oWidget:SetVisible(false)
  elseif "Normal" == tEvent.sReticleType then
    oWidget:SetVisible(true)
    if tEvent.uReticleTexture then
      oWidget:SetTexture(tEvent.uReticleTexture)
    end
    if not oWidget.CustomData.oCrosshair then
      oWidget.CustomData.oCrosshair = oWidget:GetChildren()[2]
    end
    if nil ~= tEvent.bReticleCrosshair then
      oWidget.CustomData.oCrosshair:SetVisible(tEvent.bReticleCrosshair)
    end
    if tEvent.sReticleHealthType then
      local oCrosshair = oWidget.CustomData.oCrosshair
      if "straight (bottom)" == tEvent.sReticleHealthType then
        oCrosshair.CustomData.oHealthCurve:Hide()
        oCrosshair.CustomData.oHealth = oCrosshair.CustomData.oHealthStraight
      else
        oCrosshair.CustomData.oHealthStraight:Hide()
        oCrosshair.CustomData.oHealth = oCrosshair.CustomData.oHealthCurve
      end
      oCrosshair.CustomData.oHealth:SetHealth(oCrosshair.CustomData.nCurHealth, oCrosshair.CustomData.nMaxHealth)
    end
  elseif "None" == tEvent.sReticleType then
    oWidget:SetVisible(false)
  end
end

function HandleReticleInitialization(oWidget)
  local nAimX, nAimY = Gui.GetReticlePosition(oWidget:GetOwner())
  _MoveReticle(oWidget, nAimX, nAimY)
  oWidget.SetOwner = SetReticleOwner
  oWidget.CustomData.nSpreadX = 0
  oWidget.CustomData.nSpreadY = 0
  oWidget:SetEventHandler("GuiReticlePositionChange", HandleReticlePositionChange)
end

function HandleReticlePositionChange(oWidget, tEvent)
  local nAimX, nAimY
  if tEvent.nX and tEvent.nY then
    nAimX = tEvent.nX
    nAimY = tEvent.nY
  else
    nAimX, nAimY = Gui.GetReticlePosition(oWidget:GetOwner())
  end
  _MoveReticle(oWidget, nAimX, nAimY)
end

function _MoveReticle(oWidget, nAimX, nAimY)
  local nX1, nY1, nX2, nY2 = oWidget:GetLocation()
  local nNewCenterX = 320 + nAimX * 320
  local nNewCenterY = 240 - nAimY * 240
  local nWidth = nX2 - nX1
  local nHeight = nY2 - nY1
  oWidget:SetLocation(nNewCenterX - nWidth / 2, nNewCenterY - nHeight / 2)
end

local _tSpread = {
  {nXS = 0, nYS = -1},
  {nXS = 1, nYS = 0},
  {nXS = 0, nYS = 1},
  {nXS = -1, nYS = 0}
}

function HandleCrosshairInitialization(oWidget)
  local nX1, nY1, nX2, nY2 = oWidget:GetLocation()
  local nXC = (nX1 + nX2) * 0.5
  local nYC = (nY1 + nY2) * 0.5
  local tOffsets = {}
  local tChildren = oWidget:GetChildren()
  for nIndex, oChild in pairs(tChildren) do
    local nX1, nY1, nX2, nY2 = oChild:GetLocation()
    tOffsets[nIndex] = {
      nX1 = nX1 - nXC,
      nY1 = nY1 - nYC,
      nX2 = nX2 - nXC,
      nY2 = nY2 - nYC,
      nXS = _tSpread[nIndex].nXS * 320,
      nYS = _tSpread[nIndex].nYS * 320
    }
  end
  oWidget.CustomData.tChildren = tChildren
  oWidget.CustomData.tOffsets = tOffsets
  local tParentChildren = oWidget.ParentWidget:GetChildren()
  oWidget.CustomData.oHealthCurve = tParentChildren[1]
  oWidget.CustomData.oHealthStraight = tParentChildren[3]
  oWidget.CustomData.oHealth = tParentChildren[1]
  oWidget.CustomData.oHealthStraight:SetVisible(false)
  oWidget.CustomData.nCurHealth = 0
  oWidget.CustomData.nMaxHealth = 0
  if not oWidget.EventHandlers.GuiUpdate then
    oWidget:SetEventHandler("GuiUpdate", HandleCrosshairUpdate)
  end
  local nAimX, nAimY = Gui.GetReticlePosition(oWidget:GetOwner())
  oWidget.CustomData.nSetCenterX = nAimX
  oWidget.CustomData.nCurCenterX = nAimX
  oWidget.CustomData.nSetCenterY = nAimY
  oWidget.CustomData.nCurCenterY = nAimY
  oWidget.CustomData.nSetSpreadX = 0
  oWidget.CustomData.nCurSpreadX = 0
  oWidget.CustomData.nSetSpreadY = 0
  oWidget.CustomData.nCurSpreadY = 0
end

function _MoveCrosshairChildToPoint(oChild, nXOffset, nYOffset, nTime, nIndex)
end

function Interpolate(nFrom, nTo, nRatio)
  if nRatio <= 0 then
    return nFrom
  end
  if 1 <= nRatio then
    return nTo
  end
  return nFrom + (nTo - nFrom) * nRatio
end

function HandleCrosshairUpdate(oWidget, nDt)
  oWidget.CustomData.nCurCenterX = Interpolate(oWidget.CustomData.nCurCenterX, oWidget.CustomData.nSetCenterX, nDt * 5)
  oWidget.CustomData.nCurCenterY = Interpolate(oWidget.CustomData.nCurCenterY, oWidget.CustomData.nSetCenterY, nDt * 5)
  oWidget.CustomData.nCurSpreadX = Interpolate(oWidget.CustomData.nCurSpreadX, oWidget.CustomData.nSetSpreadX, nDt * 5)
  oWidget.CustomData.nCurSpreadY = Interpolate(oWidget.CustomData.nCurSpreadY, oWidget.CustomData.nSetSpreadY, nDt * 5)
  local nCenterX = 320 + 320 * oWidget.CustomData.nCurCenterX
  local nCenterY = 240 - 240 * oWidget.CustomData.nCurCenterY
  local nSpreadX = oWidget.CustomData.nCurSpreadX
  local nSpreadY = oWidget.CustomData.nCurSpreadY
  for nIndex, oChild in ipairs(oWidget.CustomData.tChildren) do
    local tOffset = oWidget.CustomData.tOffsets[nIndex]
    oChild:SetLocation(nCenterX + tOffset.nX1 + tOffset.nXS * nSpreadX, nCenterY + tOffset.nY1 + tOffset.nYS * nSpreadY)
  end
end

_nHealthLength = 100
_nHalfHealthLength = _nHealthLength / 2
_nHealthCenter = 180
_nBaseAlpha = 80

function HandleHealthInitialization(oWidget)
  local oHealth = oWidget:GetChildren()[1]
  oWidget.CustomData.oHealth = oHealth
  oWidget:SetPieSliceRender(_nHealthCenter - _nHalfHealthLength - 2, _nHealthCenter + _nHalfHealthLength + 2)
  oHealth:SetPieSliceRender(_nHealthCenter - _nHalfHealthLength, _nHealthCenter + _nHalfHealthLength)
  oWidget.SetHealth = _SetHealth
  oWidget.Hide = _ForceHideHealth
  oWidget.CustomData.nFadedPoint = oWidget:AddAnimationPoint({TranslucencyLevel = 0})
  oWidget.CustomData.nVisiblePoint = oWidget:AddAnimationPoint({TranslucencyLevel = _nBaseAlpha})
end

function _SetHealth(oWidget, nCurrent, nMax)
  if 0 <= nCurrent and 0 < nMax then
    if oWidget:IsAnimating() then
      oWidget.CustomData.bFadingIn = true
      oWidget:AnimateToPoint(oWidget.CustomData.nVisiblePoint, 0, true, _HealthFadeInEnd)
    else
      oWidget:SetTranslucency(_nBaseAlpha)
    end
    nCurrent = math.min(nCurrent, nMax)
    local nPercent = nCurrent / nMax
    local nLength = _nHealthLength * nPercent
    oWidget.CustomData.oHealth:SetPieSliceRender(_nHealthCenter - _nHalfHealthLength + (_nHealthLength - nLength), _nHealthCenter + _nHalfHealthLength)
  elseif not oWidget:IsAnimating() or oWidget.CustomData.bFadingIn then
    oWidget:AnimateToPoint(oWidget.CustomData.nFadedPoint, 0.5, true)
  end
end

function _ForceHideHealth(oWidget)
  oWidget:AnimateToPoint(oWidget.CustomData.nFadedPoint, 0, true)
  oWidget.CustomData.bFadingIn = nil
end

function _HealthFadeInEnd(oWidget)
  oWidget.CustomData.bFadingIn = nil
end

function HandleHealthInitializationBar(oWidget)
  local oHealth = oWidget:GetChildren()[1]
  oWidget.CustomData.oHealth = oHealth
  local nX1, nY1, nX2, nY2 = oHealth:GetLocation()
  oWidget.CustomData.nX1 = nX1
  oWidget.CustomData.nLength = nX2 - nX1
  oWidget.SetHealth = _SetHealthStraight
  oWidget.Hide = _ForceHideHealth
  oWidget.CustomData.nFadedPoint = oWidget:AddAnimationPoint({TranslucencyLevel = 0})
  oWidget.CustomData.nVisiblePoint = oWidget:AddAnimationPoint({TranslucencyLevel = _nBaseAlpha})
  oWidget:Hide()
end

function _SetHealthStraight(oWidget, nCurrent, nMax)
  if 0 <= nCurrent and 0 < nMax then
    if oWidget:IsAnimating() then
      oWidget.CustomData.bFadingIn = true
      oWidget:AnimateToPoint(oWidget.CustomData.nVisiblePoint, 0, true, _HealthFadeInEnd)
    else
      oWidget:SetTranslucency(_nBaseAlpha)
    end
    nCurrent = math.min(nCurrent, nMax)
    local nPercent = nCurrent / nMax
    local nLength = oWidget.CustomData.nLength * nPercent
    oWidget.CustomData.oHealth:SetLocation(oWidget.CustomData.nX1, nil, oWidget.CustomData.nX1 + nLength, nil)
  elseif not oWidget:IsAnimating() or oWidget.CustomData.bFadingIn then
    oWidget:AnimateToPoint(oWidget.CustomData.nFadedPoint, 0.5, true)
  end
end

function SetReticleOwner(oWidget, uGuid)
  MrxGui.Widget.SetOwner(oWidget, uGuid)
  if Gui.GetReticlePosition then
    local nAimX, nAimY = Gui.GetReticlePosition(oWidget:GetOwner())
    local nX1, nY1, nX2, nY2 = oWidget:GetLocation()
    local nNewCenterX = 320 + nAimX * 320
    local nNewCenterY = 240 - nAimY * 240
    local nWidth = nX2 - nX1
    local nHeight = nY2 - nY1
    oWidget:SetLocation(nNewCenterX - nWidth / 2, nNewCenterY - nHeight / 2)
  end
end

_kNoFlashTime = -1
_kSlowFlashTime = 0.15
_kMedFlashTime = 0.15
_kFastFlashTime = 0.01
_kMedBegin = 0.4
_kHighBegin = 0.75
_ktNeutralColor = {
  nR = 128,
  nG = 128,
  nB = 128
}
_ktLockonColor = {
  nR = 0,
  nG = 255,
  nB = 0
}

function HandleStingerReticleInitialization(oWidget, tData)
  oWidget.CustomData.nPercent = 0
  local tChildren = oWidget:GetChildren()
  local oTargettingReticle = tChildren[1]
  oTargettingReticle.CustomData.bFlashState = false
  oTargettingReticle.CustomData.nTimeUntilFlash = 0
  oTargettingReticle.CustomData.nFlashTime = _kSlowFlashTime
  oTargettingReticle:SetColor(_ktNeutralColor.nR, _ktNeutralColor.nG, _ktNeutralColor.nB)
  oTargettingReticle.CustomData.nFramesWithoutUpdate = 0
  oWidget.CustomData.oTargettingReticle = oTargettingReticle
  oWidget.CustomData.oLeft = tChildren[2]
  oWidget.CustomData.oRight = tChildren[3]
  oWidget.CustomData.oHealth = tChildren[4]
  if _GuiInternal.SetWidgetUseResolutionCorrection then
    _GuiInternal.SetWidgetUseResolutionCorrection(oTargettingReticle.BasicData.uId, false)
  end
  Gui.LoadTexture("global_gui_reticle_stinger_target")
end

function HandleStingerReticleColorChangeEvent(oWidget, nTargetRelation, nScreenX, nScreenY, nSpreadX, nSpreadY, nHealth, nMaxHealth)
  if "number" ~= type(nHealth) then
    nHealth = -1
  end
  if "number" ~= type(nMaxHealth) then
    nMaxHealth = -1
  end
  oWidget.CustomData.oHealth:SetHealth(nHealth, nMaxHealth)
end

function HandleStingerReticleDataUpdate(oWidget, tData)
  local bResetFlash = false
  local oTargettingReticle = oWidget.CustomData.oTargettingReticle
  if tData.nPercent > 0 then
    oTargettingReticle.CustomData.nFramesWithoutUpdate = 0
  end
  if not oTargettingReticle.EventHandlers.GuiUpdate then
    oTargettingReticle:SetEventHandler("GuiUpdate", HandleStingerReticleUpdate)
  end
  oTargettingReticle:SetVisible(true)
  if tData.nPercent >= 1 then
    oTargettingReticle:SetColor(_ktLockonColor.nR, _ktLockonColor.nG, _ktLockonColor.nB)
    oTargettingReticle.CustomData.bFlashState = true
    oTargettingReticle.CustomData.nFlashTime = _kNoFlashTime
    if oTargettingReticle.CustomData.bSoundLooping then
      Sound.StopSound(0, _ksTargettingSound)
      oTargettingReticle.CustomData.bSoundLooping = false
    end
  else
    if tData.nPercent >= _kHighBegin then
      oTargettingReticle.CustomData.nFlashTime = _kFastFlashTime
    elseif tData.nPercent >= _kMedBegin then
      oTargettingReticle.CustomData.nFlashTime = _kMedFlashTime
    else
      oTargettingReticle.CustomData.nFlashTime = _kSlowFlashTime
    end
    if tData.nPercent >= _kHighBegin and oWidget.CustomData.nPercent <= _kHighBegin then
      bResetFlash = true
    end
    if bResetFlash then
      oTargettingReticle.CustomData.nTimeUntilFlash = 0
    end
    if not oTargettingReticle.CustomData.bSoundLooping then
      Sound.CueSound(0, _ksTargettingSound)
      oTargettingReticle.CustomData.bSoundLooping = true
    end
  end
  if tData.nX and tData.nY then
    local nX1, nY1, nX2, nY2 = oTargettingReticle:GetLocation()
    local nWidth = nX2 - nX1
    local nHeight = nY2 - nY1
    oTargettingReticle:SetLocation(tData.nX - nWidth / 2, tData.nY - nHeight / 2)
  end
  oWidget.CustomData.nPercent = tData.nPercent
end

function HandleStingerReticleUpdate(oWidget, nDeltaTime)
  oWidget.CustomData.nFramesWithoutUpdate = oWidget.CustomData.nFramesWithoutUpdate + 1
  if oWidget.CustomData.nFramesWithoutUpdate > 1 then
    oWidget:SetColor(_ktNeutralColor.nR, _ktNeutralColor.nG, _ktNeutralColor.nB)
    oWidget.CustomData.bFlashState = false
    oWidget:SetVisible(false)
    oWidget:SetEventHandler("GuiUpdate", nil)
    if oWidget.CustomData.bSoundLooping then
      Sound.StopSound(0, _ksTargettingSound)
      oWidget.CustomData.bSoundLooping = false
    end
  end
  if 0 > oWidget.CustomData.nFlashTime then
    return
  end
  oWidget.CustomData.nTimeUntilFlash = oWidget.CustomData.nTimeUntilFlash - nDeltaTime
  if 0 >= oWidget.CustomData.nTimeUntilFlash then
    oWidget.CustomData.nTimeUntilFlash = oWidget.CustomData.nFlashTime
    if oWidget.CustomData.bFlashState then
      oWidget:SetColor(_ktLockonColor.nR, _ktLockonColor.nG, _ktLockonColor.nB)
    else
      oWidget:SetColor(_ktNeutralColor.nR, _ktNeutralColor.nG, _ktNeutralColor.nB)
    end
    oWidget.CustomData.bFlashState = not oWidget.CustomData.bFlashState
  end
end

function HandleStingerReticleGunSwitchEvent(oWidget, tEvent)
  if not tEvent.sReticleType then
    return
  end
  if "Homing" == tEvent.sReticleType then
    oWidget:SetVisible(true)
    oWidget.CustomData.oTargettingReticle:SetVisible(false)
    if tEvent.nMaxLockOnRadius then
      SetStingerReticleRadius(oWidget, tEvent.nMaxLockOnRadius)
    end
    if tEvent.nStingerReticleWidth and tEvent.nStingerReticleHeight then
      SetStingerReticleDimensions(oWidget, tEvent.nStingerReticleWidth, tEvent.nStingerReticleHeight)
    end
  else
    oWidget:SetVisible(false)
    local oTargettingReticle = oWidget.CustomData.oTargettingReticle
    if oTargettingReticle.CustomData.bSoundLooping then
      Sound.StopSound(0, _ksTargettingSound)
      oTargettingReticle.CustomData.bSoundLooping = false
    end
  end
end

function SetStingerReticleRadius(oWidget, nRadius)
  local nX1, nY1, nX2, nY2 = oWidget:GetLocation()
  local nX = (nX1 + nX2) * 0.5
  local nY = (nY1 + nY2) * 0.5
  oWidget:SetCoordinates(nX - nRadius, nY - nRadius, nX + nRadius, nY + nRadius)
end

function SetStingerReticleDimensions(oWidget, nWidth, nHeight)
  local nX1, nY1, nX2, nY2 = oWidget:GetCorrectedLocation()
  local nHealthX1, nHealthY1, nHealthX2, nHealthY2 = oWidget.CustomData.oHealth:GetLocation()
  if nX1 and nY1 and nX2 and nY2 and nHealthX1 and nHealthY1 and nHealthX2 and nHealthY2 then
    local nX = (nX1 + nX2) * 0.5
    local nY = (nY1 + nY2) * 0.5
    oWidget:SetCorrectedLocation(nX - nWidth / 2, nY - nHeight / 2, nX + nWidth / 2, nY + nHeight / 2)
    oWidget.CustomData.oLeft:SetCorrectedLocation(nX - nWidth / 2, nY - nHeight / 2, nX, nY + nHeight / 2)
    oWidget.CustomData.oRight:SetCorrectedLocation(nX, nY - nHeight / 2, nX + nWidth / 2, nY + nHeight / 2)
    local nWidthRatio = nWidth / (nX2 - nX1)
    local nHealthWidth = (nHealthX2 - nHealthX1) * nWidthRatio
    local nHealthX = (nHealthX1 + nHealthX2) * 0.5
    oWidget.CustomData.oHealth:SetLocation(nHealthX - nHealthWidth * 0.5, nHealthY1, nHealthX + nHealthWidth * 0.5, nHealthY2)
    oWidget.CustomData.oHealth:GetChildren()[1]:SetLocation(nHealthX - nHealthWidth * 0.5, nHealthY1, nHealthX + nHealthWidth * 0.5, nHealthY2)
  end
end

function HandleLaserReticleInitialization(oWidget)
  local tChildren = oWidget:GetChildren()
  local tCircle = {}
  tCircle[1] = tChildren[2]
  tCircle[2] = tChildren[3]
  tCircle[3] = tChildren[4]
  tCircle[4] = tChildren[5]
  oWidget.CustomData.tCircle = tCircle
  local oArrow = tChildren[1]
  oArrow.CustomData.nPoint = oArrow:AddAnimationPoint({nRotation = 359, nRotationDirection = 1})
  oArrow.CustomData.nStopPoint = oArrow:AddAnimationPoint({})
  oWidget.CustomData.oArrow = oArrow
  oWidget.CustomData.nEnterPoint = oWidget:AddAnimationPoint({TranslucencyLevel = 128})
  oWidget.CustomData.nExitPoint = oWidget:AddAnimationPoint({TranslucencyLevel = 0})
  oWidget.CustomData.bEnabled = true
  oWidget:SetVisible(false)
  _LaserReticleDisable(oWidget)
end

function HandleLaserReticleGunSwitchEvent(oWidget, tEvent)
  if "string" ~= type(tEvent.sReticleType) then
    _LaserReticleDisable(oWidget)
    return
  end
  if "Laser" == tEvent.sReticleType then
    oWidget:SetTranslucency(0)
    oWidget:SetVisible(true)
    oWidget.CustomData.bEnabled = true
    oWidget:AnimateToPoint(oWidget.CustomData.nEnterPoint, 0.5, true)
    _LaserReticleSnapToNeutral(oWidget)
  else
    _LaserReticleDisable(oWidget)
  end
end

function HandleLaserReticleStateChangeEvent(oWidget, tEvent)
  if tEvent.bActive and tEvent.nTime then
    _LaserReticleAnimate(oWidget, tEvent.nTime)
  else
    _LaserReticleSnapToNeutral(oWidget)
  end
end

function _LaserReticleDisable(oWidget)
  if oWidget.CustomData.bEnabled then
    _LaserReticleSnapToNeutral(oWidget)
    oWidget:SetVisible(false)
    oWidget.CustomData.bEnabled = false
  end
end

function _LaserReticleAnimate(oWidget, nTime)
  for nIndex, oCircle in pairs(oWidget.CustomData.tCircle) do
    oCircle:PlayAnimation(0, 30, nTime)
  end
  _LaserReticleAnimateArrow(oWidget, nTime)
end

function _LaserReticleAnimateArrow(oWidget, nTime)
  local oArrow = oWidget.CustomData.oArrow
  if nTime < 0 then
    oArrow:AnimateToPoint(oArrow.CustomData.nStopPoint, 0, true)
  else
    oArrow:SetRotation(0)
    oArrow.CustomData.nTime = nTime
    oArrow:AnimateToPoint(oArrow.CustomData.nPoint, nTime, true, _LaserReticleArrowLoop, oArrow)
  end
end

function _LaserReticleSnapToNeutral(oWidget)
  for nIndex, oCircle in pairs(oWidget.CustomData.tCircle) do
    oCircle:HaltAnimation()
    oCircle:SetFrame(0)
  end
  local oArrow = oWidget.CustomData.oArrow
  oArrow:AnimateToPoint(oArrow.CustomData.nPoint, 0, true, oArrow.SetRotation, {0})
end

function _LaserReticleArrowLoop(oArrow)
  oArrow:SetRotation(0)
  oArrow:AnimateToPoint(oArrow.CustomData.nPoint, oArrow.CustomData.nTime, true, _LaserReticleArrowLoop, oArrow)
end
