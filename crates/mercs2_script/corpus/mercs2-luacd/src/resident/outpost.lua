import("MrxGui")
import("MrxPmc")
import("MrxUtil")
import("MrxFactionManager")
import("MrxGuiInterface")
import("MrxOutpostManager")
import("MrxSupportData")
import("MrxVoSequence")
_tOutposts = {}
_tSupportRefCount = {
  Allied = 0,
  Pirate = 0,
  China = 0,
  Guerilla = 0,
  OC = 0
}
tDefaultSupport = {
  Allied = "SoldierDelivery_AL",
  Pirate = "SoldierDelivery_PR",
  China = "SoldierDelivery_CH",
  Guerilla = "SoldierDelivery_GR",
  OC = "SoldierDelivery_OC"
}
sDefenders = "VZ"
sAttackers = "OC"
nCaptureTime = 10
nStartRange = 150
nSpawnTime = 20
iCashReward = 5000
tDBSpawners = {}
fCapturedCallback = nil
tCapturedCallbackData = {}
fDestroyedCallback = nil
tDestroyedCallbackData = {}
fUpdatedCallback = nil
tUpdatedCallbackData = {}
tCapturePts = {}
nStartingHealth = 3
nRusherQuota = 1

function Find(uGuid)
  return uGuid and _tOutposts[uGuid]
end

function Create(oPrototype, tArgs)
  if Net.IsClient() then
    return
  end
  local uGuid = Pg.GetGuidByName(tArgs.sOutpost)
  if not uGuid then
    Debug.Printf("ERROR: Outpost " .. tArgs.sOutpost .. " not found")
    return
  end
  local oSelf = {}
  setmetatable(oSelf, {__index = oPrototype})
  oSelf.uGuid = uGuid
  for key, value in pairs(tArgs) do
    oSelf[key] = value
  end
  if tArgs.sBoundary then
    table.insert(oSelf.tCapturePts, tArgs.sBoundary)
  end
  oSelf.tEvents = {}
  oSelf:Activate()
  oSelf.tEvents.OnDeath = Event.Create(Event.ObjectDeath, {
    oSelf.uGuid
  }, oSelf.OnDeath, {oSelf})
  oSelf.tEvents.OnJoin = Event.CreatePersistent(Event.ScriptEvent, {
    "mpPlayerJoin",
    function(tData)
      return Net.IsServer() and not Player.IsLocal(tData[1])
    end
  }, SendPlayerJoinEvents, {oSelf})
  _tOutposts[oSelf.uGuid] = oSelf
  return oSelf
end

function Delete(oSelf)
  if Net.IsClient() then
    return
  end
  if oSelf.bActive then
    oSelf:Deactivate()
  end
  for _, uEvent in pairs(oSelf.tEvents) do
    Event.Delete(uEvent)
  end
  _tOutposts[oSelf.uGuid] = nil
end

function OnDeath(oSelf)
  if Net.IsClient() then
    return
  end
  oSelf:Destroyed()
end

function Activate(oSelf)
  if Net.IsClient() then
    return
  end
  oSelf.nCurrentHealth = oSelf.nStartingHealth
  oSelf:SetDBFaction(oSelf.sDefenders)
  oSelf:TweakDBs("on")
  oSelf.tMarkers = {}
  MrxSupportData.AddFreebie(tDefaultSupport[oSelf.sAttackers])
  _tSupportRefCount[oSelf.sAttackers] = _tSupportRefCount[oSelf.sAttackers] + 1
  oSelf.tRushers = {}
  oSelf.tBlacklist = {}
  oSelf.tRusherSuccess = {}
  oSelf.nAttackers = 0
  oSelf.nDefenders = 0
  oSelf.tEvents.OnTimer = Event.CreatePersistent(Event.TimerRelative, {1}, oSelf.TimerTick, {oSelf})
  oSelf.bActive = true
  oSelf:UpdateHealthDisplay()
end

