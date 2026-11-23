local L0_1, L1_1, L2_1
import("MrxGui", false)
import("MrxGuiBase", false)
import("MrxGuiManager", false)
import("MrxTutorialManager", false)

function L0_1(A0_2, A1_2, A2_2, A3_2, A4_2, A5_2, A6_2)
  local L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2
  L7_2 = A0_2.CustomData.tRegionGuids[A1_2]
  if not L7_2 then
    L7_2 = 1
    while true do
      L8_2 = A0_2.CustomData.tRegions[L7_2]
      if not L8_2 then
        break
      end
      L7_2 = L7_2 + 1
    end
  else
    A0_2.RemoveRegion(A0_2, A1_2)
  end
  L8_2 = {}
  L8_2.uGuid = A1_2
  L9_2 = _Clamp(A2_2, 0, 255)
  L10_2 = _Clamp(A3_2, 0, 255)
  L11_2 = _Clamp(A4_2, 0, 255)
  L12_2 = _Clamp(A5_2, 0, 255)
  if not L9_2 then
    L9_2 = 64
  end
  if not L10_2 then
    L10_2 = 64
  end
  if not L11_2 then
    L11_2 = 160
  end
  if not L12_2 then
    L12_2 = 128
  end
  L12_2 = (L12_2 / 255) * 100
  L8_2.sColor = ("0x" .. string.format("%02X", L9_2) .. string.format("%02X", L10_2) .. string.format("%02X", L11_2))
  L8_2.nAlpha = L12_2
  L8_2.bInvert = A6_2
  L13_2 = A0_2.CustomData.tRegions
  L13_2[L7_2] = L8_2
  L13_2 = A0_2.CustomData.tRegionGuids
  L13_2[A1_2] = L7_2
  L13_2 = A0_2.CustomData.bHaveFlash
  if L13_2 then
    _DisplayMinimapRegion(A0_2, L7_2)
  end
end

AddRegionToMinimap = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2
  if A0_2 then
    L3_2 = type(A0_2)
    if "number" == L3_2 then
      goto lbl_9
    end
  end
  do return end
  ::lbl_9::
  if A2_2 < A0_2 then
    return A2_2
  end
  if A0_2 < A1_2 then
    return A1_2
  end
  return A0_2
end

_Clamp = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L2_2 = A0_2.CustomData.tRegionGuids[A1_2]
  if L2_2 then
    L3_2 = A0_2.CustomData.tRegionGuids
    L3_2[L2_2] = nil
    L3_2 = A0_2.CustomData.bHaveFlash
    if L3_2 then
      L6_2 = {}
      L6_2[1] = L2_2
      A0_2.CallActionScriptCallback(A0_2, "RemoveZone", L6_2)
    end
  end
end

RemoveRegionFromMinimap = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  A0_2.AddRegion = AddRegionToMinimap
  A0_2.RemoveRegion = RemoveRegionFromMinimap
  L1_2 = A0_2.CustomData
  L1_2.tRegions = {}
  L1_2 = A0_2.CustomData
  L1_2.tRegionGuids = {}
  L1_2 = A0_2.CustomData
  L1_2.bHaveFlash = false
  L5_2 = {}
  L5_2[1] = A0_2
  A0_2.SetSwfFile(A0_2, "minimap.gfx", _FinishInitialization, L5_2)
  oParent = A0_2.ParentWidget
  L1_2 = oParent
  oRadarObject = L1_2.GetChildren(L1_2)[3]
  L1_2 = oRadarObject
  if L1_2 then
    L1_2 = oRadarObject
    L1_2.SetEventHandler(L1_2, "SetTargetMarker", HandleSetTargetMarker)
    L1_2 = oRadarObject
    L1_2.SetEventHandler(L1_2, "SetGPSDest", HandleSetGPSDest)
    L1_2 = oRadarObject
    L1_2.SetEventHandler(L1_2, "ClearGPSDest", HandleClearGPSDest)
  end
end

_Initialize = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L1_2 = A0_2.CustomData
  L1_2.bHaveFlash = true
  L1_2 = pairs
  L2_2 = A0_2.CustomData.tRegions
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2 in L1_2, L2_2, L3_2 do
    _DisplayMinimapRegion(A0_2, L4_2)
  end
end

