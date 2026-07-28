import("MrxGui")
import("MrxGuiBase")
import("MrxGuiManager")
import("MrxTutorialManager")

function AddRegionToMinimap(oMinimap, uGuid, nRed, nGreen, nBlue, nAlpha, bInvert)
  local nId = oMinimap.CustomData.tRegionGuids[uGuid]
  if not nId then
    nId = 1
    while oMinimap.CustomData.tRegions[nId] do
      nId = nId + 1
    end
  else
    oMinimap:RemoveRegion(uGuid)
  end
  local tData = {uGuid = uGuid}
  local nR = nRed
  local nG = nGreen
  local nB = nBlue
  local nA = nAlpha
  nR = _Clamp(nR, 0, 255)
  nG = _Clamp(nG, 0, 255)
  nB = _Clamp(nB, 0, 255)
  nA = _Clamp(nA, 0, 255)
  nR = nR or 64
  nG = nG or 64
  nB = nB or 160
  nA = nA or 128
  nA = nA / 255 * 100
  tData.sColor = "0x" .. string.format("%02X", nR) .. string.format("%02X", nG) .. string.format("%02X", nB)
  tData.nAlpha = nA
  tData.bInvert = bInvert
  oMinimap.CustomData.tRegions[nId] = tData
  oMinimap.CustomData.tRegionGuids[uGuid] = nId
  if oMinimap.CustomData.bHaveFlash then
    _DisplayMinimapRegion(oMinimap, nId)
  end
end

function _Clamp(n, nMin, nMax)
  if not n or "number" ~= type(n) then
    return
  end
  if nMax < n then
    return nMax
  end
  if n < nMin then
    return nMin
  end
  return n
end

function RemoveRegionFromMinimap(oMinimap, uGuid)
  local nId = oMinimap.CustomData.tRegionGuids[uGuid]
  if nId then
    oMinimap.CustomData.tRegionGuids[nId] = nil
    if oMinimap.CustomData.bHaveFlash then
      oMinimap:CallActionScriptCallback("RemoveZone", {nId})
    end
  end
end

function _Initialize(oMinimap)
  oMinimap.AddRegion = AddRegionToMinimap
  oMinimap.RemoveRegion = RemoveRegionFromMinimap
  oMinimap.CustomData.tRegions = {}
  oMinimap.CustomData.tRegionGuids = {}
  oMinimap.CustomData.bHaveFlash = false
  oMinimap:SetSwfFile("minimap.gfx", _FinishInitialization, {oMinimap})
  oParent = oMinimap.ParentWidget
  oRadarObject = oParent:GetChildren()[3]
  if oRadarObject then
    oRadarObject:SetEventHandler("SetTargetMarker", HandleSetTargetMarker)
    oRadarObject:SetEventHandler("SetGPSDest", HandleSetGPSDest)
    oRadarObject:SetEventHandler("ClearGPSDest", HandleClearGPSDest)
  end
end

function _FinishInitialization(oMinimap)
  oMinimap.CustomData.bHaveFlash = true
  for nId in pairs(oMinimap.CustomData.tRegions) do
    _DisplayMinimapRegion(oMinimap, nId)
  end
end

function _DisplayMinimapRegion(oMinimap, nId)
  local tData = oMinimap.CustomData.tRegions[nId]
  if not tData then
    return
  end
  local tX, tY = Pg.GetLineRegionPoints(tData.uGuid, tData.bInvert)
  local nY
  local bFirst = true
  local nXOffset = 35
  local nYOffset = 40
  for nIndex, nX in ipairs(tX) do
    nY = tY[nIndex]
    if bFirst then
      oMinimap:CallActionScriptCallback("AddZone", {
        nId,
        true,
        false,
        nX + nXOffset,
        nY + nYOffset,
        tData.sColor,
        tData.nAlpha
      })
      bFirst = false
    else
      oMinimap:CallActionScriptCallback("AddZone", {
        nId,
        false,
        false,
        nX + nXOffset,
        nY + nYOffset
      })
    end
  end
  oMinimap:CallActionScriptCallback("AddZone", {
    nId,
    false,
    true,
    0,
    0
  })
end

_sTargetName = "Target marker"

function HandleSetTargetMarker(oMap, tData)
  if tData.number then
    if tData.x and tData.y and tData.z and tData.r and tData.g and tData.b and tData.texture then
      oMap:AddObjective(_sTargetName .. tostring(tData.number), tData.x, tData.y, tData.z, tData.r, tData.g, tData.b, 6, 6, tData.texture, tData.uGuid, true, nil, nil, 5)
    else
      oMap:DeleteObjective(_sTargetName .. tostring(tData.number))
    end
  end
end

_sGPSName = "GPS Beacon Marker"

function HandleSetGPSDest(oMap, tEvent)
  oMap:AddObjective(_sGPSName, tEvent.PosX, 0, tEvent.PosZ, 255, 255, 255, 10.666667, 10.666667, "MiniMap_Icon_GPS_Marker", nil, true, nil, nil, 4)
end

function HandleClearGPSDest(oMap, tEvent)
  oMap:DeleteObjective(_sGPSName)
end

