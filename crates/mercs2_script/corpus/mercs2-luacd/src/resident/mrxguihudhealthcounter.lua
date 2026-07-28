_knShowTime = 2
_knPulsingThreshold = 20
_knVisibleThreshold = 100
_knPulseTime = 0.4
local _tDefaultIcon = {
  texture = "global_gui_hud02",
  u1 = 0.443359,
  v1 = 0.466797,
  u2 = 0.552734,
  v2 = 0.576172
}
local _tArmorToIcon = {
  ArmorVehicle = {
    texture = "HUD_vehicle_armor_1",
    u1 = 0,
    v1 = 0,
    u2 = 1,
    v2 = 1
  },
  ArmorLight = {
    texture = "HUD_vehicle_armor_2",
    u1 = 0,
    v1 = 0,
    u2 = 1,
    v2 = 1
  },
  ArmorMedium = {
    texture = "HUD_vehicle_armor_3",
    u1 = 0,
    v1 = 0,
    u2 = 1,
    v2 = 1
  },
  ArmorTank = {
    texture = "HUD_vehicle_armor_4",
    u1 = 0,
    v1 = 0,
    u2 = 1,
    v2 = 1
  }
}

function HandleHealthChangedEventNew(oWidget, nCurHealth, nMaxHealth, bVehicle)
  local NewLifeValue = 100 * nCurHealth / nMaxHealth
  if NewLifeValue < 1 and 0 < NewLifeValue then
    NewLifeValue = 1
  end
  oWidget:SetText(string.format("%d", NewLifeValue))
  local tData = oWidget.CustomData
  local bTransition = tData.bVehicle ~= bVehicle
  if not tData.nPreviousValue then
    tData.nPreviousValue = 0
  end
  if tData.nPreviousValue > 20 and NewLifeValue <= 20 then
    oWidget:SetAnimationPoint(tData.nNeutralPoint, {
      RedLevel = 128,
      GreenLevel = 16,
      BlueLevel = 16
    })
  elseif tData.nPreviousValue <= 20 and 20 < NewLifeValue then
    oWidget:SetAnimationPoint(tData.nNeutralPoint, {
      RedLevel = tData.nR,
      GreenLevel = tData.nG,
      BlueLevel = tData.nB
    })
  end
  if not bTransition then
    if NewLifeValue > tData.nPreviousValue then
      oWidget:SetColor(16, 128, 16)
      oWidget:AnimateToPoint(tData.nNeutralPoint, 1, true)
    elseif NewLifeValue < tData.nPreviousValue then
      oWidget:SetColor(216, 16, 16)
      oWidget:AnimateToPoint(tData.nNeutralPoint, 1, true)
    end
  else
    oWidget:AnimateToPoint(tData.nNeutralPoint, 0, true)
  end
  tData.bVehicle = bVehicle
  tData.nPreviousValue = NewLifeValue
end

function HandleInitializationNew(oWidget)
  local tData = oWidget.CustomData
  local nR, nG, nB, nA = oWidget:GetColor()
  tData.nR = nR
  tData.nG = nG
  tData.nB = nB
  tData.nA = nA
  tData.bVehicle = false
  tData.nNeutralPoint = oWidget:AddAnimationPoint({
    RedLevel = nR,
    GreenLevel = nG,
    BlueLevel = nB
  })
end

function HandleHealthChangedEventMain(oWidget, nCurHealth, nMaxHealth, bVehicle)
  local nNewLifeValue = 100 * nCurHealth / nMaxHealth
  local tData = oWidget.CustomData
  if tData.bHidden then
    tData.bHidden = false
    oWidget:SetVisible(true)
    oWidget:AnimateToPoint(tData.nStartPoint, 0.15, true, _SetCounterVisible, {
      tData.oCounter
    })
  end
  tData.nRemainingTime = _knShowTime
  tData.nCurrentLife = nNewLifeValue
  if nNewLifeValue < _knPulsingThreshold and not tData.bPulsing then
    tData.bPulsing = true
    oWidget:AnimateToPoint(tData.nRedColorPoint, _knPulseTime, false, _LoopToNeutral)
  elseif tData.bPulsing and nNewLifeValue >= _knPulsingThreshold then
    oWidget:AnimateToPoint(tData.nNeutralColorPoint, _knPulseTime, true)
    tData.bPulsing = false
  end
end

function _SetCounterVisible(oUnused, oCounter)
  oCounter:SetVisible(true)
end

function HandleUpdateMain(oWidget, nDeltaTime)
  local tData = oWidget.CustomData
  if not tData.bHidden and tData.nCurrentLife >= _knVisibleThreshold and not tData.bPulsing then
    tData.nRemainingTime = tData.nRemainingTime - nDeltaTime
    if tData.nRemainingTime <= 0 then
      tData.nRemainingTime = _knShowTime
      oWidget.CustomData.bHidden = true
      oWidget:AnimateToPoint(tData.nClosePoint, 0.15, true, oWidget.SetVisible, {false})
      tData.oCounter:SetVisible(false)
    end
  end
