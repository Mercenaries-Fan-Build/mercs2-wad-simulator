import("MrxGuiBase")
_knMin = 0
_knMax = 100
_ksPursuit = "[0x1cab5133]"
_tLevels = false
_tLevelNames = false
_tLevelColors = false

function Init()
  _tLevels = {
    _knMin,
    25,
    50,
    75
  }
  _tLevelNames = {
    "[0x671b379b]",
    "[0x7c4225bc]",
    "[0xdb614732]",
    "[0x8c4d842e]"
  }
  local tWhite = {
    nR = 255,
    nG = 255,
    bB = 255
  }
  _tLevelColors = {
    {
      nR = 255,
      nG = 96,
      nB = 96
    },
    {
      nR = 160,
      nG = 160,
      nB = 160
    },
    {
      nR = 96,
      nG = 96,
      nB = 255
    },
    {
      nR = 96,
      nG = 96,
      nB = 255
    }
  }
end

function GetBarValueAndName(nValue)
  local nLevel = #_tLevels
  local n = nLevel
  while 0 < n do
    if nValue < _tLevels[n] then
      nLevel = n - 1
    end
    n = n - 1
  end
  local nRangeMin = _tLevels[nLevel]
  local nRangeMax = _tLevels[nLevel + 1] or 100
  local nBarValue = (nValue - nRangeMin) * (100 / (nRangeMax - nRangeMin))
  return nValue, _tLevelNames[nLevel]
end

function SetLevels(tLevelThresholds, tLevelNames, sPursuitName, bDisplayResult)
  for nIndex, nLevel in ipairs(tLevelThresholds) do
    if "number" ~= type(nLevel) then
      Debug.Printf("Faction display level setup error: Non-number for threshold")
      return false
    end
  end
  for nIndex, sLevelName in ipairs(tLevelNames) do
    if "string" ~= type(sLevelName) then
      Debug.Printf("Faction display level setup error: Non-string for level name")
      return false
    end
  end
  if tLevelThresholds[1] ~= 0 then
    Debug.Printf("Faction display level setup error: First threshold is not 0")
    return false
  end
  if #tLevelThresholds ~= #tLevelNames then
    Debug.Printf("Faction display level setup error: Number of thresholds and level names are not equal.")
    return false
  end
  local nPrevLevel = -1
  for nIndex, nLevel in ipairs(tLevelThresholds) do
    if nPrevLevel == nLevel then
      Debug.Printf("Faction display level setup error: One mood threshold has 0 range.")
      return false
    elseif nLevel < nPrevLevel then
      Debug.Printf("Faction display level setup error: thresholds not in ascending order.")
      return false
    end
  end
  _tLevels = {}
  for nIndex, nLevel in ipairs(tLevelThresholds) do
    _tLevels[nIndex] = nLevel
  end
  _tLevelNames = {}
  for nIndex, sName in ipairs(tLevelNames) do
    _tLevelNames[nIndex] = sName
  end
  if "string" == type(sPursuitName) then
    _ksPursuit = sPursuitName
  end
  if bDisplayResult then
    for nIndex, nLevel in ipairs(_tLevels) do
      if _tLevels[nIndex + 1] then
        Debug.Printf("[" .. nLevel .. ", " .. _tLevels[nIndex + 1] .. ") = " .. _tLevelNames[nIndex])
      else
        Debug.Printf("[" .. nLevel .. ", " .. 100 .. "] = " .. _tLevelNames[nIndex])
      end
    end
  end
  return true
end