function Deactivate(oSelf)
  if Net.IsClient() then
    return
  end
  oSelf.bActive = nil
  Event.Delete(oSelf.tEvents.OnTimer)
  oSelf.tEvents.OnTimer = nil
  oSelf:CancelCallForAttackers()
  oSelf:CancelCallForDefenders()
  oSelf:IdleAllRushers()
  _tSupportRefCount[oSelf.sAttackers] = _tSupportRefCount[oSelf.sAttackers] - 1
  if _tSupportRefCount[oSelf.sAttackers] <= 0 then
    MrxSupportData.RemoveFreebie(tDefaultSupport[oSelf.sAttackers])
  end
  oSelf:ClearHealthDisplay()
  oSelf:TweakDBs("off")
end

function TimerTick(oSelf)
  if oSelf.nCurrentHealth <= 0 then
    oSelf:Captured()
  else
    oSelf:CallForAttackers()
    if oSelf.nCurrentHealth < oSelf.nStartingHealth then
      oSelf:CallForDefenders()
    else
      oSelf:CancelCallForDefenders()
    end
  end
end

function Captured(oSelf)
  oSelf:SetDBFaction(oSelf.sAttackers)
  oSelf.bCaptured = true
  MrxOutpostManager.OutpostStatusChange(oSelf.uGuid, MrxOutpostManager.knStatusCaptured)
  MrxUtil.CallWithOptionalArgs(oSelf.fCapturedCallback, {
    unpack(oSelf.tCapturedCallbackData),
    oSelf.uGuid
  })
  oSelf:Delete()
end

function Destroyed(oSelf)
  oSelf.bDestroyed = true
  MrxOutpostManager.OutpostStatusChange(oSelf.uGuid, MrxOutpostManager.knStatusDestroyed)
  MrxUtil.CallWithOptionalArgs(oSelf.fDestroyedCallback, {
    unpack(oSelf.tDestroyedCallbackData),
    oSelf.uGuid
  })
  oSelf:Delete()
end

function UpdateHealthDisplay(oSelf)
  if not oSelf.bActive then
    return
  end
  UpdateHealthDisplayHelper(oSelf.nStartingHealth, oSelf.nCurrentHealth)
end

function ClearHealthDisplay(oSelf)
  ClearHealthDisplayHelper()
end

NETEVENT_UPDATEHEALTHDISPLAY = 0
NETEVENT_CLEARHEALTHDISPLAY = 1

function UpdateHealthDisplayHelper(nStartingHealth, nCurrentHealth)
  local sOutput = "[white][Generic.OutpostHealth]:"
  for i = 1, nStartingHealth - nCurrentHealth do
    sOutput = sOutput .. " [green]X"
  end
  for i = 1, nCurrentHealth do
    sOutput = sOutput .. " [red]X"
  end
  Hud.ObjectiveTray:SetSlotToText({
    nSlot = 1,
    sText = sOutput,
    bDontNetSync = true
  })
  if Net.IsServer() then
    Net.SendCustomEvent("Outpost", NETEVENT_UPDATEHEALTHDISPLAY, {nStartingHealth, nCurrentHealth})
  end
end

function ClearHealthDisplayHelper()
  Hud.ObjectiveTray:SetSlotToText({
    nSlot = 1,
    sText = " ",
    bDontNetSync = true
  })
  if Net.IsServer() then
    Net.SendCustomEvent("Outpost", NETEVENT_CLEARHEALTHDISPLAY, {})
  end
end

function NetEventCallback(nEventId, tArgs)
  if nEventId == NETEVENT_UPDATEHEALTHDISPLAY then
    UpdateHealthDisplayHelper(tArgs[1], tArgs[2])
  elseif nEventId == NETEVENT_CLEARHEALTHDISPLAY then
    ClearHealthDisplayHelper()
  end
end

function SendPlayerJoinEvents(oSelf)
  if oSelf.bActive then
    Net.SendCustomEvent("Outpost", NETEVENT_UPDATEHEALTHDISPLAY, {
      oSelf.nStartingHealth,
      oSelf.nCurrentHealth
    })
  end
end

function TweakDBs(oSelf, sState)
  for i, spawner in ipairs(oSelf.tDBSpawners) do
    local uDBGuid = Pg.GetGuidByName(spawner)
    if uDBGuid then
      Ai.TweakAttachedSpawners(uDBGuid, {SpawnerState = sState})
    end
  end
end