function ShowMapLabel(oWidget, sString, nDisplayTime, bAnimation)
  if oWidget.CustomData.bTrespass then
    return
  end
  if "string" ~= type(sString) then
    return
  end
  if "number" ~= type(nDisplayTime) then
    nDisplayTime = nil
  end
  bAnimation = false
  if bAnimation then
    oWidget:SetText(" ")
    local oFlash = oWidget.CustomData.oFlash
    if oFlash.CustomData.bLoaded then
      oWidget:SetVisible(true)
    end
  else
    if oWidget:GetVisible() then
      oWidget.CustomData.oFlash:SetVisible(false)
      oWidget:SetEventHandler("GuiUpdate", nil)
      oWidget:AnimateToPoint(oWidget.CustomData.nFadeOutPoint, 0.4, true, _HandleTransition, {sString, nDisplayTime})
    end
    oWidget:SetVisible(true)
    oWidget:SetText(sString)
    oWidget:SetTranslucency(0)
    oWidget.CustomData.nTime = nDisplayTime or 4
    if 0 <= oWidget.CustomData.nTime then
      oWidget:AnimateToPoint(oWidget.CustomData.nFadeInPoint, 0.4, true, oWidget.SetEventHandler, {"GuiUpdate", _HandleMapLabelUpdateEvent})
    else
      oWidget:AnimateToPoint(oWidget.CustomData.nFadeInPoint, 0.4, true)
    end
  end
end

function _HandleTransition(oWidget, sString, nDisplayTime)
  oWidget:SetVisible(false)
  oWidget:Show(sString, nDisplayTime)
end

function _HandleMapLabelInitialization(oWidget)
  oWidget.CustomData.nTime = 0
  oWidget.Show = ShowMapLabel
  oWidget:SetVisible(false)
  oWidget.CustomData.nFadeInPoint = oWidget:AddAnimationPoint({TranslucencyLevel = 255})
  oWidget.CustomData.nFadeOutPoint = oWidget:AddAnimationPoint({TranslucencyLevel = 0})
  local oFlash = MrxGuiBase.FlashWidget:new()
  local nX1, nY1, nX2, nY2 = oWidget:GetLocation()
  local nY = (nY1 + nY2) * 0.5
  local nScale = 0.6666667
  nX2 = nX2 + 10
  oFlash:SetLocation(nX2 - 850 * nScale, nY - 50 * (nScale / 2), nX2, nY + 50 * (nScale / 2))
  oFlash:SetAnchoring("right", "top")
  oFlash:SetName("maplabeltext")
  oFlash:SetOwner(oWidget:GetOwner())
  oWidget:AddChild(oFlash)
  oWidget.CustomData.oFlash = oFlash
  oFlash.ParentWidget = oWidget
  MrxGuiBase.AddWidget(oFlash)
  oFlash:SetVisible(false)
  MrxGuiManager.AddWidgetToHud(oWidget:GetOwner(), oFlash)
end

function _CompleteTextFlashLoad(oFlash)
  oFlash.CustomData.bLoaded = true
  oFlash:SetFlashEventHandler("close", _CompleteTextFlashAnimation)
end

function _CompleteTextFlashAnimation(oFlash)
  oFlash.ParentWidget:SetVisible(false)
end

function _HandleMapLabelUpdateEvent(oWidget, nDeltaTime)
  oWidget.CustomData.nTime = oWidget.CustomData.nTime - nDeltaTime
  if oWidget.CustomData.nTime <= 0 then
    oWidget:SetEventHandler("GuiUpdate", nil)
    oWidget.CustomData.nTime = 0
    oWidget:AnimateToPoint(oWidget.CustomData.nFadeOutPoint, 0.4, true, oWidget.SetVisible, {false})
  end
end

function _HandleMapLabelTrespassEvent(oWidget, tEvent)
  if tEvent.bTrespassing then
    oWidget:SetJustification("left")
    oWidget:Show("[red][Generic.Trespassing]", -1)
    oWidget.CustomData.bTrespass = true
    oWidget.CustomData.oFlash:SetVisible(false)
    MrxTutorialManager.StartTutorial("Trespass", true)
  else
    oWidget.CustomData.bTrespass = false
    oWidget:SetJustification("right")
    oWidget:AnimateToPoint(oWidget.CustomData.nFadeOutPoint, 0.4, true, oWidget.SetVisible, {false})
  end
end

function _HandleTrespassIconInit(oIcon)
  oIcon:SetVisible(false)
end

function _HandleTrespassIconEvent(oIcon, tEvent)
  if tEvent.bTrespassing and tEvent.sFaction and _tIcons[tEvent.sFaction] then
    oIcon:SetVisible(true)
    oIcon:SetTexture(_tIcons[tEvent.sFaction])
  else
    oIcon:SetVisible(false)
  end
end

function Init()
  _tIcons = {
    All = "HUD_faction_AN",
    Chi = "HUD_faction_CH",
    Civ = "HUD_faction_CV",
    Gur = "HUD_faction_GR",
    Oil = "HUD_faction_OC",
    Pir = "HUD_faction_PR",
    Vz = "HUD_faction_VZ"
  }
end