_FinishInitialization = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2
  L2_2 = A0_2.CustomData.tRegions[A1_2]
  if not L2_2 then
    return
  end
  L3_2 = Pg.GetLineRegionPoints
  L4_2 = L2_2.uGuid
  L3_2, L4_2 = L3_2(L4_2, L2_2.bInvert)
  L5_2 = nil
  L6_2 = true
  L7_2 = 35
  L8_2 = 40
  L9_2 = 1
  L10_2 = Gui.GetMapCorrectionOffset
  if L10_2 then
    L10_2 = Gui.GetMapCorrectionOffset
    L10_2, L11_2, L12_2 = L10_2()
    L9_2 = L12_2
    L8_2 = L11_2
    L7_2 = L10_2
  end
  L10_2 = ipairs
  L11_2 = L3_2
  L10_2, L11_2, L12_2 = L10_2(L11_2)
  for L13_2, L14_2 in L10_2, L11_2, L12_2 do
    L5_2 = L4_2[L13_2]
    if L6_2 then
      L18_2 = {}
      L22_2 = (L14_2 + L7_2) * L9_2
      L23_2 = (L5_2 + L8_2) * L9_2
      L18_2[1] = A1_2
      L18_2[2] = true
      L18_2[3] = false
      L18_2[4] = L22_2
      L18_2[5] = L23_2
      L18_2[6] = L2_2.sColor
      L18_2[7] = L2_2.nAlpha
      A0_2.CallActionScriptCallback(A0_2, "AddZone", L18_2)
      L6_2 = false
    else
      L18_2 = {}
      L22_2 = (L14_2 + L7_2) * L9_2
      L23_2 = (L5_2 + L8_2) * L9_2
      L18_2[1] = A1_2
      L18_2[2] = false
      L18_2[3] = false
      L18_2[4] = L22_2
      L18_2[5] = L23_2
      A0_2.CallActionScriptCallback(A0_2, "AddZone", L18_2)
    end
  end
  L13_2 = {}
  L13_2[1] = A1_2
  L13_2[2] = false
  L13_2[3] = true
  L13_2[4] = 0
  L13_2[5] = 0
  A0_2.CallActionScriptCallback(A0_2, "AddZone", L13_2)
end

_DisplayMinimapRegion = L0_1
_sTargetName = "Target marker"

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2
  L2_2 = A1_2.number
  if L2_2 then
    L2_2 = A1_2.x
    if L2_2 then
      L2_2 = A1_2.y
      if L2_2 then
        L2_2 = A1_2.z
        if L2_2 then
          L2_2 = A1_2.r
          if L2_2 then
            L2_2 = A1_2.g
            if L2_2 then
              L2_2 = A1_2.b
              if L2_2 then
                L2_2 = A1_2.texture
                if L2_2 then
                  A0_2.AddObjective(A0_2, (_sTargetName .. tostring(A1_2.number)), A1_2.x, A1_2.y, A1_2.z, A1_2.r, A1_2.g, A1_2.b, 6, 6, A1_2.texture, A1_2.uGuid, true, nil, nil, 5)
              end
            end
          end
        end
      end
    end
    else
      A0_2.DeleteObjective(A0_2, (_sTargetName .. tostring(A1_2.number)))
    end
  end
end

HandleSetTargetMarker = L0_1
_sGPSName = "GPS Beacon Marker"

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2
  A0_2.AddObjective(A0_2, _sGPSName, A1_2.PosX, 0, A1_2.PosZ, 255, 255, 255, 10.666667, 10.666667, "MiniMap_Icon_GPS_Marker", nil, true, nil, nil, 4)
end

HandleSetGPSDest = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2
  A0_2.DeleteObjective(A0_2, _sGPSName)
end

HandleClearGPSDest = L0_1

function L0_1(A0_2, A1_2, A2_2, A3_2)
  local L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2
  L4_2 = A0_2.CustomData.bTrespass
  if L4_2 then
    return
  end
  L4_2 = type(A1_2)
  if "string" ~= L4_2 then
    return
  end
  L4_2 = type(A2_2)
  if "number" ~= L4_2 then
    A2_2 = nil
  end
  A3_2 = false
  if A3_2 then
    A0_2.SetText(A0_2, " ")
    L5_2 = A0_2.CustomData.oFlash.CustomData.bLoaded
    if L5_2 then
      A0_2.SetVisible(A0_2, true)
    end
  else
    L4_2 = A0_2.GetVisible(A0_2)
    if L4_2 then
      L4_2 = A0_2.CustomData.oFlash
      L4_2.SetVisible(L4_2, false)
      A0_2.SetEventHandler(A0_2, "GuiUpdate", nil)
      L10_2 = {}
      L10_2[1] = A1_2
      L10_2[2] = A2_2
      A0_2.AnimateToPoint(A0_2, A0_2.CustomData.nFadeOutPoint, 0.4, true, _HandleTransition, L10_2)
    end
    A0_2.SetVisible(A0_2, true)
    A0_2.SetText(A0_2, A1_2)
    L5_2 = A0_2
    A0_2.SetTranslucency(L5_2, 0)
    L4_2 = A0_2.CustomData
    L5_2 = A2_2 or L5_2
    if not A2_2 then
      L5_2 = 4
    end
    L4_2.nTime = L5_2
    L4_2 = A0_2.CustomData.nTime
    if 0 <= L4_2 then
      L10_2 = {}
      L10_2[1] = "GuiUpdate"
      L10_2[2] = _HandleMapLabelUpdateEvent
      A0_2.AnimateToPoint(A0_2, A0_2.CustomData.nFadeInPoint, 0.4, true, A0_2.SetEventHandler, L10_2)
    else
      A0_2.AnimateToPoint(A0_2, A0_2.CustomData.nFadeInPoint, 0.4, true)
    end
  end
end

ShowMapLabel = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2
  A0_2.SetVisible(A0_2, false)
  A0_2.Show(A0_2, A1_2, A2_2)
end