end

function _LoopToRed(oWidget)
  oWidget:AnimateToPoint(oWidget.CustomData.nRedColorPoint, _knPulseTime, true, _LoopToNeutral)
end

function _LoopToNeutral(oWidget)
  oWidget:AnimateToPoint(oWidget.CustomData.nNeutralColorPoint, _knPulseTime, true, _LoopToRed)
end

function HandleShowHealthEvent(oWidget, tEvent)
  local tData = oWidget.CustomData
  if tData.bHidden then
    tData.bHidden = false
    oWidget:SetVisible(true)
    oWidget:AnimateToPoint(tData.nStartPoint, 0.15, true, _SetCounterVisible, {
      tData.oCounter
    })
  end
  tData.nRemainingTime = tEvent.nTime or _knShowTime
end

function HandleInitializationMain(oWidget)
  local tData = oWidget.CustomData
  local nX1, nY1, nX2, nY2 = oWidget:GetLocation()
  local nMidY = (nY2 + nY1) * 0.5
  local nR, nG, nB = oWidget:GetColor()
  tData.nStartPoint = oWidget:AddAnimationPoint({
    x = nX1,
    y = nY1,
    x2 = nX2,
    y2 = nY2
  })
  tData.nClosePoint = oWidget:AddAnimationPoint({
    x = nX1,
    y = nMidY,
    x2 = nX2,
    y2 = nMidY,
    RedLevel = nR,
    GreenLevel = nG,
    BlueLevel = nB
  })
  tData.oCounter = oWidget.ParentWidget:GetChildren()[2]
  tData.nRemainingTime = _knShowTime * 5
  tData.nCurrentLife = 100
  tData.nNeutralColorPoint = oWidget:AddAnimationPoint({
    RedLevel = nR,
    GreenLevel = nG,
    BlueLevel = nB
  })
  tData.nRedColorPoint = oWidget:AddAnimationPoint({
    RedLevel = 210,
    GreenLevel = 0,
    BlueLevel = 0
  })
  tData.bPulsing = false
  oWidget:SetEventHandler("GuiUpdate", HandleUpdateMain)
  oWidget:SetEventHandler("ShowAllCounters", HandleShowHealthEvent)
end

function HandleIconUpdateHuman(oWidget, nCurHealth, nMaxHealth, bVehicle)
  oWidget:SetTranslucency(bVehicle and 0 or 255)
end

function SetVehicleIcon(oWidget, tIcon)
  if not tIcon then
    return
  end
  oWidget:SetTexture(tIcon.texture)
  oWidget:SetTextureCoordinates(tIcon.u1, tIcon.v1, tIcon.u2, tIcon.v2)
end

function HandleIconUpdateVehicle(oWidget, nCurHealth, nMaxHealth, bVehicle)
  oWidget:SetTranslucency(bVehicle and 255 or 0)
  if bVehicle then
    local uPlayer = oWidget:GetOwner()
    local uCharacter = Player.GetCharacter(uPlayer)
    local uVehicle = Vehicle.GetFromRider(uCharacter)
    for sLabel, tIcon in pairs(_tArmorToIcon) do
      if Object.HasLabel(uVehicle, sLabel) then
        SetVehicleIcon(oWidget, tIcon)
        return
      end
    end
    SetVehicleIcon(oWidget, _tDefaultIcon)
  end
end