function Initialize(oWidget)
  local tData = oWidget.CustomData
  local tChildren = oWidget:GetChildren()
  tData.nCurrentValue = 0
  tData.nCurrentMood = 0
  tData.nCurrentLevel = 1
  tData.nMaxLevel = 4
  tData.nMinLevel = 1
  local tGaugeChildren = tChildren[2]:GetChildren()
  tData.oGaugeFront = tGaugeChildren[3]
  tData.oGaugeDelta = tGaugeChildren[2]
  tData.oIcon = tChildren[3]
  tData.oMood = tChildren[5]
  tData.oPursuit = tChildren[1]:GetChildren()[1]
  tData.oMood:SetJustification("right")
  local nX1, nY1, nX2, nY2 = tData.oGaugeFront:GetLocation()
  local nU1, nV1, nU2, nV2 = tData.oGaugeFront:GetTextureCoordinates()
  tData.nGaugeLength = nX2 - nX1
  tData.nGaugeBaseX = nX1
  tData.nGaugeFrontEmptyPoint = tData.oGaugeFront:AddAnimationPoint({
    x = nX1,
    x2 = nX1,
    nU1 = nU1,
    nV1 = nV1,
    nU2 = nU1,
    nV2 = nV2
  })
  tData.nGaugeFrontDestPoint = tData.oGaugeFront:AddAnimationPoint({x = nX1, x2 = nX2})
  tData.nGaugeDeltaDestPoint = tData.oGaugeDelta:AddAnimationPoint({x = nX1, x2 = nX2})
  tData.nGaugeFrontColorPoint = tData.oGaugeFront:AddAnimationPoint({})
  tData.nGaugeDeltaNullPoint = tData.oGaugeDelta:AddAnimationPoint({})
  tData.oGaugeDelta:SetLocation(nX2, nil, nX2, nil)
  local nR, nG, nB = tData.oMood:GetColor()
  tData.nMoodRaisePoint = tData.oMood:AddAnimationPoint({
    RedLevel = 64,
    GreenLevel = 255,
    BlueLevel = 64
  })
  tData.nMoodLowerPoint = tData.oMood:AddAnimationPoint({
    RedLevel = 210,
    GreenLevel = 0,
    BlueLevel = 0
  })
  tData.nMoodStartPoint = tData.oMood:AddAnimationPoint({
    RedLevel = nR,
    GreenLevel = nG,
    BlueLevel = nB
  })
  tData.nGaugeFrontU1 = nU1
  tData.nGaugeFrontUDiff = nU2 - nU1
  oWidget.SetValue = SetValue
  oWidget.GetValue = GetValue
  oWidget.SetIcon = SetIcon
  oWidget.SetIconVisible = SetIconVisible
  oWidget.ChangeValue = ChangeValue
  oWidget.StartTimer = StartTimer
  oWidget.StopTimer = StopTimer
  oWidget.StartPursuit = StartPursuitGauge
  oWidget.StopPursuit = StopPursuitGauge
  oWidget.IsPursuitActive = IsPursuitActive
  oWidget.GetRemainingPursuitTime = GetRemainingPursuitTime
  oWidget._Initialize = Initialize
  oWidget._RiseToValue = _RiseToValue
  oWidget._CancelRise = _CancelRise
  oWidget._RealSetVisible = MrxGuiBase.Widget.SetVisible
  oWidget.SetVisible = _SetVisible
  tData.oTimer = tChildren[4]
  tData.fTimerCallback = nil
  tData.tTimerCallbackData = nil
  tData.oTimer:SetVisible(false)
  oWidget:SetValue(100, true)
end

