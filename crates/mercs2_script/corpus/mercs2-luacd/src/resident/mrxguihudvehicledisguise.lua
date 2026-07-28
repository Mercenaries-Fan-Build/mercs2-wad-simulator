_knPulseTime = 0.4
_knPulseTimeFast = 0.1
_knMoveTime = 2
import("MrxGui")
nVehicleNameTime = 3

function _Initialize(oWidget)
  local tData = oWidget.CustomData
  tData.nValue = 0
  local nR, nG, nB = oWidget:GetColor()
  tData.nBasePoint = oWidget:AddAnimationPoint({
    RedLevel = nR,
    GreenLevel = nG,
    BlueLevel = nB
  })
  tData.nRedPoint = oWidget:AddAnimationPoint({GreenLevel = 0, BlueLevel = 0})
  oWidget.PulseRed = _PulseRed
  oWidget.PulseRedLoop = _PulseRedLoop
  oWidget.HaltPulse = _HaltPulse
  oWidget.SetDisguiseLevel = SetDisguiseLevel
  oWidget.SetVehicleName = SetVehicleName
  local tChildren = oWidget:GetChildren()
  tData.oIcon = tChildren[3]
  tData.oIconCross = tChildren[4]
  tData.oBarBack = tChildren[1]
  tData.oBarFront = tChildren[2]
  tData.oIdentifier = tChildren[5]
  for n, oChild in pairs(tChildren) do
    SetUpVisibilityManagement(oChild)
  end
  local nX1, nY1, nX2, nY2 = tData.oBarFront:GetLocation()
  local nU1, nV1, nU2, nV2 = tData.oBarFront:GetTextureCoordinates()
  tData.nBarX = nX1
  tData.nBarWidth = nX2 - nX1
  tData.nBarU = nU1
  tData.nBarTexWidth = nU2 - nU1
  local oIcon = tData.oIcon
  tData.nIconFade = oIcon:AddAnimationPoint({TranslucencyLevel = 0})
  tData.nIconBase = oIcon:AddAnimationPoint({TranslucencyLevel = 255})
  local oIconCross = tData.oIconCross
  tData.nIconCrossFade = oIconCross:AddAnimationPoint({TranslucencyLevel = 0})
  tData.nIconCrossBase = oIconCross:AddAnimationPoint({TranslucencyLevel = 255})
  oWidget:SetVisible(false)
end

function SetDisguiseLevel(oWidget, nValue, bDisguised)
  local tData = oWidget.CustomData
  local oIcon = tData.oIcon
  nValue = math.max(math.min(100, nValue), 0)
  tData.bIsDisguised = bDisguised
  tData.nNewDisguiseLevel = nValue
end

STATE_DISGUISED = 1
STATE_UNDISGUISED = 2
STATE_GAINING = 3
STATE_LOSING = 4

