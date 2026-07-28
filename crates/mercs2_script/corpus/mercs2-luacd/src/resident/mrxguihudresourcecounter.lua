import("MrxGui")
import("MrxPmc")
_kTickUpSound = "ui_HUD_Money_Gain"
_kTickDownSound = "ui_HUD_Money_Lose"
_kDefaultTickTime = 0.5
_kTickLong = 2.05
_kTickMedium = 1.25
_kTickShort = 0.5
_kPulseTime = 0.2
_kWindowTime = 0.1

function GetCounterValue(oSelf)
  return oSelf.CustomData.nCurrentValue
end

function SetCounterValue(oSelf, nNewValue, sReason, nIncrement)
  if "number" ~= type(nNewValue) then
    return false
  end
  if nNewValue == oSelf.CustomData.nCurrentValue and (not nIncrement or 0 == nIncrement) then
    return false
  end
  if oSelf.CustomData.bSuppressed then
    oSelf.CustomData.nDisplayValue = nNewValue
    oSelf.CustomData.nCurrentValue = nNewValue
    oSelf.CustomData.nDeltaValue = 0
    oSelf.CustomData.bActive = false
    oSelf.CustomData.nMagnitude = 0
    oSelf:SetEventHandler("GuiUpdate", nil)
    oSelf:UpdateText()
    return false
  else
    local nChange = nIncrement or nNewValue - oSelf.CustomData.nCurrentValue
    if oSelf.CustomData.oReasonList then
      if "string" == type(sReason) then
        oSelf.CustomData.oReasonList:AddReason(sReason, nChange)
      elseif 0 < nChange then
        oSelf.CustomData.oReasonList:AddReason("[Generic.MoneyReasons.Credit]", nChange)
      else
        oSelf.CustomData.oReasonList:AddReason("[Generic.MoneyReasons.Debit]", nChange)
      end
    end
    if 0 < nChange then
      oSelf:PulseRise(_kPulseTime)
    elseif nChange < 0 then
      oSelf:PulseFall(_kPulseTime)
    end
    local nMagnitude = 0
    local sSound, nNewTickSpeed
    if 0 < nChange then
      if 1000000 < nChange then
        sSound = "UI_hud_cashUp_large"
        nNewTickSpeed = _kTickLong
        nMagnitude = 3
      elseif 100000 < nChange then
        sSound = "UI_hud_cashUp_med"
        nNewTickSpeed = _kTickMedium
        nMagnitude = 2
      else
        sSound = "UI_hud_cashUp_small"
        nNewTickSpeed = _kTickShort
        nMagnitude = 1
      end
    elseif nChange < 0 then
      if nChange < -1000000 then
        sSound = "UI_hud_cashDown_large"
        nNewTickSpeed = _kTickLong
        nMagnitude = -3
      elseif nChange < -100000 then
        sSound = "UI_hud_cashDown_med"
        nNewTickSpeed = _kTickMedium
        nMagnitude = -2
      else
        sSound = "UI_hud_cashDown_small"
        nNewTickSpeed = _kTickShort
        nMagnitude = -1
      end
    end
    local bUpdate = false
    if sSound then
      if math.abs(nMagnitude) > math.abs(oSelf.CustomData.nMagnitude) then
        bUpdate = true
      elseif math.abs(nMagnitude) == math.abs(oSelf.CustomData.nMagnitude) and nMagnitude ~= oSelf.CustomData.nMagnitude then
        bUpdate = true
      end
    end
    if bUpdate then
      Sound.CueSound(0, sSound)
      oSelf.CustomData.nMagnitude = nMagnitude
      oSelf.CustomData.nTickSpeed = nNewTickSpeed
    end
    oSelf.CustomData.nDeltaValue = (nNewValue - oSelf.CustomData.nDisplayValue) / oSelf.CustomData.nTickSpeed
    oSelf.CustomData.nCurrentValue = nNewValue
    oSelf.CustomData.bActive = true
    if oSelf.CustomData.bPersistWhenLow then
      local nThreshold = (MrxPmc.GetFuelCapacity() or 300) * 0.1
      if 0 < nChange and nThreshold < oSelf.CustomData.nCurrentValue then
        oSelf.CustomData.bPersist = false
      elseif nChange < 0 and nThreshold >= oSelf.CustomData.nCurrentValue then
        oSelf.CustomData.bPersist = true
      end
    end
    oSelf:SetEventHandler("GuiUpdate", _HandleCounterUpdateEvent)
    return true
  end