function SetDBFaction(oSelf, sFaction)
  local tGroups = {
    Ground = "Ground",
    Balcony = "Balcony",
    G1 = "AA",
    Rooftop = "Balcony",
    AA = "AA",
    Window = "Balcony"
  }
  for i, spawner in ipairs(oSelf.tDBSpawners) do
    local uDBGuid = Pg.GetGuidByName(spawner)
    if uDBGuid then
      for group, spawnlist in pairs(tGroups) do
        Debug.Printf("Setting \"" .. tostring(group) .. "\" to \"" .. spawnlist .. "\"")
        local sList = tostring("Spawnlist (" .. sFaction .. " " .. spawnlist .. ")")
        Ai.TweakAttachedSpawnersInGroup(uDBGuid, group, {
          SpawnList = sList,
          SecondsPerCycle = oSelf.nSpawnTime
        })
      end
    end
  end
end

function CallForAttackers(oSelf)
  oSelf:CallForRushers(true)
end

function CallForDefenders(oSelf)
  oSelf:CallForRushers(false)
end

function CallForRushers(oSelf, bAttackers)
  local sFaction = bAttackers and oSelf.sAttackers or oSelf.sDefenders
  local sCapturePt = oSelf:GetCapturePoint()
  local uCapturePt = Pg.GetGuidByName(sCapturePt)
  local x, y, z = Object.GetPosition(uCapturePt)
  if oSelf:IsRusherQuotaMet(bAttackers) then
    Debug.Printf("@@@@@@@@@@ CallForRushers: Rusher already active, skipping")
    return
  end
  local tRushers = Pg.FastCollectHumans(x, y, z, 50, sFaction)
  local tRushersWithData = {}
  for i, uRusher in pairs(tRushers) do
    local bVehicleAllowed = true
    local uRusherSeat, uRusherVehicle = Object.InVehicle(uRusher)
    if bVehicleAllowed and uRusherVehicle then
      local vehType = Object.GetPhysicsType(uRusherVehicle)
      if vehType == "helicopter" then
        bVehicleAllowed = false
      end
    end
    if bVehicleAllowed and uRusherVehicle then
      for i, uPlayer in ipairs(Player.GetAllPlayers()) do
        local uVehicle = Player.GetControlledObject(uPlayer)
        if uVehicle == uRusherVehicle then
          bVehicleAllowed = false
        end
      end
    end
    if bVehicleAllowed and uRusherSeat then
      local tSeatParams = Vehicle.GetSeatParams(uRusherSeat)
      if tSeatParams and tSeatParams.IsGunner then
        bVehicleAllowed = false
      end
    end
    if bVehicleAllowed and not oSelf.tRusherSuccess[uRusher] and not oSelf.tBlacklist[uRusher] and not Ai.GetState({AIGuid = uRusher, State = "NoCapture"}) and sCapturePt then
      local tRusherData = {uRusher = uRusher, uInVehicle = uRusherVehicle}
      table.insert(tRushersWithData, tRusherData)
    end
  end
  if table.getn(tRushersWithData) == 0 then
    for i, uRusher in pairs(tRushers) do
      if oSelf.tBlacklist[uRusher] then
        oSelf.tBlacklist[uRusher] = false
      end
    end
  end
  local bRushers = false
  for i, tRusherData in ipairs(tRushersWithData) do
    bRushers = true
    if not tRusherData.uInVehicle then
      oSelf:IssueCommand(tRusherData.uRusher, sCapturePt, bAttackers)
    elseif tRusherData.uInVehicle and table.getn(tRushersWithData) == i then
      local uVehicle = Vehicle.GetFromRider(tRusherData.uRusher)
      Vehicle.Exit(uVehicle, tRusherData.uRusher, true)
      oSelf:IssueCommand(tRusherData.uRusher, sCapturePt, bAttackers)
    end
    if oSelf:IsRusherQuotaMet(bAttackers) then
      break
    end
  end
  if not bRushers then
  end
end

function GetCapturePoint(oSelf)
  if oSelf.sCapturePt then
    return oSelf.sCapturePt
  elseif oSelf.tCapturePts then
    return oSelf.tCapturePts[1]
  end
end

