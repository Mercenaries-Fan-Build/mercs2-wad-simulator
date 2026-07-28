import("MrxGui")
_knNumSlots = 2

function Initialize(oWidget)
  local oTemplateGauge = oWidget:GetChildren()[1]
  local nX1, nY1, nX2, nY2 = oWidget:GetLocation()
  local tData = oWidget.CustomData
  local nGaugeX1, nGaugeY1, nGaugeX2, nGaugeY2 = oTemplateGauge:GetLocation()
  local nSlotHeight = nGaugeY2 - nGaugeY1
  tData.tSlotLife = {}
  tData.tSlotPointData = {}
  tData.tSlotOccupants = {}
  tData.tFactionGauges = {}
  local n = 1
  while n <= _knNumSlots do
    tData.tSlotLife[n] = 0
    tData.tSlotPointData[n] = {
      x = nX1,
      y = nY1 + (n - 1) * nSlotHeight,
      TranslucencyLevel = 255
    }
    n = n + 1
  end
  tData.tSlotPointData[_knNumSlots + 1] = {
    x = nX1,
    y = nY2,
    TranslucencyLevel = 0
  }
  tData.oTemplateGauge = oTemplateGauge
  MrxGui.RemoveWidgetWithChildren(tData.oTemplateGauge)
  oWidget:SetEventHandler("GuiUpdate", _Update)
  oWidget.AddFactionGauge = AddFactionGauge
  oWidget.SetInsideFactionZone = SetInsideFactionZone
  oWidget.SetValue = SetValue
  oWidget.ShowAll = ShowAll
  oWidget.StartTimer = StartTimer
  oWidget.StartPursuit = StartPursuit
  oWidget.HideGauge = HideGauge
end

function AddFactionGauge(oWidget, sFactionName, sTexture)
  if "string" ~= type(sFactionName) then
    return
  end
  if "string" ~= type(sTexture) then
    sTexture = nil
  end
  local tData = oWidget.CustomData
  local oGauge = tData.oTemplateGauge:Duplicate()
  oGauge:_Initialize()
  oGauge:SetIcon(sTexture)
  oGauge.CustomData.oTimer:_Initialize()
  oGauge.CustomData.tSlotPoints = {}
  local n = 1
  while n <= _knNumSlots + 1 do
    oGauge.CustomData.tSlotPoints[n] = oGauge:AddAnimationPoint(tData.tSlotPointData[n])
    n = n + 1
  end
  oWidget:AddChild(oGauge)
  tData.tFactionGauges[sFactionName] = oGauge
  oGauge:SetVisible(false)
  if oWidget.BasicData.bEnabled then
    MrxGui.AddWidgetWithChildren(oGauge)
  end
  oGauge:AnimateToPoint(oGauge.CustomData.tSlotPoints[_knNumSlots + 1], 0, true, oGauge.SetVisible, false)
end

function SetInsideFactionZone(oWidget, sFactionName, bInside, bInitialize)
end

function SetValue(oWidget, sFactionName, nLevel, bInitialize)
  local tData = oWidget.CustomData
  local oGauge = tData.tFactionGauges[sFactionName]
  if not oGauge then
    return
  end
  local nDeltaValue = math.abs(oGauge:GetValue() - nLevel)
  if nDeltaValue < 3 then
    bInitialize = true
  end
  local nTime = 0.25
  if bInitialize then
    nTime = 0
  end
  oGauge:SetValue(nLevel, bInitialize)
  if not bInitialize then
    local nSlot
    local bActive = false
    nSlot, bActive = _FindSlot(oWidget, sFactionName)
    local nPreviousTime = 0
    if tData.tSlotOccupants[nSlot] and not bActive then
      local oOldGauge = tData.tSlotOccupants[nSlot]
      oOldGauge:AnimateToPoint(oOldGauge.CustomData.tSlotPoints[_knNumSlots + 1], nTime, true, oOldGauge.SetVisible, false)
    end
    if tData.tSlotOccupants[nSlot] == oGauge then
      nPreviousTime = tData.tSlotLife[nSlot]
    end
    tData.tSlotLife[nSlot] = math.max(5, nPreviousTime)
    tData.tSlotOccupants[nSlot] = oGauge
    oGauge:SetVisible(true)
    if not bActive then
      oGauge:AnimateToPoint(oGauge.CustomData.tSlotPoints[nSlot], nTime, true)
    end
    local oTray = MrxGui.GetWidgetByNameAndOwner("Objective Tray", oWidget:GetOwner())
    if oTray and oTray:IsSlotOccupied(3) and oTray:GetVisible() then
      oTray:SetVisible(false)
      oWidget.CustomData.bTrayDisabled = true
    end
  end
end

function StartTimer(oWidget, sFactionName, nTime, fFunction, tCallbackData)
  local tData = oWidget.CustomData
  local oGauge = tData.tFactionGauges[sFactionName]
  if not oGauge then
    return
  end
  local nLifeTime = 0.25
  if bInitialize then
    nLifeTime = 0
  end
  local nSlot
  local bActive = false
  nSlot, bActive = _FindSlot(oWidget, sFactionName)
  if tData.tSlotOccupants[nSlot] and not bActive then
    local oOldGauge = tData.tSlotOccupants[nSlot]
    oOldGauge:AnimateToPoint(oOldGauge.CustomData.tSlotPoints[_knNumSlots + 1], nLifeTime, true, oOldGauge.SetVisible, false)
  end
  oGauge:StartTimer(nTime, fFunction, tCallbackData)
  tData.tSlotLife[nSlot] = 5 + nTime
  tData.tSlotOccupants[nSlot] = oGauge
  oGauge:SetVisible(true, true)
  if not bActive then
    oGauge:AnimateToPoint(oGauge.CustomData.tSlotPoints[nSlot], nLifeTime, true)
  end
  local oTray = MrxGui.GetWidgetByNameAndOwner("Objective Tray", oWidget:GetOwner())
  if oTray and oTray:IsSlotOccupied(3) and oTray:GetVisible() then
    oTray:SetVisible(false)
    oWidget.CustomData.bTrayDisabled = true
  end