end

function ModifyCounterValue(oSelf, nDeltaValue)
  oSelf:SetValue(oSelf:GetValue() + nDeltaValue)
end

function SetCounterAppendedString(oSelf, sAppendedString)
  if nil == sAppendedString then
    oSelf.CustomData.sAppend = nil
  elseif "string" == type(sAppendedString) then
    oSelf.CustomData.sAppend = sAppendedString
  end
  oSelf:UpdateText()
end

function SetCounterTickSpeed(oSelf, nSpeed)
  if "number" == type(nSpeed) then
    oSelf.CustomData.nTickSpeed = nSpeed
  end
end

function IsTicking(oSelf)
  return oSelf.CustomData.bActive
end

function _HandleCounterUpdateEvent(oSelf, nDeltaTime)
  if oSelf.CustomData.bActive then
    oSelf.CustomData.nDisplayValue = oSelf.CustomData.nDisplayValue + oSelf.CustomData.nDeltaValue * nDeltaTime
    local bDone = false
    if oSelf.CustomData.nDeltaValue < 0 then
      if oSelf.CustomData.nDisplayValue <= oSelf.CustomData.nCurrentValue then
        bDone = true
      end
    elseif oSelf.CustomData.nDeltaValue > 0 then
      if oSelf.CustomData.nDisplayValue >= oSelf.CustomData.nCurrentValue then
        bDone = true
      end
    else
      bDone = true
    end
    if bDone then
      if oSelf.CustomData.bPersist then
        if oSelf.CustomData.nDeltaValue > 0 then
          oSelf:PulseFall(_kPulseTime)
        end
      else
        oSelf:HaltPulse(_kPulseTime)
      end
      oSelf.CustomData.nDisplayValue = oSelf.CustomData.nCurrentValue
      oSelf.CustomData.nDeltaValue = 0
      oSelf.CustomData.bActive = false
      oSelf.CustomData.nMagnitude = 0
    end
    oSelf:UpdateText()
  end
end

function _CounterInitialization(oSelf)
  oSelf.CustomData.nCurrentValue = 0
  oSelf.CustomData.nDisplayValue = 0
  oSelf.CustomData.nTickSpeed = _kDefaultTickTime
  oSelf.CustomData.nDeltaValue = 1
  oSelf.CustomData.bActive = false
  oSelf.CustomData.sAppend = nil
  oSelf.CustomData.bSuppressed = false
  oSelf.CustomData.nMagnitude = 0
  oSelf.GetValue = GetCounterValue
  oSelf.SetValue = SetCounterValue
  oSelf.ModifyValue = ModifyCounterValue
  oSelf.SetTickSpeed = SetCounterTickSpeed
  oSelf.SetAppendedString = SetCounterAppendedString
  oSelf.UpdateText = _UpdateText
  oSelf.IsTicking = IsTicking
  oSelf.HaltPulse = HaltPulse
  oSelf.PulseRise = PulseRise
  oSelf.PulseFall = PulseFall
  oSelf:UpdateText()
  local nR, nG, nB = oSelf:GetColor()
  oSelf.CustomData.nBaseColor = oSelf:AddAnimationPoint({
    RedLevel = nR,
    GreenLevel = nG,
    BlueLevel = nB
  })
  oSelf.CustomData.nFallColor = oSelf:AddAnimationPoint({
    RedLevel = 255,
    GreenLevel = 64,
    BlueLevel = 64
  })
  oSelf.CustomData.nRiseColor = oSelf:AddAnimationPoint({
    RedLevel = 255,
    GreenLevel = 255,
    BlueLevel = 255
  })
  if oSelf.BasicData.name == "Money Counter" then
    local oReasonList = oSelf.ParentWidget.ParentWidget:GetChildren()[3]
    oReasonList.CustomData.bFormatMoney = true
    oSelf.CustomData.oReasonList = oReasonList
    oSelf.CustomData.bFormatMoney = true
  elseif oSelf.BasicData.name == "Fuel Counter" then
    local oReasonList = oSelf.ParentWidget.ParentWidget:GetChildren()[4]
    oReasonList.CustomData.nBufferSize = 0
    oSelf.CustomData.bPersistWhenLow = true
    oSelf.CustomData.oReasonList = oReasonList
  end
  oSelf:UpdateText()