function IssueCommand(oSelf, uRusher, sCapturePt, bAttacker)
  if oSelf.tRushers[uRusher] then
    return false
  end
  Debug.Printf("@@@@@@@@@@ IssueCommand: Sending rusher " .. tostring(uRusher) .. "(" .. tostring(bAttacker) .. ") to " .. tostring(sCapturePt))
  local uMoveGoal = Ai.Goal({
    AIGuid = uRusher,
    Goal = "MoveTo",
    Target = Pg.GetGuidByName(sCapturePt),
    Priority = "hiPri",
    Callback = oSelf.RusherGoalFulfilled,
    CallbackData = {oSelf},
    Force = true
  })
  if not uMoveGoal then
    return false
  end
  PlayerRusherVO(uRusher)
  Ai.SetPriorityTarget(uRunnerGuid)
  local uTimeoutEvent = Event.Create(Event.TimerRelative, {20}, oSelf.RusherFailed, {oSelf, uRusher})
  local uDeathEvent = Event.Create(Event.ObjectDeath, {uRusher}, oSelf.RescindRusherCommand, {oSelf, uRusher})
  oSelf.tRushers[uRusher] = {
    uMoveGoal = uMoveGoal,
    uTimeoutEvent = uTimeoutEvent,
    uDeathEvent = uDeathEvent,
    bAttacker = bAttacker
  }
  oSelf:MarkRusher(uRusher, true)
  if bAttacker then
    oSelf.nAttackers = oSelf.nAttackers + 1
  else
    oSelf.nDefenders = oSelf.nDefenders + 1
  end
  Debug.Printf("@@@@@@@@@@ IssueCommand: Rusher command given (" .. tostring(uRusher) .. ")")
end

function RusherGoalFulfilled(oSelf, uRusher, nState)
  if nState == 0 then
    oSelf:RescindRusherCommand(uRusher)
    return
  end
  local uFaction = Ai.GetFactionGuid(uRusher)
  local uDefenseFaction = Pg.GetGuidByName(oSelf.sDefenders)
  local uOffenseFaction = Pg.GetGuidByName(oSelf.sAttackers)
  local bAttackers
  if uFaction == uOffenseFaction then
    bAttackers = true
  elseif uFaction == uDefenseFaction then
    bAttackers = false
  else
    ASSERT(false)
  end
  local nDelta
  if not Object.InSeat(uRusher) and not oSelf.tRusherSuccess[uRusher] and not oSelf.tBlacklist[uRusher] then
    if bAttackers then
      nDelta = -1
    else
      nDelta = 1
    end
  end
  local bSuccess
  if nDelta then
    bSuccess = oSelf:HealthChange(nDelta)
  end
  if bSuccess then
    oSelf.tRusherSuccess[uRusher] = true
    oSelf:RescindRusherCommand(uRusher)
    Debug.Printf("@@@@@@@@@@ Removing Rusher")
    Object.FadeOut(uRusher, 0.5, true)
  end
end

function CancelCallForAttackers(oSelf)
  oSelf:CancelCallForRushers(true)
end

function CancelCallForDefenders(oSelf)
  oSelf:CancelCallForRushers(false)
end

function CancelCallForRushers(oSelf, bAttackers)
  for uRusher, tRusherData in pairs(oSelf.tRushers) do
    if tRusherData.bAttacker == bAttackers then
      oSelf:RescindRusherCommand(uRusher)
    end
  end
end

function RusherFailed(oSelf, uRusher)
  oSelf.tBlacklist[uRusher] = true
  oSelf:RescindRusherCommand(uRusher)
end

function RescindRusherCommand(oSelf, uRusher)
  local tRusherData = oSelf.tRushers[uRusher]
  if not tRusherData then
    return
  end
  Ai.RemoveGoal(uRusher, tRusherData.uMoveGoal)
  Event.Delete(tRusherData.uTimeoutEvent)
  Event.Delete(tRusherData.uDeathEvent)
  oSelf:MarkRusher(uRusher, false)
  if tRusherData.bAttacker then
    oSelf.nAttackers = oSelf.nAttackers - 1
  else
    oSelf.nDefenders = oSelf.nDefenders - 1
  end
  oSelf.tRushers[uRusher] = nil
end