end

function StartPursuit(oWidget, sFactionName, nTime, fFunction, tCallbackData)
  local tData = oWidget.CustomData
  local oGauge = tData.tFactionGauges[sFactionName]
  if not oGauge then
    return
  end
  local nLifeTime = 0.25
  if bInitialize then
    nLifeTime = 0
  end
  local nSlot
  local bActive = false
  nSlot, bActive = _FindSlot(oWidget, sFactionName)
  if tData.tSlotOccupants[nSlot] and not bActive then
    local oOldGauge = tData.tSlotOccupants[nSlot]
    oOldGauge:AnimateToPoint(oOldGauge.CustomData.tSlotPoints[_knNumSlots + 1], nLifeTime, true, oOldGauge.SetVisible, false)
  end
  oGauge:StartPursuit(nTime, fFunction, tCallbackData)
  tData.tSlotOccupants[nSlot] = oGauge
  nTime = -3
  if not bActive or 0 >= tData.tSlotLife[nSlot] then
    tData.tSlotLife[nSlot] = 2 + nTime
  end
  if not bActive then
    oGauge:AnimateToPoint(oGauge.CustomData.tSlotPoints[nSlot], nLifeTime, true)
  end
  local oTray = MrxGui.GetWidgetByNameAndOwner("Objective Tray", oWidget:GetOwner())
  if oTray and oTray:IsSlotOccupied(3) and oTray:GetVisible() then
    oTray:SetVisible(false)
    oWidget.CustomData.bTrayDisabled = true
  end
end

function HideGauge(oWidget, sFactionName)
  local tData = oWidget.CustomData
  local oGauge = tData.tFactionGauges[sFactionName]
  if not oGauge then
    return
  end
  local nSlot
  local bActive = false
  nSlot, bActive = _FindSlot(oWidget, sFactionName)
  if bActive then
    tData.tSlotOccupants[nSlot] = nil
    tData.tSlotLife[nSlot] = 0
    oGauge:SetTranslucency(oGauge:GetTranslucency())
    oGauge:AnimateToPoint(oGauge.CustomData.tSlotPoints[_knNumSlots + 1], 0.25, true, oGauge.SetVisible, {false})
    oGauge:StopTimer()
    oGauge:StopPursuit()
  end
end

function ModifyFactionMood(oWidget, sFactionName, nLevel, bInitialize)
end

function ShowAll(oWidget, nDuration)
end

function _Update(oWidget, nDeltaTime)
  nDeltaTime = nDeltaTime or 0
  local tData = oWidget.CustomData
  for nSlot, nAge in pairs(tData.tSlotLife) do
    if 0 < nAge then
      tData.tSlotLife[nSlot] = nAge - nDeltaTime
      if 0 >= tData.tSlotLife[nSlot] then
        local oGauge = tData.tSlotOccupants[nSlot]
        if oGauge:IsPursuitActive() then
          oGauge:SetVisible(true, false, true)
          tData.tSlotLife[nSlot] = oGauge:GetRemainingPursuitTime() + 2
        else
          oGauge:SetTranslucency(oGauge:GetTranslucency())
          oGauge:AnimateToPoint(oGauge.CustomData.tSlotPoints[_knNumSlots + 1], 0.25, true, oGauge.SetVisible, {false})
          if oGauge.CustomData.bForceIcon then
            oGauge:SetIconVisible(true, 255)
          end
          tData.tSlotOccupants[nSlot] = nil
        end
      end
    end
  end
  if tData.bTrayDisabled and _IsBufferEmpty(oWidget) then
    local oTray = MrxGui.GetWidgetByNameAndOwner("Objective Tray", oWidget:GetOwner())
    if oTray then
      oTray:SetVisible(true)
      tData.bTrayDisabled = false
    end
  end
end

function _FindSlot(oWidget, sFactionName)
  local tData = oWidget.CustomData
  local oGauge = tData.tFactionGauges[sFactionName]
  if not oGauge then
    return
  end
  local nSlot
  local bActive = false
  local nIndex = 1
  while nIndex <= _knNumSlots do
    local oOccupant = tData.tSlotOccupants[nIndex]
    if oGauge == oOccupant then
      nSlot = nIndex
      bActive = true
    end
    if not bActive and not nSlot and not oOccupant then
      nSlot = nIndex
    end
    nIndex = nIndex + 1
  end
  if not nSlot then
    local nMinLife = 99999
    for nSlotIndex, nLife in pairs(tData.tSlotLife) do
      if nLife < nMinLife then
        nMinLife = nLife
        nSlot = nSlotIndex
      end
    end
  end
  ASSERT(nSlot)
  return nSlot, bActive
end

function _IsBufferEmpty(oWidget)
  for nSlot, oOccupant in pairs(oWidget.CustomData.tSlotOccupants) do
    if oOccupant then
      return false
    end
  end
  return true
end