function SetValue(oWidget, nValue, bInitialize)
  local tData = oWidget.CustomData
  if tData.bPursuitActive and nValue > _knMin then
    oWidget:StopPursuit()
  end
  local nX1, nY1, nX2, nY2 = tData.oGaugeFront:GetLocation()
  local nDisplayValue = (nX2 - nX1) / tData.nGaugeLength * 100
  local nNewLevel
  if nValue <= tData.nCurrentMood then
    local nLoopLevel = tData.nCurrentLevel
    while nLoopLevel >= tData.nMinLevel do
      if nValue < _tLevels[nLoopLevel] then
        nNewLevel = nLoopLevel - 1
      end
      nLoopLevel = nLoopLevel - 1
    end
  elseif nValue >= tData.nCurrentMood then
    local nLoopLevel = tData.nCurrentLevel + 1
    local nMaxLevel = #_tLevels
    while nLoopLevel <= nMaxLevel do
      if nValue >= _tLevels[nLoopLevel] then
        nNewLevel = nLoopLevel
      end
      nLoopLevel = nLoopLevel + 1
    end
  end
  local nCurLevel = nNewLevel or tData.nCurrentLevel
  local nRangeMin = _tLevels[nCurLevel]
  local nRangeMax = _tLevels[nCurLevel + 1] or 100
  local nBarValue = nValue
  tData.nCurrentMood = nValue
  if bInitialize then
    repeat
      _SnapBarToValue(oWidget, nBarValue)
      tData.nCurrentLevel = nCurLevel
      tData.nCurrentValue = nBarValue
      oWidget.CustomData.oMood:SetText(_tLevelNames[oWidget.CustomData.nCurrentLevel])
      do return end
      do break end -- pseudo-goto
      local nTimePerBar = 1.5
      local nLevelDiff = math.abs(nNewLevel - tData.nCurrentLevel) - 1
      if 2 <= nLevelDiff then
        nTimePerBar = 0.5
      elseif 1 <= nLevelDiff then
        nTimePerBar = 1
      end
      local nTotalTime = nLevelDiff * nTimePerBar
      local nTime
      if nNewLevel > tData.nCurrentLevel then
        nTime = (_knMax - nDisplayValue) * nTimePerBar * 0.01
        nTotalTime = nTotalTime + nBarValue * nTimePerBar * 0.01
      elseif nNewLevel < tData.nCurrentLevel then
        nTime = nDisplayValue * nTimePerBar * 0.01
        nTotalTime = nTotalTime + (_knMax - nBarValue) * nTimePerBar * 0.01
      end
      if nNewLevel < tData.nCurrentLevel then
        SetValueAndLevel(oWidget, _knMin, false, nTime, tData.nCurrentLevel - 1, _TransitionToLevel, {
          oWidget,
          nNewLevel,
          nBarValue,
          nTotalTime,
          false
        })
      elseif nNewLevel > tData.nCurrentLevel then
        SetValueAndLevel(oWidget, _knMax, false, nTime, tData.nCurrentLevel + 1, _TransitionToLevel, {
          oWidget,
          nNewLevel,
          nBarValue,
          nTotalTime,
          true
        })
      end
    until true
  else
    SetValueAndLevel(oWidget, nBarValue, bInitialize, _Abs(nBarValue - tData.nCurrentValue) * 0.01 * 2, nNewLevel)
  end
end

function _TransitionToLevel(oWidget, nTargetLevel, nTargetValue, nRemainingTime, bRising)
  local tData = oWidget.CustomData
  if tData.nCurrentLevel == 1 and nTargetLevel < 1 then
  elseif nTargetLevel == tData.nCurrentLevel then
    if bRising then
      _SnapBarToValue(oWidget, _knMin)
    else
      _SnapBarToValue(oWidget, _knMax)
    end
    SetValueAndLevel(oWidget, nTargetValue, false, nRemainingTime, nil, nil, nil)
  elseif bRising then
    _SnapBarToValue(oWidget, _knMin)
    local nRemainingBars = (nTargetValue - _knMin) * 0.01 + (nTargetLevel - tData.nCurrentLevel)
    local nTime
    if 0 < nRemainingBars then
      nTime = nRemainingTime / nRemainingBars
    else
      nTime = 0
    end
    nRemainingTime = nRemainingTime - nTime
    SetValueAndLevel(oWidget, _knMax, false, nTime, tData.nCurrentLevel + 1, _TransitionToLevel, {
      oWidget,
      nTargetLevel,
      nTargetValue,
      nRemainingTime,
      bRising
    })
  else
    _SnapBarToValue(oWidget, _knMax)
    local nRemainingBars = (_knMax - nTargetValue) * 0.01 + (tData.nCurrentLevel - nTargetLevel)
    local nTime
    if 0 < nRemainingBars then
      nTime = nRemainingTime / nRemainingBars
    else
      nTime = 0
    end
    nRemainingTime = nRemainingTime - nTime
    SetValueAndLevel(oWidget, _knMin, false, nTime, tData.nCurrentLevel - 1, _TransitionToLevel, {
      oWidget,
      nTargetLevel,
      nTargetValue,
      nRemainingTime,
      bRising
    })
  end
end