function HandleUpdateEvent(oWidget, nTimeSinceLastUpdate)
  local nHealthPercentage = oWidget.CustomData.nHealthValue / 100
  local nBarX1, nBarY1, nBarX2, nBarY2 = oWidget:GetChildren()[5]:GetLocation()
  if nBarX2 < nBarX1 + nHealthPercentage * oWidget.CustomData.nBarLength then
    oWidget:GetChildren()[5]:SetLocation(nil, nil, nBarX2 + 1 * nTimeSinceLastUpdate * oWidget.CustomData.nBarLength)
    local nBarU1, nBarV1, nBarU2, nBarV2 = oWidget:GetChildren()[5]:GetTextureCoordinates()
    nBarX1, nBarY1, nBarX2, nBarY2 = oWidget:GetChildren()[5]:GetLocation()
    oWidget:GetChildren()[5]:SetTextureCoordinates(nil, nil, nBarU1 + oWidget.CustomData.nUDifference * ((nBarX2 - nBarX1) / oWidget.CustomData.nBarLength))
  end
  nBarX1, nBarY1, nBarX2, nBarY2 = oWidget:GetChildren()[5]:GetLocation()
  if nBarX2 > nBarX1 + nHealthPercentage * oWidget.CustomData.nBarLength then
    oWidget:GetChildren()[5]:SetLocation(nil, nil, nBarX1 + oWidget.CustomData.nHealthValue / 100 * oWidget.CustomData.nBarLength)
    local nBarU1, nBarV1, nBarU2, nBarV2 = oWidget:GetChildren()[5]:GetTextureCoordinates()
    nBarX1, nBarY1, nBarX2, nBarY2 = oWidget:GetChildren()[5]:GetLocation()
    oWidget:GetChildren()[5]:SetTextureCoordinates(nil, nil, nBarU1 + oWidget.CustomData.nUDifference * ((nBarX2 - nBarX1) / oWidget.CustomData.nBarLength))
  end
  local nDeltaX1, nDeltaY1, nDeltaX2, nDeltaY2 = oWidget:GetChildren()[4]:GetLocation()
  if nDeltaX2 > nBarX1 + oWidget.CustomData.nBarLength * nHealthPercentage then
    oWidget:GetChildren()[4]:SetLocation(nil, nil, nDeltaX2 - 0.25 * oWidget.CustomData.nBarLength * nTimeSinceLastUpdate)
  end
  nDeltaX1, nDeltaY1, nDeltaX2, nDeltaY2 = oWidget:GetChildren()[4]:GetLocation()
  if nBarX2 > nDeltaX2 then
    oWidget:GetChildren()[4]:SetLocation(nil, nil, nBarX2)
  end
end

function Min(nA, nB)
  if nA < nB then
    return nA
  else
    return nB
  end
end

function HandleHealthChangedEvent(oWidget, nCurHealth, nMaxHealth)
  local NewLifeValue = 100 * nCurHealth / nMaxHealth
  Debug.Printf("<--> HandleHealthChangedEvent")
  NewLifeValue = NewLifeValue or 100
  if 100 < NewLifeValue then
    NewLifeValue = 100
  end
  if NewLifeValue < 0 then
    NewLifeValue = 0
  end
  if NewLifeValue < 1 and 0 < NewLifeValue then
    NewLifeValue = 1
  end
  local nBarX1, nBarY1, nBarX2 = oWidget:GetChildren()[5]:GetLocation()
  local nCurrentBarLength = nBarX2 - nBarX1
  if NewLifeValue > oWidget.CustomData.nHealthValue then
    oWidget:GetChildren()[4]:SetColor(0, 128, 0)
    local nBaseX = oWidget:GetChildren()[3]:GetLocation()
    oWidget:GetChildren()[4]:SetLocation(nil, nil, nBaseX + NewLifeValue / 100 * oWidget.CustomData.nBarLength)
    oWidget:GetChildren()[1]:SetColor(0, 255, 0)
    if oWidget.CustomData.nHealthValue <= 20 and 20 < NewLifeValue then
      oWidget:GetChildren()[1].CustomData.bPulse = false
    end
    oWidget:GetChildren()[1].CustomData.bGoingDown = false
  elseif NewLifeValue < oWidget.CustomData.nHealthValue then
    local nBaseX = oWidget:GetChildren()[5]:GetLocation()
    oWidget:GetChildren()[5]:SetLocation(nil, nil, nBaseX + NewLifeValue / 100 * oWidget.CustomData.nBarLength)
    oWidget:GetChildren()[4]:SetColor(128, 0, 0)
    oWidget:GetChildren()[1]:SetColor(255, 0, 0)
    if oWidget.CustomData.nHealthValue > 20 and NewLifeValue <= 20 then
      oWidget:GetChildren()[1].CustomData.bPulse = true
      oWidget:GetChildren()[1].CustomData.bGoingDown = false
    end
  end
  nBarX1, nBarY1, nBarX2 = oWidget:GetChildren()[5]:GetLocation()
  local nBarU1 = oWidget:GetChildren()[5]:GetTextureCoordinates()
  oWidget:GetChildren()[5]:SetTextureCoordinates(nil, nil, nBarU1 + oWidget.CustomData.nUDifference * ((nBarX2 - nBarX1) / oWidget.CustomData.nBarLength))
  oWidget.CustomData.nHealthValue = NewLifeValue
  if 0 == NewLifeValue then
    oWidget:GetChildren()[5]:SetVisible(false)
  else
    oWidget:GetChildren()[5]:SetVisible(oWidget:GetVisible())
  end
end

function HandleInitialization(oWidget, Event)
  local nBackX1, nBackY1, nBackX2 = oWidget:GetChildren()[3]:GetLocation()
  oWidget.CustomData.nHealthValue = nBackX2 - nBackX1
  oWidget.CustomData.nBarLength = nBackX2 - nBackX1
  local nBarU1, nBarV1, nBarU2 = oWidget:GetChildren()[5]:GetTextureCoordinates()
  oWidget.CustomData.nUDifference = nBarU2 - nBarU1
