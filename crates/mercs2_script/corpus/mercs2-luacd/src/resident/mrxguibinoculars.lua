import("MrxGuiManager")
import("MrxGui")
import("MrxSound")
_nIntroZoomScale = 3
_nIntroZoomTime = 0.25

function HandleBinocularsEnter(oWidget, tEvent)
  if oWidget:GetOwner() ~= tEvent.uPlayerGuid then
    return
  end
  if not tEvent.bSniper then
    MrxSound.EnterScopeView()
    oWidget:SetVisible(true)
    if not oWidget.CustomData.bHaveHudState then
      local bEnabled = MrxGuiManager.GetHudState(oWidget:GetOwner())
      oWidget.CustomData.bHudState = bEnabled
      if bEnabled then
        MrxGuiManager.ToggleHud(oWidget:GetOwner(), false, "scope")
      end
      oWidget.CustomData.bHaveHudState = true
    end
    oWidget.CustomData.bOn = true
    if oWidget.CustomData.bUsingZoom then
      oWidget:AnimateToPoint(oWidget.CustomData.nEndPoint, _nIntroZoomTime, true, _FinishEnter)
      oWidget.CustomData.oFocusText:SetVisible(false)
      oWidget.CustomData.oFocusText:SetEnabled(true)
      oWidget.CustomData.oFaction:SetEnabled(true)
      oWidget.CustomData.oDescription:SetVisible(false)
      local oReticle = oWidget.CustomData.oReticle
      oReticle:AnimateToPoint(oReticle.CustomData.nEndPoint, _nIntroZoomTime * 0.5, true)
    else
      _FinishEnter(oWidget)
    end
  end
end

function _FinishEnter(oWidget)
  local tChildren = oWidget:GetChildren()
  for nIndex, oChild in pairs(tChildren) do
    oChild:SetEnabled(true)
  end
  oWidget.CustomData.oFocusText:SetLocation(oWidget.CustomData.nTextX1, oWidget.CustomData.nTextY1, oWidget.CustomData.nTextX2, oWidget.CustomData.nTextY2)
  oWidget.CustomData.oFocusText:SetVisible(true)
  oWidget.CustomData.oDescription:SetLocation(oWidget.CustomData.nDescX1, oWidget.CustomData.nDescY1, oWidget.CustomData.nDescX2, oWidget.CustomData.nDescY2)
  oWidget.CustomData.oDescription:SetVisible(true)
end

function HandleBinocularsExit(oWidget, tEvent)
  if oWidget:GetOwner() ~= tEvent.uPlayerGuid then
    return
  end
  local tChildren = oWidget:GetChildren()
  for nIndex, oChild in pairs(tChildren) do
    oChild:SetEnabled(false)
  end
  if oWidget.CustomData.bOn then
    if oWidget.CustomData.bUsingZoom then
      oWidget.CustomData.bOn = false
      oWidget.CustomData.oFocusText:SetVisible(false)
      oWidget.CustomData.oFaction:SetVisible(false)
      oWidget.CustomData.oDescription:SetVisible(false)
      oWidget:AnimateToPoint(oWidget.CustomData.nBigPoint, _nIntroZoomTime, true, _FinishExit)
      local oReticle = oWidget.CustomData.oReticle
      oReticle:AnimateToPoint(oReticle.CustomData.nFadePoint, _nIntroZoomTime * 0.9, true, oReticle.SetVisible, {false})
    else
      _FinishExit(oWidget)
    end
  end
end

function _FinishExit(oWidget)
  oWidget:SetVisible(false)
  if oWidget.CustomData.bHudState then
    MrxGuiManager.ToggleHud(oWidget:GetOwner(), true)
  end
  oWidget.CustomData.bHaveHudState = false
  oWidget.CustomData.bOn = false
  MrxSound.ExitScopeView()
end

function HandleInitialization(oWidget)
  oWidget:SetVisible(false)
  local tChildren = oWidget:GetChildren()
  for nIndex, oChild in pairs(tChildren) do
    oChild:SetEnabled(false)
  end
  local oReticle = tChildren[2]
  local oFocusText = tChildren[3]
  local oFaction = tChildren[7]
  local oDescription = tChildren[8]
  oWidget.CustomData.oReticle = oReticle
  oWidget.CustomData.oFocusText = oFocusText
  oWidget.CustomData.oFaction = oFaction
  oWidget.CustomData.oDescription = oDescription
  oWidget.CustomData.bHudState = true
  oDescription:Wrap()
  oFocusText.CustomData.oDescription = oDescription
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
    oReticle.CustomData.nFadePoint = oReticle:AddAnimationPoint({TranslucencyLevel = 0})
    oReticle.CustomData.nEndPoint = oReticle:AddAnimationPoint({TranslucencyLevel = 255})
    oReticle:AnimateToPoint(nFadePoint, 0, true)
  end
end