function SetValueAndLevel(oWidget, nNewValue, bInitialize, nTime, nNewLevel, fCallback, tCallbackData)
  local nValue = _Clamp(nNewValue, _knMin, _knMax)
  if nValue == oWidget.CustomData.nCurrentValue and not oWidget.CustomData.bPursuitActive and not bInitialize then
    oWidget.CustomData.nCurrentLevel = nNewLevel or oWidget.CustomData.nCurrentLevel
    oWidget.CustomData.oMood:SetText(_tLevelNames[oWidget.CustomData.nCurrentLevel])
    if fCallback then
      tCallbackData = tCallbackData or {}
      fCallback(unpack(tCallbackData))
    end
    return
  end
  local tData = oWidget.CustomData
  tData.oGaugeFront:SetColor(255, 255, 255)
  local nX1, nY1, nX2, nY2 = tData.oGaugeFront:GetLocation()
  local nU1, nV1, nU2, nV2 = tData.oGaugeFront:GetTextureCoordinates()
  local nNewX2 = tData.nGaugeBaseX + nValue * 0.01 * tData.nGaugeLength
  local nNewU2 = tData.nGaugeFrontU1 + nValue * 0.01 * tData.nGaugeFrontUDiff
  local nDisplayValue = (nX2 - nX1) / tData.nGaugeLength * 100
  local tNewFrontPoint = {
    x = nX1,
    x2 = nNewX2,
    nU1 = nU1,
    nV1 = nV1,
    nU2 = nNewU2,
    nV2 = nV2
  }
  local tNewDeltaPoint = {x = nNewX2, x2 = nNewX2}
  tData.oGaugeFront:SetAnimationPoint(tData.nGaugeFrontDestPoint, tNewFrontPoint)
  tData.oGaugeDelta:SetAnimationPoint(tData.nGaugeDeltaDestPoint, tNewDeltaPoint)
  local nTime = nTime or math.abs(nValue - tData.nCurrentValue) * 0.01 * 2
  local nColorTime
  if nTime then
    nColorTime = nTime * 0.25
  else
    nColorTime = 0.5
  end
  if bInitialize then
    tData.oGaugeFront:AnimateToPoint(tData.nGaugeFrontDestPoint, 0, true)
    tData.oGaugeDelta:AnimateToPoint(tData.nGaugeDeltaDestPoint, 0, true, _FinishGaugeAnimation, {
      oWidget,
      tData.oGaugeDelta.SetVisible,
      {false}
    })
  elseif nValue < nDisplayValue then
    tData.oGaugeDelta:SetColor(128, 0, 0)
    tData.oGaugeDelta:AnimateToPoint(tData.nGaugeDeltaNullPoint, 0, true)
    tData.oGaugeFront:SetLocation(nX1, nil, nNewX2)
    tData.oGaugeFront:SetTextureCoordinates(nil, nil, tData.nGaugeFrontU1 + tData.nGaugeFrontUDiff * (nValue / (_knMax - _knMin)))
    tData.oGaugeFront:AnimateToPoint(tData.nGaugeFrontColorPoint, nColorTime, true, _Animate, {
      tData.oGaugeDelta,
      tData.nGaugeDeltaDestPoint,
      nTime,
      true,
      _FinishGaugeAnimation,
      {
        oWidget,
        fCallback,
        tCallbackData,
        true
      }
    })
    tData.oGaugeDelta:SetLocation(nNewX2, nil, nX2)
  elseif nValue > nDisplayValue then
    tData.oGaugeDelta:SetColor(0, 128, 0)
    tData.oGaugeDelta:AnimateToPoint(tData.nGaugeDeltaNullPoint, 0, true)
    tData.oGaugeDelta:SetLocation(nX2, nil, nNewX2)
    tData.oGaugeFront:AnimateToPoint(tData.nGaugeFrontColorPoint, nColorTime, true, _Animate, {
      tData.oGaugeDelta,
      tData.nGaugeDeltaDestPoint,
      nTime,
      true
    })
    tData.oGaugeFront:AnimateToPoint(tData.nGaugeFrontDestPoint, nTime, false, _FinishGaugeAnimation, {
      oWidget,
      fCallback,
      tCallbackData,
      true
    })
  end
  tData.nCurrentValue = nNewValue
  tData.nCurrentLevel = nNewLevel or tData.nCurrentLevel
  if tData.nCurrentLevel and _tLevelColors[tData.nCurrentLevel] then
    local tColor = _tLevelColors[tData.nCurrentLevel]
    tData.oGaugeFront:SetColor(tColor.nR, tColor.nG, tColor.nB)
  end