function DisguiseUpdate(oWidget, nDeltaTime)
  local tData = oWidget.CustomData
  local nState = 0
  local bLostDisguised = tData.bWasDisguised and not tData.bIsDisguised
  local bDisguised = tData.bIsDisguised
  local nEpsilon = 0.001
  local bValueChanging = nEpsilon < math.abs(tData.nLastDisguiseLevel - tData.nNewDisguiseLevel)
  if bValueChanging then
    if bDisguised then
      nState = STATE_LOSING
    else
      nState = STATE_GAINING
    end
  elseif bDisguised then
    nState = STATE_DISGUISED
  else
    nState = STATE_UNDISGUISED
  end
  if tData.oIdentifier:GetVisible() then
    if tData.nNewDisguiseLevel < 50 and tData.bIsDisguised then
      tData.oIdentifier:ChangeVisible(false)
      tData.oIdentifier.CustomData.nVisibleTime = 0
    else
      tData.oIdentifier.CustomData.nVisibleTime = tData.oIdentifier.CustomData.nVisibleTime - nDeltaTime
      if 0 >= tData.oIdentifier.CustomData.nVisibleTime then
        tData.oIdentifier:ChangeVisible(false)
      end
    end
  end
  local bBarVisible = not tData.oIdentifier.CustomData.bVisible
  if STATE_DISGUISED == nState then
    tData.oIcon:ChangeVisible(true)
    tData.oIconCross:ChangeVisible(false)
    tData.oBarFront:ChangeVisible(false)
    tData.oBarBack:ChangeVisible(false)
    tData.nIconTimeUntilHide = nil
  elseif STATE_UNDISGUISED == nState then
    tData.oBarFront:ChangeVisible(false)
    tData.oBarBack:ChangeVisible(false)
    if not tData.nIconTimeUntilHide then
      tData.oIcon:ChangeVisible(true)
      tData.oIconCross:ChangeVisible(true)
      tData.nIconTimeUntilHide = 1
    elseif 0 < tData.nIconTimeUntilHide then
      tData.nIconTimeUntilHide = tData.nIconTimeUntilHide - nDeltaTime
      if 0 >= tData.nIconTimeUntilHide then
        tData.nIconTimeUntilHide = -1
        tData.oIcon:ChangeVisible(false)
        tData.oIconCross:ChangeVisible(false)
      end
    end
  elseif STATE_GAINING == nState then
    tData.oIcon:ChangeVisible(true)
    tData.oIconCross:ChangeVisible(true)
    tData.oBarFront:ChangeVisible(bBarVisible)
    tData.oBarBack:ChangeVisible(bBarVisible)
    tData.oBarFront:SetColor(255, 102, 102)
    tData.nIconTimeUntilHide = nil
  elseif STATE_LOSING == nState then
    tData.oIcon:ChangeVisible(true)
    tData.oIconCross:ChangeVisible(false)
    tData.oBarFront:ChangeVisible(bBarVisible)
    tData.oBarBack:ChangeVisible(bBarVisible)
    tData.oBarFront:SetColor(200, 255, 200)
    tData.nIconTimeUntilHide = nil
  end
  local oBar = tData.oBarFront
  oBar:SetLocation(tData.nBarX, nil, tData.nBarX + tData.nBarWidth * (tData.nLastDisguiseLevel / 100))
  oBar:SetTextureCoordinates(tData.nBarU, nil, tData.nBarU + tData.nBarTexWidth * (tData.nLastDisguiseLevel / 100))
  tData.bWasDisguised = tData.bIsDisguised
  tData.nLastDisguiseLevel = tData.nNewDisguiseLevel
  tData.oIcon:SetVisible(Player.GetVehicleDisguise())
  tData.oIconCross:SetVisible(Player.GetVehicleDisguise())
  tData.oBarFront:SetVisible(Player.GetVehicleDisguise())
  tData.oBarBack:SetVisible(Player.GetVehicleDisguise())
end

function SetVehicleName(oWidget, sName, uFaction, bDisguised)
  local sFaction
  if not uFaction then
    sFaction = "temp_radar_icon_pmc"
  else
    sFaction = _tFactionTextures[uFaction]
  end
  sFaction = sFaction or "temp_radar_icon_pmc"
  if not sName then
    oWidget:SetVisible(false)
    for n, oChild in pairs(oWidget:GetChildren()) do
      oChild:ChangeVisible(false)
    end
    oWidget.CustomData.oIdentifier:Set(nil)
    oWidget.CustomData.nLastDisguiseLevel = nil
    oWidget.CustomData.bWasDisguised = nil
    oWidget.CustomData.bPulsing = false
    oWidget:SetEventHandler("GuiUpdate", nil)
  else
    oWidget:SetVisible(true)
    for n, oChild in pairs(oWidget:GetChildren()) do
      oChild:ChangeVisible(false, true)
      oChild:SetVisible(Player.GetVehicleDisguise())
    end
    oWidget:SetEventHandler("GuiUpdate", DisguiseUpdate)
    oWidget.CustomData.oIdentifier:Set(sName)
    oWidget.CustomData.oIdentifier:ChangeVisible(true)
    oWidget.CustomData.oIdentifier:SetVisible(true)
    oWidget.CustomData.oIcon:SetTexture(sFaction)
    local tData = oWidget.CustomData
    oWidget.CustomData.nLastDisguiseLevel = 0
    oWidget.CustomData.nNewDisguiseLevel = 0
    oWidget.CustomData.bWasDisguised = false
    oWidget.CustomData.bIsDisguised = false
    oWidget.CustomData.bPulsing = false
  end
end

function HandleVehicleNameUpdate(oWidget, sName, uFaction)
  oWidget:SetVehicleName(sName, uFaction)
end

function _PulseRed(oWidget)
  oWidget:AnimateToPoint(oWidget.CustomData.nRedPoint, 0, true)
  oWidget:AnimateToPoint(oWidget.CustomData.nRedPoint, _knPulseTime, false)
  if oWidget.CustomData.bPulse then
    oWidget:AnimateToPoint(oWidget.CustomData.nBasePoint, _knPulseTime, false, _PulseRedLoopLow)
  else
    oWidget:AnimateToPoint(oWidget.CustomData.nBasePoint, _knPulseTime * 2, false)
  end