end

function PulseRise(oCounter, nTime)
  _PulseToRiseRise(oCounter, nTime or _kPulseTime)
end

function PulseFall(oCounter, nTime)
  _PulseToFallFall(oCounter, nTime or _kPulseTime)
end

function HaltPulse(oCounter, nTime)
  oCounter:AnimateToPoint(oCounter.CustomData.nBaseColor, nTime or _kPulseTime, true)
end

function _PulseToBaseRise(oCounter, nTime)
  oCounter:AnimateToPoint(oCounter.CustomData.nBaseColor, nTime, true, _PulseToRiseRise, {nTime})
end

function _PulseToRiseRise(oCounter, nTime)
  oCounter:AnimateToPoint(oCounter.CustomData.nRiseColor, nTime, true, _PulseToBaseRise, {nTime})
end

function _PulseToBaseFall(oCounter, nTime)
  oCounter:AnimateToPoint(oCounter.CustomData.nBaseColor, nTime, true, _PulseToFallFall, {nTime})
end

function _PulseToFallFall(oCounter, nTime)
  oCounter:AnimateToPoint(oCounter.CustomData.nFallColor, nTime, true, _PulseToBaseFall, {nTime})
end

function _TopLevelInitialization(oSelf)
  oSelf.CustomData.bTicking = false
  oSelf.CustomData.nVisibleTime = 0
  local tChildren = oSelf:GetChildren()
  local oBg1 = tChildren[1]
  local oBg2 = tChildren[2]
  local oCounter = tChildren[3]
  oSelf.CustomData.oBg1 = oBg1
  oSelf.CustomData.oBg2 = oBg2
  oSelf.CustomData.oCounter = oCounter
  local nX1, nY1, nX2, nY2 = oBg1:GetLocation()
  local nMidY = (nY1 + nY2) * 0.5
  oSelf.CustomData.nOpenPoint1 = oBg1:AddAnimationPoint({
    x = nX1,
    y = nY1,
    x2 = nX2,
    y2 = nY2
  })
  oSelf.CustomData.nClosePoint1 = oBg1:AddAnimationPoint({
    x = nX1,
    y = nMidY,
    x2 = nX2,
    y2 = nMidY
  })
  nX1, nY1, nX2, nY2 = oBg2:GetLocation()
  nMidY = (nY1 + nY2) * 0.5
  oSelf.CustomData.nOpenPoint2 = oBg2:AddAnimationPoint({
    x = nX1,
    y = nY1,
    x2 = nX2,
    y2 = nY2
  })
  oSelf.CustomData.nClosePoint2 = oBg2:AddAnimationPoint({
    x = nX1,
    y = nMidY,
    x2 = nX2,
    y2 = nMidY
  })
  local tShakePoints = {}
  nX1, nY1 = oSelf:GetLocation()
  tShakePoints[0] = oSelf:AddAnimationPoint({x = nX1, y = nY1})
  tShakePoints[1] = oSelf:AddAnimationPoint({
    x = nX1 - 1.5,
    y = nY1 - 1
  })
  tShakePoints[2] = oSelf:AddAnimationPoint({
    x = nX1 + 2,
    y = nY1 - 1.5
  })
  tShakePoints[3] = oSelf:AddAnimationPoint({
    x = nX1 - 1,
    y = nY1 + 1
  })
  tShakePoints[4] = oSelf:AddAnimationPoint({
    x = nX1 + 2,
    y = nY1 + 2
  })
  oSelf.CustomData.nShakePoint = 1
  oSelf.CustomData.tShakePoints = tShakePoints
  oSelf.Show = _Show
  oSelf.Hide = _Hide
  oSelf.SetValue = TopLevelSetValue
  oSelf.SetAppendedString = TopLevelSetAppendedString
  oSelf.SetSuppressed = SetSuppressed
  oSelf.CustomData.bActive = true
  oSelf:Hide()
end