function MarkRusher(oSelf, uRusher, bEnable)
  local tRusherData = oSelf.tRushers[uRusher]
  if not tRusherData then
    return
  end
  if bEnable then
    local sFaction
    if tRusherData.bAttacker then
      sFaction = oSelf.sAttackers
    else
      sFaction = oSelf.sDefenders
    end
    local sFactionAbbrev = MrxFactionManager.GetFactionAbbrev(sFaction)
    local sTexture
    if sFactionAbbrev then
      sTexture = MrxFactionManager.GetMarkerTexture(sFactionAbbrev)
    end
    if sTexture then
      tRusherData.uMarker = Marker.AddBlip(uRusher, sTexture, 32, 255, 255, 255, 255, 2, nil, nil, 32, nil, true)
      if Net.IsServer() then
        local nIndex = MrxUtil.MarkerGetIndexByName_World(sTexture)
        Net.SendEvent_AddMarkerObjective(uRusher, tRusherData.uMarker, 255, 255, 255, 2, nIndex, 1, 16, false, nil, nil)
      end
    end
    if sFactionAbbrev then
      local tFactionAbbrevToTexture = {
        Pmc = "MiniMap_Icon_Faction_PMC",
        Gur = "MiniMap_Icon_Faction_GR",
        Oil = "MiniMap_Icon_Faction_OC",
        Pir = "MiniMap_Icon_Faction_PR",
        All = "MiniMap_Icon_Faction_AN",
        Chi = "MiniMap_Icon_Faction_CH",
        Vza = "MiniMap_Icon_Faction_VZ"
      }
      local sRadarTexture = tFactionAbbrevToTexture[sFactionAbbrev]
      Hud.Radar:AddObjective({
        sName = tostring(uRusher),
        uGuid = uRusher,
        nR = 255,
        nG = 255,
        nB = 255,
        nWidth = 6,
        nHeight = 6,
        sTexture = sRadarTexture,
        bSticky = true,
        nSortOrder = tRusherData.bAttacker and 4 or 2
      })
    end
  else
    if tRusherData.uMarker then
      Marker.Remove(tRusherData.uMarker)
    end
    Hud.Radar:RemoveObjective({
      sName = tostring(uRusher)
    })
    if Net.IsServer() then
      if tRusherData.uMarker then
        Net.SendEvent_RemoveMarkerObjective(tRusherData.uMarker)
      end
      Net.SendEvent_RemoveRadarObjective(tostring(uRusher))
    end
  end
end

function IsRusherQuotaMet(oSelf, bAttackers)
  local sCountVarName = "nDefenders"
  if bAttackers then
    sCountVarName = "nAttackers"
  end
  return oSelf[sCountVarName] >= oSelf.nRusherQuota
end

function HealthChange(oSelf, nDelta)
  if nDelta < 0 and 0 >= oSelf.nCurrentHealth or 0 < nDelta and oSelf.nCurrentHealth >= oSelf.nStartingHealth then
    return false
  end
  oSelf.nCurrentHealth = oSelf.nCurrentHealth + nDelta
  oSelf:UpdateHealthDisplay()
  if oSelf.fUpdatedCallback then
    MrxUtil.CallWithOptionalArgs(oSelf.fUpdatedCallback, {
      unpack(oSelf.tUpdatedCallbackData)
    })
  end
  if 0 >= oSelf.nCurrentHealth then
    oSelf:Captured()
  end
  return true
end

function IdleAllRushers(oSelf, bKilled)
  local x, y, z = Object.GetPosition(oSelf.uGuid)
  if x then
    local tRushers = Pg.FastCollectHumans(x, y, z, 30, oSelf.sAttackers .. "||" .. oSelf.sDefenders)
    for i, rusher in ipairs(tRushers) do
      Ai.Role({
        AIGuid = rusher,
        Role = "Idle",
        Priority = "loPri"
      })
    end
  end
end

function GetFactionSupportName(sFaction)
  return MrxSupportData.GetFreebieName(tDefaultSupport[sFaction])
end