_HandleTransition = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2
  L1_2 = A0_2.CustomData
  L1_2.nTime = 0
  A0_2.Show = ShowMapLabel
  A0_2.SetVisible(A0_2, false)
  L1_2 = A0_2.CustomData
  L4_2 = {}
  L4_2.TranslucencyLevel = 255
  L1_2.nFadeInPoint = A0_2.AddAnimationPoint(A0_2, L4_2)
  L1_2 = A0_2.CustomData
  L4_2 = {}
  L4_2.TranslucencyLevel = 0
  L1_2.nFadeOutPoint = A0_2.AddAnimationPoint(A0_2, L4_2)
  L1_2 = MrxGuiBase.FlashWidget
  L1_2 = L1_2.new(L1_2)
  L3_2 = A0_2
  L2_2 = A0_2.GetLocation
  L2_2, L3_2, L4_2, L5_2 = L2_2(L3_2)
  L6_2 = (L3_2 + L5_2) * 0.5
  L7_2 = 0.6666667
  L4_2 = L4_2 + 10
  L10_2 = L4_2 - (850 * L7_2)
  L11_2 = L6_2 - (50 * (L7_2 / 2))
  L12_2 = L4_2
  L13_2 = L6_2 + (50 * (L7_2 / 2))
  L1_2.SetLocation(L1_2, L10_2, L11_2, L12_2, L13_2)
  L1_2.SetAnchoring(L1_2, "right", "top")
  L1_2.SetName(L1_2, "maplabeltext")
  L11_2 = A0_2
  L10_2 = A0_2.GetOwner
  L10_2, L11_2, L12_2, L13_2 = L10_2(L11_2)
  L1_2.SetOwner(L1_2, L10_2, L11_2, L12_2, L13_2)
  A0_2.AddChild(A0_2, L1_2)
  L8_2 = A0_2.CustomData
  L8_2.oFlash = L1_2
  L1_2.ParentWidget = A0_2
  MrxGuiBase.AddWidget(L1_2)
  L1_2.SetVisible(L1_2, false)
  MrxGuiManager.AddWidgetToHud(A0_2.GetOwner(A0_2), L1_2)
end

_HandleMapLabelInitialization = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2
  L1_2 = A0_2.CustomData
  L1_2.bLoaded = true
  A0_2.SetFlashEventHandler(A0_2, "close", _CompleteTextFlashAnimation)
end

_CompleteTextFlashLoad = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2
  L1_2 = A0_2.ParentWidget
  L1_2.SetVisible(L1_2, false)
end

_CompleteTextFlashAnimation = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L2_2 = A0_2.CustomData
  L2_2.nTime = (A0_2.CustomData.nTime - A1_2)
  L2_2 = A0_2.CustomData.nTime
  if L2_2 <= 0 then
    A0_2.SetEventHandler(A0_2, "GuiUpdate", nil)
    L2_2 = A0_2.CustomData
    L2_2.nTime = 0
    L8_2 = {}
    L8_2[1] = false
    A0_2.AnimateToPoint(A0_2, A0_2.CustomData.nFadeOutPoint, 0.4, true, A0_2.SetVisible, L8_2)
  end
end

_HandleMapLabelUpdateEvent = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L2_2 = A1_2.bTrespassing
  if L2_2 then
    A0_2.SetJustification(A0_2, "left")
    A0_2.Show(A0_2, "[red][Generic.Trespassing]", -1)
    L2_2 = A0_2.CustomData
    L2_2.bTrespass = true
    L2_2 = A0_2.CustomData.oFlash
    L2_2.SetVisible(L2_2, false)
    MrxTutorialManager.StartTutorial("Trespass", true)
  else
    L2_2 = A0_2.CustomData
    L2_2.bTrespass = false
    A0_2.SetJustification(A0_2, "right")
    L8_2 = {}
    L8_2[1] = false
    A0_2.AnimateToPoint(A0_2, A0_2.CustomData.nFadeOutPoint, 0.4, true, A0_2.SetVisible, L8_2)
  end
end

_HandleMapLabelTrespassEvent = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2
  A0_2.SetVisible(A0_2, false)
end

_HandleTrespassIconInit = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2
  L2_2 = A1_2.bTrespassing
  if L2_2 then
    L2_2 = A1_2.sFaction
    if L2_2 then
      L2_2 = _tIcons[A1_2.sFaction]
      if L2_2 then
        A0_2.SetVisible(A0_2, true)
        A0_2.SetTexture(A0_2, _tIcons[A1_2.sFaction])
    end
  end
  else
    A0_2.SetVisible(A0_2, false)
  end
end

_HandleTrespassIconEvent = L0_1

function L0_1()
  local L0_2, L1_2
  L0_2 = {}
  L0_2.All = "HUD_faction_AN"
  L0_2.Chi = "HUD_faction_CH"
  L0_2.Civ = "HUD_faction_CV"
  L0_2.Gur = "HUD_faction_GR"
  L0_2.Oil = "HUD_faction_OC"
  L0_2.Pir = "HUD_faction_PR"
  L0_2.Vz = "HUD_faction_VZ"
  _tIcons = L0_2
end

Init = L0_1
