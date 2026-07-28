import("MrxGuiManager")
import("MrxGui")
import("MrxSound")
_nIntroZoomScale = 3
_nIntroZoomTime = 0.25

function HandleSniperScopeEnter(oWidget, tEvent)
  if oWidget:GetOwner() ~= tEvent.uPlayerGuid then
    return
  end
  if tEvent.bSniper then
    MrxSound.EnterScopeView()
    oWidget:SetVisible(true)
    if oWidget.CustomData.bNeedsPush then
      MrxGui.PushWidgetToBack(oWidget)
      MrxGui.PushWidgetToFront(oWidget.CustomData.oFocusText)
      MrxGui.PushWidgetToFront(oWidget.CustomData.oFaction)
      MrxGui.PushWidgetToFront(oWidget.CustomData.oDescription)
      MrxGui.PushWidgetToFront(oWidget.CustomData.oHealth)
      oWidget.CustomData.bNeedsPush = nil
    end
    oWidget.CustomData.bOn = true
    if oWidget.CustomData.bUsingZoom then
      oWidget:AnimateToPoint(oWidget.CustomData.nEndPoint, _nIntroZoomTime, true, _FinishEnter)
      oWidget.CustomData.oFocusText:SetVisible(false)
      oWidget.CustomData.oDescription:SetVisible(false)
      oWidget.CustomData.oHealth:SetVisible(false)
      oWidget.CustomData.oFocusText:SetEnabled(true)
      oWidget.CustomData.oFaction:SetEnabled(true)
      oWidget.CustomData.oHealth:SetEnabled(false)
    else
      _FinishEnter(oWidget)
    end
    if not oWidget.CustomData.bHaveHudState then
      local bEnabled = MrxGuiManager.GetHudState(oWidget:GetOwner())
      oWidget.CustomData.bHudState = bEnabled
      if bEnabled then
        MrxGuiManager.ToggleHud(oWidget:GetOwner(), false, "scope")
      end
      oWidget.CustomData.bHaveHudState = true
      local oAmmo = MrxGui.GetWidgetByNameAndOwner("Guns", oWidget:GetOwner())
      _RecursiveWakeup(oAmmo, true)
      local oHealth = MrxGui.GetWidgetByNameAndOwner("Health Counter", oWidget:GetOwner())
      _RecursiveWakeup(oHealth, true)
    end
  end
end

function _FinishEnter(oWidget)
  local tChildren = oWidget:GetChildren()
  for nIndex, oChild in pairs(tChildren) do
    oChild:SetEnabled(true)
  end
  oWidget.CustomData.oFocusText:SetLocation(oWidget.CustomData.nTextX1, oWidget.CustomData.nTextY1, oWidget.CustomData.nTextX2, oWidget.CustomData.nTextY2)
  oWidget.CustomData.oDescription:SetLocation(oWidget.CustomData.nDescX1, oWidget.CustomData.nDescY1, oWidget.CustomData.nDescX2, oWidget.CustomData.nDescY2)
  oWidget.CustomData.oFocusText:SetVisible(true)
  oWidget.CustomData.oDescription:SetVisible(true)
  oWidget.CustomData.oHealth:SetVisible(true)
  local nX, nY, nZ, uTarget = Player.GetTargetUnderReticle(oWidget:GetOwner())
  if uTarget then
    local nHealth = Object.GetHealth(uTarget) or -1
    local nMaxHealth = Object.GetMaxHealth(uTarget) or -1
    HandleSniperHealthUpdate(oWidget.CustomData.oHealth, nil, nil, nil, nil, nil, nHealth, nMaxHealth)
  else
    oWidget.CustomData.oHealth:SetVisible(false)
  end
end

function HandleSniperScopeExit(oWidget, tEvent)
  if oWidget:GetOwner() ~= tEvent.uPlayerGuid then
    return
  end
  if oWidget.CustomData.bOn then
    local tChildren = oWidget:GetChildren()
    for nIndex, oChild in pairs(tChildren) do
      oChild:SetEnabled(false)
    end
    if oWidget.CustomData.bUsingZoom then
      oWidget.CustomData.bOn = false
      oWidget.CustomData.oFocusText:SetVisible(false)
      oWidget.CustomData.oDescription:SetVisible(false)
      oWidget.CustomData.oHealth:SetVisible(false)
      oWidget:AnimateToPoint(oWidget.CustomData.nBigPoint, _nIntroZoomTime, true, _FinishExit)
    else
      _FinishExit(oWidget)
    end
  end
end

function _FinishExit(oWidget)
  oWidget:SetVisible(false)
  if oWidget.CustomData.bHudState then
    MrxGuiManager.ToggleHud(oWidget:GetOwner(), true)
  else
    local oAmmo = MrxGui.GetWidgetByNameAndOwner("Guns", oWidget:GetOwner())
    _RecursiveWakeup(oAmmo, false)
    local oHealth = MrxGui.GetWidgetByNameAndOwner("Health Counter", oWidget:GetOwner())
    _RecursiveWakeup(oHealth, false)
  end
  oWidget.CustomData.bOn = false
  oWidget.CustomData.bHaveHudState = false
  MrxSound.ExitScopeView()
end