function HandleHeadingUpdate(oWidget, tEvent)
  if oWidget:GetOwner() ~= tEvent.uPlayerGuid then
    return
  end
  local nCameraHeading = tonumber(tEvent.nCameraHeading)
  if "number" ~= type(nCameraHeading) then
    return
  end
  if not oWidget.CustomData.oPointer then
    local nX1, nY1, nX2, nY2 = oWidget:GetLocation()
    local oPointer = oWidget:GetChildren()[1]
    local nPointerX1, nPointerY1, nPointerX2, nPointerY2 = oPointer:GetLocation()
    local nPointerSize = (nPointerX2 - nPointerX1) * 0.5
    local nCenter = (nX2 + nX1) * 0.5
    local nMaxDistance = nX2 - nX1
    oWidget.CustomData.oPointer = oPointer
    oWidget.CustomData.nMaxDistance = nMaxDistance
    oWidget.CustomData.nCenter = nCenter
    oWidget.CustomData.nPointerSize = nPointerSize
    oWidget.CustomData.nPointerY = nPointerY1
  end
  local nNewCoordinate = nCameraHeading / 360 * oWidget.CustomData.nMaxDistance + oWidget.CustomData.nCenter
  oWidget.CustomData.oPointer:SetLocation(nNewCoordinate - oWidget.CustomData.nPointerSize, oWidget.CustomData.nPointerY)
end

function HandleZoomUpdate(oWidget, tEvent)
  if oWidget:GetOwner() ~= tEvent.uPlayerGuid then
    return
  end
  local nZoomLevel = tonumber(tEvent.nZoomLevel)
  if "number" ~= type(nZoomLevel) then
    return
  end
  if not oWidget.CustomData.oPointer then
    local nX1, nY1, nX2, nY2 = oWidget:GetLocation()
    local oPointer = oWidget:GetChildren()[1]
    local nPointerX1, nPointerY1, nPointerX2, nPointerY2 = oPointer:GetLocation()
    local nPointerSize = (nPointerY2 - nPointerY1) * 0.5
    local nCenter = (nY2 + nY1) * 0.5
    local nMaxDistance = nY2 - nY1
    oWidget.CustomData.oPointer = oPointer
    oWidget.CustomData.nMaxDistance = nMaxDistance
    oWidget.CustomData.nCenter = nCenter
    oWidget.CustomData.nPointerSize = nPointerSize
    oWidget.CustomData.nPointerX = nPointerX1
  end
  local nNewCoordinate = (nZoomLevel - 4) / -6 * oWidget.CustomData.nMaxDistance + oWidget.CustomData.nCenter
  oWidget.CustomData.oPointer:SetLocation(oWidget.CustomData.nPointerX, nNewCoordinate - oWidget.CustomData.nPointerSize)
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

function HandleFocusUpdate(oWidget, tEvent)
  if oWidget:GetOwner() ~= tEvent.uPlayerGuid then
    return
  end
  if "string" ~= type(tEvent.sFocusName) then
    return
  end
  if tEvent.sFocusName then
    oWidget:SetText(tEvent.sFocusName)
    if tEvent.uFocusGuid then
      Event.Post("InFocus", {
        uTarget = tEvent.uFocusGuid,
        uViewer = oWidget:GetOwner(),
        bSniper = false
      })
      oWidget.CustomData.oDescription:SetText(MrxGui.GetObjectiveDescription(tEvent.uFocusGuid) or " ")
    else
      oWidget.CustomData.oDescription:SetText(" ")
    end
  end
end

function HandleVertScrollUpdate(oWidget, tEvent)
  if oWidget:GetOwner() ~= tEvent.uPlayerGuid then
    return
  end
  local nPitch = tEvent.nPitch
  if "number" ~= type(nPitch) then
    return
  end
  if not oWidget.CustomData.oPointer then
    local nX1, nY1, nX2, nY2 = oWidget:GetLocation()
    local oPointer = oWidget:GetChildren()[1]
    local nPointerX1, nPointerY1, nPointerX2, nPointerY2 = oPointer:GetLocation()
    local nPointerSize = (nPointerY2 - nPointerY1) * 0.5
    local nCenter = (nY2 + nY1) * 0.5
    local nMaxDistance = nY2 - nY1
    oWidget.CustomData.oPointer = oPointer
    oWidget.CustomData.nMaxDistance = nMaxDistance
    oWidget.CustomData.nCenter = nCenter
    oWidget.CustomData.nPointerSize = nPointerSize
    oWidget.CustomData.nPointerX = nPointerX1
  end
  local nNewCoordinate = (tEvent.nPitch - 6) / 100 * oWidget.CustomData.nMaxDistance + oWidget.CustomData.nCenter
  oWidget.CustomData.oPointer:SetLocation(oWidget.CustomData.nPointerX, nNewCoordinate - oWidget.CustomData.nPointerSize)
end