function PlayerRusherVO(uRusher)
  local sFaction = MrxUtil.GetFaction(uRusher)
  if not sFaction then
    return
  end
  if Object.HasLabel(uRusher, "Female") then
    sFaction = sFaction .. "F"
  end
  local VO = {
    Allied = {
      "Mirron01_Soldier_AI Advance_x_x_x_x_x_01",
      "Mirron01_Soldier_AI Advance_x_x_x_x_x_03",
      "Matt01_Soldier_AI Advance_x_x_x_x_x_01",
      "Matt01_Soldier_AI Advance_x_x_x_x_x_03",
      "Allied NY_Richard01_Soldier_AI Advance_x_x_x_x_x_01",
      "Allied NY_Richard01_Soldier_AI Advance_x_x_x_x_x_03",
      "Mirron01_Soldier_AI Attack_Building_x_x_x_x_01",
      "Matt01_Soldier_AI Attack_Building_x_x_x_x_01",
      "Allied NY_Richard01_Soldier_AI Attack_Building_x_x_x_x_01"
    },
    Guerilla = {
      "VZSoldierArc_Zev01_VZ Soldier_AI Advance_x_x_x_x_x_03",
      "VZSoldierArc_Zev01_Soldier_AI Advance_x_x_x_x_x_02",
      "VZSoldierArc_Zev01_Soldier_AI Advance_x_x_x_x_x_03",
      "VZSoldierArc_Zev01_Soldier_AI Attack_Building_x_x_x_x_01"
    },
    GuerillaF = {
      "GuerillaSoldier_Rebecca01_Soldier_AI Advance_x_x_x_x_x_00",
      "GuerillaSoldier_Rebecca01_Soldier_AI Advance_x_x_x_x_x_01",
      "GuerillaSoldier_Rebecca01_Soldier_AI Advance_x_x_x_x_x_03",
      "GuerillaSoldier_Rebecca01_Soldier_AI Attack_Building_x_x_x_x_01"
    },
    VZ = {
      "VZSoldierArc_Zev01_VZ Soldier_AI Advance_x_x_x_x_x_03",
      "VZSoldierArc_Zev01_Soldier_AI Advance_x_x_x_x_x_02",
      "VZSoldierArc_Zev01_Soldier_AI Advance_x_x_x_x_x_03",
      "VZSoldierArc_Zev01_Soldier_AI Attack_Building_x_x_x_x_01"
    },
    OC = {
      "Generic OC Soldier_Derek01_Soldier_AI Advance_x_x_x_x_x_01",
      "Generic OC Soldier_Derek01_Soldier_AI Advance_x_x_x_x_x_03",
      "Generic OC Soldier_Keith01_Soldier_AI Advance_x_x_x_x_x_01",
      "Generic OC Soldier_Keith01_Soldier_AI Advance_x_x_x_x_x_03",
      "Generic OC Soldier_Derek01_Soldier_AI Attack_Building_x_x_x_x_01",
      "Generic OC Soldier_Keith01_Soldier_AI Attack_Building_x_x_x_x_01"
    },
    China = {
      "China Soldier_Ming01_Soldier_AI Advance_x_x_x_x_x_00",
      "China Soldier_Ming01_Soldier_AI Advance_x_x_x_x_x_01",
      "China Soldier_Ming01_Soldier_AI Advance_x_x_x_x_x_03",
      "China Soldier_Ming01_Soldier_AI Advance_x_x_x_x_x_05",
      "China Soldier_Ming01_Soldier_AI Attack_Building_x_x_x_x_01"
    },
    Pirate = {
      "Pirate Thug_Darryl01_Soldier_AI Advance_x_x_x_x_x_01",
      "Pirate Thug_Darryl01_Soldier_AI Advance_x_x_x_x_x_03",
      "Pirate Thug_Jonell01_Soldier_AI Advance_x_x_x_x_x_01",
      "Pirate Thug_Jonell01_Soldier_AI Advance_x_x_x_x_x_03",
      "Pirate Thug_Darryl01_Soldier_AI Attack_Building_x_x_x_x_01"
    },
    PirateF = {
      "Pirate Thug_Jonell01_Soldier_AI Advance_x_x_x_x_x_00",
      "Pirate Thug_Jonell01_Soldier_AI Advance_x_x_x_x_x_01",
      "Pirate Thug_Jonell01_Soldier_AI Advance_x_x_x_x_x_03",
      "Pirate Thug_Jonell01_Soldier_AI Attack_Building_x_x_x_x_01"
    }
  }
  if VO[sFaction] then
    local sCue = MrxUtil.GetRandomTableElement(VO[sFaction])
    MrxVoSequence.Start(sCue, nil, MrxVoSequence.knPriorityFreeplay)
  end
end