end

function _SnapBarToValue(oWidget, nValue)
  local tData = oWidget.CustomData
  local nX1 = tData.nGaugeBaseX
  local nU1, nV1, nU2, nV2 = tData.oGaugeFront:GetTextureCoordinates()
  nU1 = tData.nGaugeFrontU1
  nU2 = tData.nGaugeFrontU1 + tData.nGaugeFrontUDiff * (nValue / 100)
  tData.oGaugeFront:SetLocation(nX1, nil, nX1 + tData.nGaugeLength * (nValue / 100))
  tData.oGaugeFront:SetTextureCoordinates(nU1, nil, nU2)
  tData.oGaugeDelta:SetLocation(nX1, nil, nX1)
  tData.oGaugeFront:SetAnimationPoint(tData.nGaugeFrontDestPoint, {
    x = nX1,
    x2 = nX1 + tData.nGaugeLength * (nValue / 100),
    nU1 = nU1,
    nV1 = nV1,
    nU2 = nU2,
    nV2 = nV2
  })
  tData.oGaugeDelta:SetAnimationPoint(tData.nGaugeDeltaDestPoint, {x = nX1, x2 = nX2})
  tData.oGaugeFront:AnimateToPoint(tData.nGaugeFrontDestPoint, 0, true)
  tData.oGaugeDelta:AnimateToPoint(tData.nGaugeDeltaDestPoint, 0, true)
  tData.nCurrentValue = nValue
  if tData.nCurrentLevel and _tLevelColors[tData.nCurrentLevel] then
    local tColor = _tLevelColors[tData.nCurrentLevel]
    tData.oGaugeFront:SetColor(tColor.nR, tColor.nG, tColor.nB)
  end
end

function GetValue(oWidget)
  return oWidget.CustomData.nCurrentValue
end

function SetIcon(oWidget, sTexture)
  if oWidget.CustomData.oIcon then
    oWidget.CustomData.oIcon:SetTexture(sTexture)
  end
end

function SetIconVisible(oWidget, bVisible, nTranslucency)
  if oWidget.CustomData.oIcon then
    oWidget.CustomData.oIcon:SetVisible(bVisible)
    if nTranslucency then
      oWidget.CustomData.oIcon:SetTranslucency(nTranslucency)
    end
  end
end

function ChangeValue(oWidget, nDelta, bInitialize)
  oWidget:SetValue(oWidget:GetValue() + nDelta, bInitialize)
end

function StartTimer(oWidget, nTime, fCallback, tCallbackData)
  local tData = oWidget.CustomData
  oWidget:SetVisible(true, true)
  tData.fTimerCallback = fCallback
  tData.tTimerCallbackData = tCallbackData
  tData.oTimer:SetCallback(_TimerCallback, {oWidget})
  tData.oTimer:Start(nTime)
end

function StopTimer(oWidget)
  local tData = oWidget.CustomData
  tData.fTimerCallback = nil
  tData.tTimerCallbackData = nil
  tData.oTimer:SetCallback(nil)
  tData.oTimer:Stop(nTime)
end

function _TimerCallback(oWidget)
  local tData = oWidget.CustomData
  oWidget:SetVisible(true, false)
  if "function" == type(tData.fTimerCallback) then
    local fFunction = tData.fTimerCallback
    local tCallbackData = {}
    if "table" == type(tData.tTimerCallbackData) then
      tCallbackData = tData.tTimerCallbackData
    end
    tData.fTimerCallback = nil
    tData.tTimerCallbackData = nil
    fFunction(unpack(tCallbackData))
  end
end

function _FinishGaugeAnimation(oUnused, oWidget, fSecondCallback, tData, bSkipAnimationInsert)
  oWidget.CustomData.oMood:SetText(_tLevelNames[oWidget.CustomData.nCurrentLevel])
  if "function" == type(fSecondCallback) then
    tData = tData or {}
    if not bSkipAnimationInsert then
      table.insert(tData, 1, oUnused)
    end
    fSecondCallback(unpack(tData))
  end
end

