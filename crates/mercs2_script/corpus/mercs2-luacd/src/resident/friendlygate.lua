import("MrxFactionManager")
import("MrxUtil")
import("MrxVoSequence")
local tFactions = {
  "VZ",
  "Allied",
  "China",
  "Guerilla",
  "OC",
  "Pirate",
  "PMC",
  "Civ"
}

function Init()
end

function Deinit()
end

_tLockedGates = {}

function LockGate(uGateGuid, bLock)
  local sGuidString = Sys.GuidToString(uGateGuid)
  if bLock then
    _tLockedGates[sGuidString] = true
  else
    _tLockedGates[sGuidString] = nil
  end
  if _tGates[uGateGuid] then
    _EvaluateCandidates(uGateGuid, nil, nil)
  end
end

function IsGateLocked(uGateGuid)
  local sGuidString = Sys.GuidToString(uGateGuid)
  return _tLockedGates[sGuidString] == true
end

_tGates = {}

function OnActivate(uGateGuid)
  Event.Create(Event.ObjectHibernation, {uGateGuid, "awake"}, Start, {uGateGuid})
end

function Start(uGateGuid)
  Event.Create(Event.TimerRelative, {2}, function()
    Vehicle.SetParts(uGateGuid, "LightFront", false)
    Vehicle.SetParts(uGateGuid, "LightBrake", true)
  end)
  if _tGates[uGateGuid] then
    return
  end
  local sFaction = MrxUtil.GetFaction(uGateGuid)
  if not sFaction then
    return
  end
  local uFilter = ObjectFilter.Create()
  ObjectFilter.SetFilter(uFilter, "Hero||(" .. sFaction .. "&&Vehicle)")
  local uNearEvent = CreateProxEvent(uFilter, uGateGuid)
  local sFactionAbbrev = MrxFactionManager.GetFactionAbbrev(sFaction)
  local uAttitudeChangeEvent = MrxFactionManager.CreatePersistentAttitudeChangeEvent({sFactionAbbrev, "Pmc"}, _EvaluateCandidates, {
    uGateGuid,
    nil,
    nil
  })
  _tGates[uGateGuid] = {
    uNearEvent = uNearEvent,
    uAttitudeChangeEvent = uAttitudeChangeEvent,
    tCandidates = {},
    uFilter = uFilter
  }
  _tGates[uGateGuid].DeathA = Event.Create(Event.ObjectHealth, {
    uGateGuid,
    "piece1a_propattach00",
    "<",
    1
  }, OnDeath, {uGateGuid})
  _tGates[uGateGuid].DeathB = Event.Create(Event.ObjectHealth, {
    uGateGuid,
    "piece1a_propattach01",
    "<",
    1
  }, OnDeath, {uGateGuid})
end

function OnDeath(uGateGuid, uNodeGuid)
  OnDeactivate(uGateGuid)
end

function OnDeactivate(uGateGuid)
  Vehicle.SetParts(uGateGuid, "LightFront", false)
  Vehicle.SetParts(uGateGuid, "LightBrake", false)
  local tGate = _tGates[uGateGuid]
  if not tGate then
    return
  end
  Event.Delete(tGate.uNearEvent)
  Event.Delete(tGate.uAttitudeChangeEvent)
  Event.Delete(tGate.DeathA)
  Event.Delete(tGate.DeathB)
  tGate.uFilter = nil
  for uGuid in pairs(tGate.tCandidates) do
    _RemoveCandidate(uGateGuid, uGuid)
  end
  _tGates[uGateGuid] = nil
end

function CreateProxEvent(uFilter, uGateGuid)
  return Event.Create(Event.ObjectProximity, {
    uFilter,
    uGateGuid,
    "<",
    20,
    false,
    false
  }, _EvaluateCandidates, {uGateGuid, true})
end