function _Show(oSelf, nTime)
  if oSelf.CustomData.bSuppressed then
    return
  end
  if not oSelf.CustomData.bActive then
    oSelf:SetVisible(true)
    oSelf.CustomData.oCounter:SetVisible(false)
    oSelf.CustomData.oBg1:AnimateToPoint(oSelf.CustomData.nOpenPoint1, _kWindowTime, true, _FinishOpen, {oSelf})
    oSelf.CustomData.oBg2:AnimateToPoint(oSelf.CustomData.nOpenPoint2, _kWindowTime, true)
  end
  oSelf.CustomData.bActive = true
  if oSelf.CustomData.nVisibleTime > -1 then
    oSelf.CustomData.nVisibleTime = nTime or 2
  end
end

function _FinishOpen(oBg, oSelf)
  oSelf:SetVisible(true)
  oSelf:SetEventHandler("GuiUpdate", _TopLevelUpdate)
end

function _Hide(oSelf)
  if not oSelf.CustomData.bActive then
    return
  end
  if oSelf.CustomData.oCounter.CustomData.bPersist then
    return
  end
  oSelf.CustomData.bActive = false
  oSelf.CustomData.oCounter:SetVisible(false)
  oSelf.CustomData.oBg1:AnimateToPoint(oSelf.CustomData.nClosePoint1, _kWindowTime, true, _FinishClose, {oSelf})
  oSelf.CustomData.oBg2:AnimateToPoint(oSelf.CustomData.nClosePoint2, _kWindowTime, true)
  oSelf:AnimateToPoint(oSelf.CustomData.tShakePoints[0], 0, true)
  oSelf.CustomData.nMagnitude = 0
  if oSelf.CustomData.oCounter.HaltPulse then
    oSelf.CustomData.oCounter:HaltPulse(0)
  end
  oSelf.CustomData.nVisibleTime = 0
  oSelf:SetEventHandler("GuiUpdate", nil)
end

function _FinishClose(oBg, oSelf)
  oSelf:SetVisible(false)
end

function SetSuppressed(oSelf, bSuppress)
  bSuppress = bSuppress or false
  bSuppress = bSuppress and true
  if oSelf.CustomData.bSuppressed == bSuppress then
    return
  end
  oSelf.CustomData.bSuppressed = bSuppress
  oSelf.CustomData.oCounter.CustomData.bSuppressed = bSuppress
  if bSuppress then
    oSelf:Hide()
  end
end

function TopLevelSetValue(oSelf, nValue, sReason, nIncrement)
  if oSelf.CustomData.oCounter:SetValue(nValue, sReason, nIncrement) then
    oSelf:Show()
  end
end

function TopLevelSetAppendedString(oSelf, sString)
  oSelf.CustomData.oCounter:SetAppendedString(sString)
end

function _TopLevelUpdate(oSelf, nDeltaTime)
  local bTicking = oSelf.CustomData.oCounter:IsTicking()
  if bTicking and not oSelf.AnimationData.bAnimating then
    oSelf:AnimateToPoint(oSelf.CustomData.tShakePoints[oSelf.CustomData.nShakePoint], 0.05, true)
    oSelf.CustomData.nShakePoint = oSelf.CustomData.nShakePoint + 1
    if oSelf.CustomData.nShakePoint > 4 then
      oSelf.CustomData.nShakePoint = 1
    end
  elseif not bTicking and oSelf.CustomData.bTicking then
    oSelf:AnimateToPoint(oSelf.CustomData.tShakePoints[0], 0, true)
  end
  oSelf.CustomData.bTicking = bTicking
  if not oSelf.CustomData.bTicking and 0 < oSelf.CustomData.nVisibleTime and not oSelf.CustomData.bAlwaysShow and not oSelf.CustomData.oCounter.CustomData.bPersist then
    oSelf.CustomData.nVisibleTime = oSelf.CustomData.nVisibleTime - nDeltaTime
    if 0 >= oSelf.CustomData.nVisibleTime then
      oSelf.CustomData.nVisibleTime = 0
      oSelf:Hide()
    end
  end
end

function _HandleShowEvent(oSelf, tEvent)
  oSelf:Show(tEvent.nTime or 2)
end

function _HandleSetValueEvent(oSelf, tEvent)
  if oSelf:GetOwner() == tEvent.uGuid or nil == tEvent.uGuid then
    oSelf:SetValue(tEvent.nValue)
  end
end

