inherit("Inheritable")
import("MrxUtil")
bSticky = false

function OnActivate(uGuid, uRuntimeOwner, iArg)
  Debug.Printf("Blippable.OnActivate")
  Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, Awake, {uGuid, iArg})
end

function Awake(uGuid, iArg)
  local oPrototype = getfenv()
  local oInstance = oPrototype:Create(uGuid, iArg)
end

function Delete(self)
  if self.bActive then
    self:ClearBlipped()
  end
  Inheritable.Delete(self)
end

function SetBlipped(self)
  self:AddObjective()
  self.bActive = true
end

function ClearBlipped(self)
  self:RemoveObjective()
  self.bActive = nil
end

function AddObjective(self, bFlash)
  for i, Guid in pairs(tHiddenGuids) do
    if self.uGuid == Guid then
      return
    end
  end
  local tColor = bFlash and self.tFlash or self.tColor or {
    255,
    51,
    51
  }
  local nWidth = self.nWidth or self.nSize
  local nHeight = self.nHeight or self.nSize
  Hud.Radar:AddObjective({
    sName = self.sName,
    nR = tColor[1],
    nG = tColor[2],
    nB = tColor[3],
    nWidth = nWidth,
    nHeight = nHeight,
    sTexture = self.sTexture,
    uGuid = self.uGuid,
    bSticky = self.bSticky,
    bRotate = self.bRotate,
    bOriented = self.bOriented,
    nSortOrder = self.nSortOrder,
    bDontNetSync = not self.bNetSync
  })
  local tMarker = self.tMarker
  if tMarker then
    if self.uMarkerGuid then
      if Net.IsServer() and self.bNetSync then
        Net.SendEvent_RemoveMarkerObjective(self.uMarkerGuid)
      end
      Marker.Remove(self.uMarkerGuid)
    end
    local sMarkerTexture = tMarker.sTexture or "HUD_objective_destroy"
    local nMarkerSize = tMarker.nSize or 32
    local r, g, b = MrxUtil.GetSecondaryObjectiveRgb()
    local tMarkerColor = bFlash and tMarker.tFlash or tMarker.tColor or {
      r,
      g,
      b,
      255
    }
    local tMarkerVerticalOffset = tMarker.nVerticalOffset or 0
    local nNearDist = tMarker.nNearDist or 140
    local nFarDist = tMarker.nFarDist or 150
    local nClampDist = tMarker.nClampDist or -1
    local sGroup = tMarker.sGroup or ""
    local bJust2DCheck = tMarker.bJust2DCheck or false
    self.uMarkerGuid = Marker.AddBlip(self.uGuid, sMarkerTexture, nMarkerSize, tMarkerColor[1], tMarkerColor[2], tMarkerColor[3], tMarkerColor[4], tMarkerVerticalOffset, nNearDist, nFarDist, nClampDist, sGroup, bJust2DCheck)
    if Net.IsServer() and self.bNetSync then
      Net.SendEvent_AddMarkerObjective(self.uGuid, self.uMarkerGuid, tMarkerColor[1], tMarkerColor[2], tMarkerColor[3], tMarkerVerticalOffset, 0, 1, 0.5 * nMarkerSize, false, nNearDist, nFarDist)
    end
  end
end

tHiddenGuids = {}

function HideMarker(uGuid)
  local oTarget = tInstance[uGuid]
  if oTarget then
    oTarget:RemoveObjective()
  end
  table.insert(tHiddenGuids, uGuid)
end

function RemoveObjective(self)
  Hud.Radar:RemoveObjective({
    sName = self.sName,
    bDontNetSync = not self.bNetSync
  })
  if self.uMarkerGuid then
    Marker.Remove(self.uMarkerGuid)
    if Net.IsServer() and not self.bDontNetSync then
      Net.SendEvent_RemoveMarkerObjective(self.uMarkerGuid)
    end
    self.uMarkerGuid = nil
  end
end