function _EvaluateCandidates(uGateGuid, bApproaching, vObjects)
  local tGate = _tGates[uGateGuid]
  if not tGate then
    return
  end
  if bApproaching then
    tGate.uNearEvent = nil
  end
  local tCandidates = tGate.tCandidates
  local sFaction = MrxUtil.GetFaction(uGateGuid)
  if type(vObjects) == "userdata" then
    vObjects = {vObjects}
  end
  if vObjects then
    for i, uGuid in ipairs(vObjects) do
      _RemoveCandidate(uGateGuid, uGuid)
      if bApproaching then
        local bValid = true
        if not tGate.uNearEvent then
          tGate.uNearEvent = CreateProxEvent(tGate.uFilter, uGateGuid)
        end
        if uGuid == Player.GetPrimaryCharacter() or uGuid == Player.GetSecondaryCharacter() then
          bValid = _TestAttitude(sFaction, uGuid)
        end
        if bValid then
          local uFarEvent = Event.Create(Event.ObjectProximity, {
            uGuid,
            uGateGuid,
            ">",
            40,
            false,
            false
          }, _EvaluateCandidates, {uGateGuid, false})
          local uDeathEvent = Event.Create(Event.ObjectDeath, {uGuid}, _EvaluateCandidates, {
            uGateGuid,
            nil,
            nil
          })
          tCandidates[uGuid] = {uFarEvent = uFarEvent, uDeathEvent = uDeathEvent}
        end
      end
    end
  end
  local nCandidates = 0
  for uGuid in pairs(tCandidates) do
    local bValid = true
    if uGuid == Player.GetPrimaryCharacter() or uGuid == Player.GetSecondaryCharacter() then
      bValid = _TestAttitude(sFaction, uGuid)
    end
    bValid = bValid and Object.IsAlive(uGuid)
    if bValid then
      nCandidates = nCandidates + 1
    else
      _RemoveCandidate(uGateGuid, uGuid)
    end
  end
  local bOpen = 0 < nCandidates
  bOpen = bOpen and not IsGateLocked(uGateGuid)
  _ChangeState(uGateGuid, bOpen)
end

function _RemoveCandidate(uGateGuid, uGuid)
  local tGate = _tGates[uGateGuid]
  local tCandidates = tGate.tCandidates
  if tCandidates[uGuid] then
    Event.Delete(tCandidates[uGuid].uFarEvent)
    Event.Delete(tCandidates[uGuid].uDeathEvent)
    tCandidates[uGuid] = nil
  end
end

function _ChangeState(uGateGuid, bOpen)
  local tGate = _tGates[uGateGuid]
  if tGate.bOpen ~= bOpen then
    tGate.bOpen = bOpen
    if bOpen then
      if not tGate.bPlayedVO then
        tGate.bPlayedVO = true
        local VO = {
          Allied = "AlliedSoldier01.Misc.GateYes01",
          China = "ChinaSoldier01.Misc.GateYes01",
          Guerilla = "GurSoldier01.Misc.GateYes01",
          OC = "OCSoldier01.Misc.GateYes01",
          VZ = "GurSoldier01.Misc.GateYes01"
        }
        local x, y, z = Object.GetPosition(uGateGuid)
        local sFaction = MrxUtil.GetFaction(uGateGuid)
        tSoldiers = Pg.FastCollectHumans(x, y, z, 25, sFaction)
        if tSoldiers and tSoldiers[1] and Object.IsAlive(tSoldiers[1]) and VO[sFaction] then
          MrxVoSequence.Start({
            {
              VO[sFaction],
              tSoldiers[1]
            }
          }, false, MrxVoSequence.knPriorityFreeplay)
        end
      end
      Object.OpenGate(uGateGuid)
      Vehicle.SetParts(uGateGuid, "LightFront", true)
      Vehicle.SetParts(uGateGuid, "LightBrake", false)
    else
      tGate.bPlayedVO = nil
      Object.CloseGate(uGateGuid)
      Vehicle.SetParts(uGateGuid, "LightFront", false)
      Vehicle.SetParts(uGateGuid, "LightBrake", true)
    end
  else
  end
end

function _TestAttitude(sGateFaction, uPlayerCharGuid)
  local sGateFactionAbbrev = MrxFactionManager.GetFactionAbbrev(sGateFaction)
  local sPlayerCharFaction = MrxFactionManager.GetPerceivedFaction(uPlayerCharGuid)
  local sPlayerCharFactionAbbrev = MrxFactionManager.GetFactionAbbrev(sPlayerCharFaction)
  local bResult = true
  local sTargetAttitude = "Friendly"
  if sPlayerCharFactionAbbrev == "Pmc" then
    bResult = bResult and MrxFactionManager.IsAttitudeMutable(sGateFactionAbbrev)
    sTargetAttitude = "Neutral"
  end
  bResult = bResult and MrxFactionManager.TestAttitude(sGateFactionAbbrev, sPlayerCharFactionAbbrev, ">=", sTargetAttitude)
  return bResult
end

function SaveSingleton()
  return _tLockedGates
end

function LoadSingleton(tLockedGates)
  if tLockedGates then
    _tLockedGates = tLockedGates
  end
end