function _UpdateText(oWidget)
  if oWidget.CustomData.bFormatMoney then
    oWidget:SetText(_ConvertNumber(oWidget.CustomData.nDisplayValue))
  else
    oWidget:SetText(string.format("%d", oWidget.CustomData.nDisplayValue) .. (oWidget.CustomData.sAppend or " "))
  end
end

function InitReasonList(oList)
  oList.CustomData.nDisplayTime = 0
  oList.CustomData.tReasons = {}
  oList.CustomData.nBufferSize = oList.CustomData.nBufferSize or _knBufferSize
  oList.CustomData.nShowPoint = oList:AddAnimationPoint({TranslucencyLevel = 255})
  oList.CustomData.nHidePoint = oList:AddAnimationPoint({TranslucencyLevel = 0})
  oList.AddReason = AddReason
  oList.Hide = _FadeReasons
  oList:AnimateToPoint(oList.CustomData.nHidePoint, 0, true, _ClearReasons)
end

function AddReason(oList, sReason, nAmount)
  table.insert(oList.CustomData.tReasons, {nAmount, sReason})
  _UpdateReasonDisplay(oList)
  oList:AnimateToPoint(oList.CustomData.nShowPoint, 0, true, _Delay)
end

function _Delay(oList)
  oList:AnimateToPoint(oList.CustomData.nShowPoint, 2, true, _FadeReasons)
end

function _FadeReasons(oList)
  oList:AnimateToPoint(oList.CustomData.nHidePoint, 0.5, true, _ClearReasons)
end

function _ClearReasons(oList)
  oList.CustomData.tReasons = {}
  oList:SetText(" ")
end

_knBufferSize = 4

function _UpdateReasonDisplay(oList)
  local nTotal = 0
  local sReasonList = ""
  local sSymbol
  local nFirstDisplayItem = 1
  local nItems = #oList.CustomData.tReasons
  if nItems > oList.CustomData.nBufferSize then
    nFirstDisplayItem = nItems - oList.CustomData.nBufferSize + 1
  end
  for nIndex, tData in pairs(oList.CustomData.tReasons) do
    nTotal = nTotal + tData[1]
    if nIndex >= nFirstDisplayItem then
      if 0 < tData[1] then
        sSymbol = "+"
      elseif 0 > tData[1] then
        sSymbol = "-"
      else
        sSymbol = " "
      end
      if oList.CustomData.bFormatMoney then
        sReasonList = sReasonList .. tData[2] .. " (" .. sSymbol .. _ConvertNumber(math.abs(tData[1])) .. ")[n]"
      else
        sReasonList = sReasonList .. tData[2] .. " (" .. sSymbol .. math.abs(tData[1]) .. ")[n]"
      end
    end
  end
  local sColor = ""
  if 0 < nTotal then
    sColor = "[green]"
    sSymbol = "+"
  elseif nTotal < 0 then
    sColor = "[red]"
    sSymbol = "-"
  else
    sColor = "[white]"
    sSymbol = " "
  end
  if oList.CustomData.bFormatMoney then
    sReasonList = sColor .. sSymbol .. _ConvertNumber(math.abs(nTotal)) .. "[n][white]" .. sReasonList
  else
    sReasonList = sColor .. sSymbol .. math.abs(nTotal) .. "[n][white]" .. sReasonList
  end
  oList:SetText(sReasonList)
end

_tNumbers = false

function _ConvertNumber(n)
  local nFactor = 1000
  local sSuffix = "[0xe00c096a]"
  if 1.0E15 < n then
    n = 1.0E15
  end
  if n < 0 then
    n = 0
  end
  for nTestFactor, sTestSuffix in pairs(_tNumbers) do
    if nTestFactor > nFactor and nTestFactor <= n then
      nFactor = nTestFactor
      sSuffix = sTestSuffix
    end
  end
  local s = string.format("[SHELL.Common.Money:%d:%d:%s]", n / nFactor, 10 * (n / nFactor - math.floor(n / nFactor)), sSuffix)
  return s
end

function Init()
  _tNumbers = {}
  _tNumbers[1.0E15] = "[0x7be2637c]"
  _tNumbers[1.0E12] = "[0x9d96ba8f]"
  _tNumbers[1000000000] = "[0x4cf9c95f]"
  _tNumbers[1000000] = "[0xcd15e5e8]"
  _tNumbers[1000] = "[0xe00c096a]"
end
