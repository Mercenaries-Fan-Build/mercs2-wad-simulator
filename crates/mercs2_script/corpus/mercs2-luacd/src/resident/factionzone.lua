inherit("Inheritable")
import("MrxGui")

function Init()
  _tAssociationMap = {
    [String.GetHash("Allied")] = {sFaction = "All"},
    [String.GetHash("China")] = {sFaction = "Chi"},
    [String.GetHash("Civ")] = {sFaction = "Civ"},
    [String.GetHash("Guerilla")] = {sFaction = "Gur"},
    [String.GetHash("OC")] = {sFaction = "Oil"},
    [String.GetHash("Pirate")] = {sFaction = "Pir"},
    [String.GetHash("PMC")] = {sFaction = "Pmc"},
    [String.GetHash("VZ")] = {sFaction = "Vz"}
  }
end

function OnActivate(uGuid, uRuntimeOwner, iArg)
  local oPrototype = getfenv()
  local oInstance = oPrototype:Create(uGuid, uRuntimeOwner)
end

function Create(oPrototype, uGuid, uRuntimeOwner)
  local oSelf = Inheritable.Create(oPrototype, uGuid, uRuntimeOwner)
  oSelf.uFaction = Ai.GetFactionGuid(oSelf.uGuid)
  local sAssociation = Object.GetName(oSelf.uFaction)
  for k, v in pairs(_tAssociationMap[sAssociation]) do
    oSelf[k] = v
  end
  if not oSelf.bActive then
    oSelf:Enable()
  end
end

function Delete(oSelf)
  if oSelf.bActive then
    oSelf:Disable()
  end
  Inheritable.Delete(oSelf)
end

function BoundaryCallback(oSelf, uObjectGuid, uBoundaryGuid, sAction)
  if not (sAction ~= "enter" or oSelf.bTrespassing) or sAction == "exit" and oSelf.bTrespassing then
    local bTrespassing = sAction == "enter"
    local tEvent = {
      EventType = "TrespassStateChange",
      bTrespassing = bTrespassing,
      sFaction = oSelf.sFaction
    }
    MrxGui.SendEvent(tEvent)
    oSelf.bTrespassing = bTrespassing
  end
end

function Enable(oSelf)
  local tRegionParam = {
    uGuid = oSelf.uGuid,
    nRed = 64,
    nGreen = 0,
    nBlue = 0,
    nAlpha = 160
  }
  Hud.Radar:AddLineRegion(tRegionParam)
  Pda.Map:AddLineRegion(tRegionParam)
  oSelf.BoundaryEvent = Event.CreatePersistent(Event.Boundary, {
    Player.GetLocalCharacter(),
    oSelf.uGuid,
    "any",
    false
  }, BoundaryCallback, {oSelf})
  oSelf.bActive = true
end

function Disable(oSelf)
  local tRegionParam = {
    uGuid = oSelf.uGuid
  }
  Hud.Radar:RemoveLineRegion(tRegionParam)
  Pda.Map:RemoveLineRegion(tRegionParam)
  Event.Delete(oSelf.BoundaryEvent)
  if oSelf.bTrespassing then
    local tEvent = {
      EventType = "TrespassStateChange",
      bTrespassing = false,
      sFaction = oSelf.sFaction
    }
    MrxGui.SendEvent(tEvent)
    oSelf.bTrespassing = false
  end
  oSelf.bActive = nil
end