end

function HandleVehicleInitialization(oWidget, Event)
  oWidget:SetVisible(false)
  HandleInitialization(oWidget, Event)
end

function HandleUpdateEventForBackground(oWidget, nTimeSinceLastUpdate)
  local bPulse = oWidget.CustomData.bPulse
  local bGoingDown = oWidget.CustomData.bGoingDown
  local r, g, b = oWidget:GetColor()
  if not oWidget.CustomData.bGoingDown or not oWidget.CustomData.bPulse then
    local nColorIncrement = 192 * nTimeSinceLastUpdate
    if oWidget.CustomData.bPulse then
      nColorIncrement = 512 * nTimeSinceLastUpdate
    end
    local nRIncrement = Min(nColorIncrement, 255 - r)
    local nGIncrement = Min(nColorIncrement, 255 - g)
    local nBIncrement = Min(nColorIncrement, 255 - b)
    oWidget:SetColor(r + nRIncrement, g + nGIncrement, b + nBIncrement)
    r, g, b = oWidget:GetColor()
    if 255 <= r and 255 <= g and 255 <= b and oWidget.CustomData.bPulse then
      oWidget.CustomData.bGoingDown = true
    end
  elseif oWidget.CustomData.bPulse and oWidget.CustomData.bGoingDown then
    local nColorIncrement = 512 * nTimeSinceLastUpdate
    local nRIncrement = 0
    local nGIncrement = Min(nColorIncrement, g)
    local nBIncrement = Min(nColorIncrement, b)
    oWidget:SetColor(r - nRIncrement, g - nGIncrement, b - nBIncrement)
    r, g, b = oWidget:GetColor()
    if g <= 0 and b <= 0 and oWidget.CustomData.bPulse then
      oWidget.CustomData.bGoingDown = false
    end
  end
end

function HandleVehicleEvent(oWidget, nCurHealth, nMaxHealth, bInVehicle)
  local NewLifeValue = 100 * nCurHealth / nMaxHealth
  local bAllowTransition = true
  if bInVehicle then
    if not oWidget:GetVisible() then
      bAllowTransition = false
    end
    if not oWidget.CustomData.bE3HudMode then
      oWidget:SetVisible(true)
    end
  else
    oWidget:SetVisible(false)
  end
  HandleHealthChangedEvent(oWidget, nCurHealth, nMaxHealth)
  if "number" ~= type(NewLifeValue) then
    NewLifeValue = 100
  end
  if 100 < NewLifeValue then
    NewLifeValue = 100
  end
  if NewLifeValue < 0 then
    NewLifeValue = 0
  end
  if not bAllowTransition then
    oWidget:GetChildren()[1]:SetColor(255, 255, 255)
    local nBackX1, nBackY1, nBackX2 = oWidget:GetChildren()[3]:GetLocation()
    oWidget:GetChildren()[5]:SetLocation(nil, nil, nBackX1 + NewLifeValue / 100 * oWidget.CustomData.nBarLength)
    oWidget:GetChildren()[4]:SetLocation(nil, nil, nBackX1 + NewLifeValue / 100 * oWidget.CustomData.nBarLength)
    local nBarX1, nBarY1, nBarX2 = oWidget:GetChildren()[5]:GetLocation()
    local nBarU1 = oWidget:GetChildren()[5]:GetTextureCoordinates()
    oWidget:GetChildren()[5]:SetTextureCoordinates(nil, nil, nBarU1 + oWidget.CustomData.nUDifference * ((nBarX2 - nBarX1) / oWidget.CustomData.nBarLength))
  end
end

function HandleE3HudModeEvent(oWidget, tEvent)
  if tEvent.bOn then
    if not oWidget.CustomData.bE3HudMode then
      if not oWidget:GetVisible() then
        oWidget.CustomData.bStayInvisible = true
      end
      oWidget:SetVisible(false)
      oWidget.CustomData.bE3HudMode = true
    end
  else
    if not oWidget.CustomData.bStayInvisible then
      oWidget:SetVisible(true)
    end
    oWidget.CustomData.bE3HudMode = false
  end
end

function DrawDebugRectangle(TargetWidget, t)
  local thing = {
    CommandType = "text",
    x = 100,
    y = 100,
    text = t,
    font = "lucida12",
    RedLevel = 255,
    GreenLevel = 0,
    BlueLevel = 0,
    TranslucencyLevel = 255,
    HorizontalAnchor = "left",
    VerticalAnchor = "top"
  }
  TargetWidget.DrawingCommands[1] = thing
end