function StartPursuitGauge(oWidget, nTime, fCallback, tCallbackData)
  local tData = oWidget.CustomData
  oWidget:SetValue(tData.nCurrentMood, true)
  oWidget:SetVisible(true, false, true)
  tData.bPursuitActive = true
  local oPursuit = tData.oPursuit
  local tData = oWidget.CustomData
  tData.oMood:SetText(_ksPursuit)
  tData.oGaugeFront:SetColor(210, 0, 0)
  tData.oGaugeFront:AnimateToPoint(tData.nGaugeFrontEmptyPoint, 0, true, _AnimateToEnd, {oWidget, nTime})
  local nX = tData.oGaugeDelta:GetLocation()
  tData.oGaugeDelta:SetLocation(nX, nil, nX, nil)
  tData.oGaugeDelta:AnimateToPoint(tData.nGaugeDeltaNullPoint, 0, true)
  if "number" == type(nTime) and 0 < nTime then
    oPursuit.CustomData.fCallback = fCallback
    oPursuit.CustomData.tCallbackData = tCallbackData
    oPursuit.CustomData.nTotalTime = nTime
    oPursuit:SetClockAnimationCallback(_PursuitAnimationComplete, {oWidget})
    oPursuit:SetClockAnimation(0, nTime, false, true)
    oPursuit:SetVisible(false)
  end
end

function _AnimateToEnd(oGauge, oWidget, nTime)
  local tData = oWidget.CustomData
  local nX1 = tData.nGaugeBaseX
  local nU1, nV1, nU2, nV2 = oGauge:GetTextureCoordinates()
  nU1 = tData.nGaugeFrontU1
  local nNewX2 = nX1 + tData.nGaugeLength
  local nNewU2 = nU1 + tData.nGaugeFrontUDiff
  local tNewFrontPoint = {
    x = nX1,
    x2 = nNewX2,
    nU1 = nU1,
    nV1 = nV1,
    nU2 = nNewU2,
    nV2 = nV2
  }
  oGauge:SetAnimationPoint(tData.nGaugeFrontDestPoint, tNewFrontPoint)
  oGauge:AnimateToPoint(tData.nGaugeFrontDestPoint, nTime)
end

function StopPursuitGauge(oWidget)
  oWidget.CustomData.bPursuitActive = false
  local tData = oWidget.CustomData
  local oPursuit = tData.oPursuit
  oPursuit:AnimateToPoint(oPursuit.CustomData.nRedPoint, 0, true)
  oPursuit:SetVisible(false)
  tData.fCallback = nil
  tData.tCallbackData = nil
  tData.nTotalTime = nil
  oPursuit:SetClockAnimationCallback(nil)
  oPursuit:SetClockAnimation(1, 1, false, true)
  tData.oGaugeFront:SetColor(255, 255, 255)
  _SnapBarToValue(oWidget, tData.nCurrentValue)
  oWidget:SetValue(tData.nCurrentMood, true)
  tData.oMood:SetText(_tLevelNames[tData.nCurrentLevel])
end

function IsPursuitActive(oWidget)
  return oWidget.CustomData.bPursuitActive
end

function GetRemainingPursuitTime(oWidget)
  local oPursuit = oWidget.CustomData.oPursuit
  local nElapsedTime = oPursuit:GetClockElapsedTime()
  if oPursuit.CustomData.nTotalTime and nElapsedTime then
    return oPursuit.CustomData.nTotalTime - nElapsedTime
  end
  return 0
end

function _LoopToRed(oPursuit)
  oPursuit:AnimateToPoint(oPursuit.CustomData.nRedPoint, 0.25, true, _LoopToBase)
end

function _LoopToBase(oPursuit)
  oPursuit:AnimateToPoint(oPursuit.CustomData.nBasePoint, 0.25, true, _LoopToRed)
end

function _PursuitAnimationComplete(oWidget)
  local oPursuit = oWidget.CustomData.oPursuit
  local fCallback = oPursuit.CustomData.fCallback
  local tCallbackData = oPursuit.CustomData.tCallbackData
  StopPursuitGauge(oWidget)
  if "function" == type(fCallback) then
    if "table" ~= type(tCallbackData) then
      tCallbackData = {}
    end
    fCallback(unpack(tCallbackData))
  end