end

function _PulseRedLoop(oWidget)
  oWidget.CustomData.bPulse = true
  oWidget:AnimateToPoint(oWidget.CustomData.nRedPoint, _knPulseTime, false, _PulseRedLoopHigh)
end

function _PulseRedLoopLow(oWidget)
  local nTime = _knPulseTime
  if oWidget.CustomData.nLastDisguiseLevel and oWidget.CustomData.nLastDisguiseLevel < 25 then
    nTime = _knPulseTimeFast
  end
  if oWidget.CustomData.bPulse then
    oWidget:AnimateToPoint(oWidget.CustomData.nRedPoint, nTime, true, _PulseRedLoopHigh)
  end
end

function _PulseRedLoopHigh(oWidget)
  local nTime = _knPulseTime
  if oWidget.CustomData.nLastDisguiseLevel and oWidget.CustomData.nLastDisguiseLevel < 25 then
    nTime = _knPulseTimeFast
  end
  oWidget:AnimateToPoint(oWidget.CustomData.nBasePoint, nTime, true, _PulseRedLoopLow)
end

function _HaltPulse(oWidget, bImmediate)
  oWidget.CustomData.bPulse = false
  if bImmediate then
    oWidget:AnimateToPoint(oWidget.CustomData.nBasePoint, 0, true)
  end
end

function _HandleVehicleChange(oWidget, sVehicleName, nDisguiseLevel)
  oWidget:SetVehicleName(sVehicleName, nDisguiseLevel)
end

function HandleDisguiseUpdate(oWidget, nLevel, bDisguised)
  oWidget:SetDisguiseLevel(nLevel, bDisguised)
end

function SetUpVisibilityManagement(oWidget)
  oWidget.CustomData.nVisiblePoint = oWidget:AddAnimationPoint({TranslucencyLevel = 255})
  oWidget.CustomData.nInvisiblePoint = oWidget:AddAnimationPoint({TranslucencyLevel = 0})
  oWidget.CustomData.bVisible = true
  oWidget.ChangeVisible = ChangeVisibility
end

function ChangeVisibility(oWidget, bVisible, bInstant)
  if bVisible ~= oWidget.CustomData.bVisible or bInstant then
    local nTime = 0.25
    if bInstant then
      nTime = 0
    end
    if bVisible then
      oWidget:AnimateToPoint(oWidget.CustomData.nVisiblePoint, nTime, true)
    else
      oWidget:AnimateToPoint(oWidget.CustomData.nInvisiblePoint, nTime, true)
    end
    oWidget.CustomData.bVisible = bVisible
  end
end

function _InitIdentifier(oWidget)
  oWidget:SetVisible(false)
  local tData = oWidget.CustomData
  tData.nBasePoint = oWidget:AddAnimationPoint({TranslucencyLevel = 255})
  tData.nFadePoint = oWidget:AddAnimationPoint({TranslucencyLevel = 0})
  oWidget.Set = SetIdentifier
end

function SetIdentifier(oWidget, sName)
  local tData = oWidget.CustomData
  if not sName then
    oWidget:SetVisible(false)
    oWidget:SetText(" ")
    oWidget:ChangeVisible(false, true)
  else
    oWidget:SetText(sName)
    oWidget:SetVisible(true)
    oWidget:ChangeVisible(true)
    oWidget.CustomData.nVisibleTime = nVehicleNameTime
  end
end

_tFactionTextures = false

function Init()
  _tFactionTextures = {}
  local uAllies = StringToGuid("0xbbc34ef4")
  local uChina = StringToGuid("0x41359cce")
  local uOil = StringToGuid("0xe947b797")
  local uPlav = StringToGuid("0xb10d73ce")
  local uPirate = StringToGuid("0xc18215fe")
  local uPmc = StringToGuid("0x30e4a26f")
  local uCiv = StringToGuid("0xdcc8b14d")
  local uVza = StringToGuid("0xb4420059")
  _tFactionTextures[uAllies] = "HUD_faction_AN"
  _tFactionTextures[uChina] = "HUD_faction_CH"
  _tFactionTextures[uOil] = "HUD_faction_OC"
  _tFactionTextures[uPlav] = "HUD_faction_GR"
  _tFactionTextures[uPirate] = "HUD_faction_PR"
  _tFactionTextures[uCiv] = "HUD_faction_CV"
  _tFactionTextures[uVza] = "HUD_faction_VZ"
  _tFactionTextures[uPmc] = "HUD_faction_PMC"
end