function HandleInitialization(oWidget, tEvent)
  oWidget:SetVisible(false)
  oWidget.CustomData.bNeedsPush = true
  local tChildren = oWidget:GetChildren()
  for nIndex, oChild in pairs(tChildren) do
    oChild:SetEnabled(false)
  end
  local oFocusText = tChildren[3]
  local oDescription = tChildren[5]
  oWidget.CustomData.oFocusText = oFocusText
  oWidget.CustomData.oFaction = tChildren[4]
  oWidget.CustomData.oDescription = oDescription
  oWidget.CustomData.oHealth = tChildren[6]
  oFocusText.CustomData.oDescription = oDescription
  oDescription:Wrap()
  oWidget.CustomData.bHudState = true
  if _GuiInternal.SetWidgetUseNewRescale then
    local nTextX1, nTextY1, nTextX2, nTextY2 = oFocusText:GetLocation()
    oWidget.CustomData.nTextX1 = nTextX1
    oWidget.CustomData.nTextY1 = nTextY1
    oWidget.CustomData.nTextX2 = nTextX2
    oWidget.CustomData.nTextY2 = nTextY2
    local nDescX1, nDescY1, nDescX2, nDescY2 = oDescription:GetLocation()
    oWidget.CustomData.nDescX1 = nDescX1
    oWidget.CustomData.nDescY1 = nDescY1
    oWidget.CustomData.nDescX2 = nDescX2
    oWidget.CustomData.nDescY2 = nDescY2
    local nX1, nY1, nX2, nY2 = oWidget:GetLocation()
    local nWidth = (nX2 - nX1) * _nIntroZoomScale * 0.5
    local nHeight = (nY2 - nY1) * _nIntroZoomScale * 0.5
    oWidget.CustomData.nBigPoint = oWidget:AddAnimationPoint({
      x = 320 - nWidth,
      y = 240 - nHeight,
      x2 = 320 + nWidth,
      y2 = 240 + nHeight
    })
    oWidget.CustomData.nEndPoint = oWidget:AddAnimationPoint({
      x = nX1,
      y = nY1,
      x2 = nX2,
      y2 = nY2
    })
    oWidget.CustomData.bUsingZoom = true
    _GuiInternal.SetWidgetUseNewRescale(oWidget.BasicData.uId, true)
    oWidget:AnimateToPoint(oWidget.CustomData.nBigPoint, 0, true)
  end
end

function HandleSniperHealthInit(oWidget, tEvent)
  local oBar = oWidget:GetChildren()[1]
  oWidget.CustomData.oBar = oBar
  local nX1, nY1, nX2 = oBar:GetLocation()
  oWidget.CustomData.nX = nX1
  oWidget.CustomData.nLength = nX2 - nX1
end

function HandleSniperHealthUpdate(oWidget, nTargetRelation, nScreenX, nScreenY, nSpreadX, nSpreadY, nHealth, nMaxHealth)
  if 0 <= nHealth and 0 <= nMaxHealth then
    oWidget:SetVisible(true)
    oWidget.CustomData.oBar:SetLocation(oWidget.CustomData.nX, nil, oWidget.CustomData.nX + oWidget.CustomData.nLength * (nHealth / nMaxHealth))
  else
    oWidget:SetVisible(false)
  end
end

function HandleZoomUpdate(oWidget, tEvent)
  if oWidget:GetOwner() ~= tEvent.uPlayerGuid then
    return
  end
  if tEvent.nZoomLevel then
    oWidget:SetText("ZOOM x " .. tEvent.nZoomLevel)
  end
end

function HandleHeadingUpdate(oWidget, tEvent)
  if oWidget:GetOwner() ~= tEvent.uPlayerGuid then
    return
  end
  if tEvent.nCameraHeading then
    oWidget:SetText(tEvent.nCameraHeading)
  end
end

function HandleFocusUpdate(oWidget, tEvent)
  if oWidget:GetOwner() ~= tEvent.uPlayerGuid then
    return
  end
  if tEvent.sFocusName then
    oWidget:SetText(tEvent.sFocusName)
    if tEvent.uFocusGuid then
      Event.Post("InFocus", {
        uTarget = tEvent.uFocusGuid,
        uViewer = oWidget:GetOwner(),
        bSniper = true
      })
      oWidget.CustomData.oDescription:SetText(MrxGui.GetObjectiveDescription(tEvent.uFocusGuid) or " ")
    else
      oWidget.CustomData.oDescription:SetText(" ")
    end
  end
end

function HandleFactionUpdate(oWidget, tEvent)
  if oWidget:GetOwner() ~= tEvent.uPlayerGuid then
    return
  end
  local sType = type(tEvent.FactionTexture)
  if "userdata" == sType or "string" == sType then
    oWidget:SetTexture(tEvent.FactionTexture)
    oWidget:SetTranslucency(255)
  elseif "number" == sType then
    oWidget:SetTranslucency(0)
  end
end

function HandleHorizScrollUpdate(oWidget, tEvent)
  if oWidget:GetOwner() ~= tEvent.uPlayerGuid then
    return
  end
  local nHeading = tonumber(tEvent.nCameraHeading)
  if "number" ~= type(nHeading) then
    return
  end
  local nRange = 180
  while nHeading < 0 do
    nHeading = nHeading + nRange
  end
  while nRange < nHeading do
    nHeading = nHeading - nRange
  end
  nHeading = nHeading / (nRange * 2)
  oWidget:SetTextureCoordinates(nHeading, nil, nHeading + 0.5, nil)
end

function HandleVertScrollUpdate(oWidget, tEvent)
  if oWidget:GetOwner() ~= tEvent.uPlayerGuid then
    return
  end
  local nPitch = tEvent.nPitch
  if "number" ~= type(nPitch) then
    return
  end
  local nRange = 180
  while nPitch < 0 do
    nPitch = nPitch + nRange
  end
  while nRange < nPitch do
    nPitch = nPitch - nRange
  end
  nPitch = nPitch / (nRange * 2)
  oWidget:SetTextureCoordinates(nil, nPitch, nil, nPitch + 0.5)
end

function _RecursiveWakeup(oWidget, bAwaken)
  oWidget:SetSleeping(not bAwaken)
  local tChildren = oWidget:GetChildren()
  for nIndex, oChild in pairs(tChildren) do
    _RecursiveWakeup(oChild, bAwaken)
  end
end