end

function _SetVisible(oWidget, bVisible, bShowTimer, bShowPursuit)
  if bVisible then
    if bShowTimer then
      oWidget:_RealSetVisible(true)
      oWidget.CustomData.oIcon:SetVisible(true)
      oWidget.CustomData.oTimer:SetVisible(true)
      oWidget.CustomData.oPursuit:SetVisible(false)
      oWidget.CustomData.oMood:SetVisible(false)
    elseif bShowPursuit then
      oWidget:_RealSetVisible(true)
      oWidget.CustomData.oIcon:SetVisible(true)
      oWidget.CustomData.oTimer:SetVisible(false)
      oWidget.CustomData.oPursuit:SetVisible(false)
      oWidget.CustomData.oMood:SetVisible(true)
    else
      oWidget:_RealSetVisible(true)
      oWidget.CustomData.oPursuit:SetVisible(false)
      if oWidget.CustomData.oTimer:IsActive() then
        oWidget.CustomData.oTimer:SetVisible(true)
        oWidget.CustomData.oMood:SetVisible(false)
      else
        oWidget.CustomData.oTimer:SetVisible(false)
        oWidget.CustomData.oMood:SetVisible(true)
      end
    end
    if oWidget.CustomData.bPursuitActive then
      oWidget.CustomData.oPursuit:SetVisible(false)
    end
  else
    oWidget:_RealSetVisible(bVisible)
  end
end

function _Min(nA, nB)
  if nA < nB then
    return nA
  end
  return nB
end

function _Max(nA, nB)
  if nB < nA then
    return nA
  end
  return nB
end

function _Clamp(n, nMin, nMax)
  if nMax < n then
    n = nMax
  end
  if nMin > n then
    n = nMin
  end
  return n
end

function _Abs(n)
  if n < 0 then
    return -1 * n
  end
  return n
end

function _Animate(oUnused, oWidget, nPoint, nTime, bImmediate, fCallback, tCallbackData)
  oWidget:AnimateToPoint(nPoint, nTime, bImmediate, fCallback, tCallbackData)
end

function _InitializeFactionTimer(oTimer)
  oTimer:SetText("00:00:00")
  oTimer:SetVisible(false)
  oTimer.CustomData.nTime = 0
  oTimer.CustomData.bEnabled = false
  oTimer.SetCallback = SetFactionTimerCallback
  oTimer.Start = StartFactionTimer
  oTimer.Stop = StopFactionTimer
  oTimer.IsActive = IsActive
  oTimer._Initialize = _InitializeFactionTimer
  oTimer:Stop()
end

function SetFactionTimerCallback(oTimer, fCallback, tData)
  oTimer.CustomData.fCallback = fCallback
  if "table" == type(tData) then
    oTimer.CustomData.tCallbackData = tData
  end
end

function StartFactionTimer(oTimer, nTime)
  oTimer:SetEventHandler("GuiUpdate", _UpdateTimer)
  oTimer.CustomData.bEnabled = true
  oTimer.CustomData.nTime = nTime
  oTimer:SetVisible(true)
end

function StopFactionTimer(oTimer)
  oTimer:SetEventHandler("GuiUpdate", nil)
  oTimer.CustomData.bEnabled = false
  oTimer:SetVisible(false)
end

function _UpdateTimer(oTimer, nTime)
  oTimer.CustomData.nTime = oTimer.CustomData.nTime - nTime
  if oTimer.CustomData.nTime <= 0 then
    oTimer:Stop()
    oTimer:SetText("00:00:00")
    if "function" == type(oTimer.CustomData.fCallback) then
      local tData = {}
      if "table" == type(oTimer.CustomData.tCallbackData) then
        tData = oTimer.CustomData.tCallbackData
      end
      local fCallback = oTimer.CustomData.fCallback
      oTimer.CustomData.fCallback = nil
      oTimer.CustomData.tCallbackData = nil
      fCallback(unpack(tData))
    end
    return
  end
  nTime = oTimer.CustomData.nTime
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
  oTimer:SetText(sMinutes .. ":" .. sSeconds .. ":" .. sMseconds)
end

function IsActive(oWidget)
  return oWidget.CustomData.bEnabled
end
