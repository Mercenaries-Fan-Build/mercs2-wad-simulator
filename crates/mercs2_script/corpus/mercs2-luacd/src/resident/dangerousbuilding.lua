import("MrxGui")
import("MrxPmc")
import("MrxUtil")
tDBs = {}
local nDBCount = 0
local nMaxDBs = 8
local nDefaultRarity = 16
local nGlobalRarity = nDefaultRarity
local nDefaultCashReward = 0

function OnActivate(uGuid, uRuntimeOwner, iArg)
  if iArg and 0 < iArg then
    return
  end
  tDBs[uGuid] = tDBs[uGuid] or {}
  tDBs[uGuid].WakeEvent = Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, Start, {uGuid})
end

function Start(uGuid)
  tDBs[uGuid].WakeEvent = nil
  if not Object.IsAlive(uGuid) then
    return
  end
  if tDBs[uGuid] and tDBs[uGuid].WakeupFunc then
    MrxUtil.CallWithOptionalArgs(tDBs[uGuid].WakeupFunc, {uGuid})
  end
  if not Net.IsClient() then
    if Object.HasLabel(uGuid, "Occupied") then
      SetupOccupied(uGuid, false)
      return
    end
    local iRarity = nGlobalRarity
    if tDBs[uGuid] and tDBs[uGuid].Rarity then
      iRarity = tDBs[uGuid].Rarity
    end
    if iRarity < 0 or nDBCount >= nMaxDBs or tDBs[uGuid] and tDBs[uGuid].Active then
      return
    end
    local Random = math.randf() * nMaxDBs * iRarity
    if Random < nMaxDBs then
      TurnOnRandomDB(uGuid, false)
    else
    end
  end
end

function SetupOccupied(uGuid, bForceOnClient)
  tDBs[uGuid] = tDBs[uGuid] or {}
  if Net.IsClient() then
    if not bForceOnClient then
      return
    else
    end
  elseif Net.IsServer() then
    Net.SendEvent_SetOccupiedDangerousBuilding(uGuid)
    tDBs[uGuid].HealthEvent = Event.Create(Event.ObjectHealth, {
      uGuid,
      "<",
      Object.GetHealth(uGuid)
    }, TurnOn, {
      uGuid,
      true,
      false,
      false
    })
  end
  if not tDBs[uGuid].Blip then
    tDBs[uGuid].Blip = true
    Hud.Radar:AddObjective({
      sName = "db_" .. tostring(uGuid),
      nR = 170,
      nG = 170,
      nB = 170,
      nWidth = 8,
      nHeight = 8,
      sTexture = "temp_radar_icon_db",
      uGuid = uGuid,
      bSticky = false,
      bDontNetSync = true,
      nSortOrder = 3
    })
  end
end

function TurnOn(uGuid, bRadar, bPermanent, bForceOnClient)
  if Net.IsClient() and not bForceOnClient then
    return
  end
  local tGuids = ConvertToTableOfGuids(uGuid)
  for i, uGuid in pairs(tGuids) do
    if not (uGuid and Object.IsAlive(uGuid)) or tDBs[uGuid] and tDBs[uGuid].Active == true then
      return
    end
    tDBs[uGuid] = tDBs[uGuid] or {}
    if Net.IsClient() then
      tDBs[uGuid].Permanent = bPermanent
    else
      tDBs[uGuid].Permanent = tDBs[uGuid].Permanent or bPermanent
    end
    tDBs[uGuid].Active = true
    Ai.TweakAttachedSpawners(uGuid, {SpawnerState = "on"})
    if bRadar then
      if tDBs[uGuid].Blip then
        Hud.Radar:RemoveObjective({
          sName = "db_" .. tostring(uGuid),
          bDontNetSync = true
        })
      end
      Hud.Radar:AddObjective({
        sName = "db_" .. tostring(uGuid),
        nR = 250,
        nG = 0,
        nB = 0,
        nWidth = 8,
        nHeight = 8,
        sTexture = "temp_radar_icon_dbactive",
        uGuid = uGuid,
        bSticky = false,
        bDontNetSync = true
      })
      Hud.Radar:AnimateObjectiveSize({
        sName = "db_" .. tostring(uGuid),
        nDuration = 5,
        nMinWidth = 4,
        nMinHeight = 4,
        nMaxWidth = 12,
        nMaxHeight = 12,
        nSpeedWidth = 20,
        nSpeedHeight = 20
      })
      tDBs[uGuid].Blip = true
    end
    if Net.IsServer() and not bForceOnClient then
      Net.SendEvent_AddDangerousBuilding(uGuid, bRadar, tDBs[uGuid].Permanent)
    end
  end
end

function OccupiedBuildingSpawnCallback(uGuid)
  if tDBs ~= nil and tDBs[uGuid] ~= nil and tDBs[uGuid].Blip then
    Hud.Radar:AnimateObjectiveAlpha({
      sName = "db_" .. tostring(uGuid),
      nDuration = 4,
      nMinAlpha = 0,
      nMaxAlpha = 1,
      nSpeed = 2
    })
  end
end

function TurnOnRandomDB(uGuid, bForceOnClient)
  if Net.IsClient() and not bForceOnClient then
    return
  end
  local tGuids = ConvertToTableOfGuids(uGuid)
  for i, uGuid in pairs(tGuids) do
    if not uGuid then
      return
    end
    tDBs[uGuid] = tDBs[uGuid] or {}
    tDBs[uGuid].Active = true
    tDBs[uGuid].Reward = 0
    Ai.TweakAttachedSpawners(uGuid, {
      SpawnerState = "on",
      SpawnerType = "Once",
      RadiusType = "RADIUS_PLAYER_2D",
      ActiveRadius = 100,
      SkipPercentChange = 100,
      SpawnList = "Spawnlist (VZ Tower)"
    })
    Ai.TweakAttachedSpawnersInGroup(uGuid, "ground", {SpawnerState = "off"})
    nDBCount = nDBCount + 1
    if Net.IsServer() and not bForceOnClient then
      Net.SendEvent_AddRandomDangerousBuilding(uGuid)
    end
  end
end

function OnDeactivate(uGuid)
  if tDBs[uGuid] and tDBs[uGuid].Permanent then
    return
  else
    RemoveDB(uGuid, false, false)
  end
  if tDBs[uGuid] then
    if tDBs[uGuid].WakeEvent then
      Event.Delete(tDBs[uGuid].WakeEvent)
      tDBs[uGuid].WakeEvent = nil
    end
    if tDBs[uGuid].HealthEvent then
      Event.Delete(tDBs[uGuid].HealthEvent)
      tDBs[uGuid].HealthEvent = nil
    end
  end
end

function Delete(oSelf)
  RemoveDB(oSelf.uGuid)
end

function OnDeath(uGuid)
  RemoveDB(uGuid, true, false)
end

function ClearProperties(uGuid)
  RemoveDB(uGuid, false, false)
end

function RemoveDB(uGuid, bKilled, bForceOnClient)
  if Net.IsClient() and not bForceOnClient then
    return
  end
  local tGuids = ConvertToTableOfGuids(uGuid)
  for i, uGuid in pairs(tGuids) do
    if tDBs[uGuid] then
      local bSend = false
      if tDBs[uGuid].Blip then
        bSend = true
        Hud.Radar:RemoveObjective({
          sName = "db_" .. tostring(uGuid),
          bDontNetSync = true
        })
        tDBs[uGuid].Blip = nil
      end
      if tDBs[uGuid].Active then
        bSend = true
        Ai.TweakAttachedSpawners(uGuid, {SpawnerState = "off"})
        nDBCount = nDBCount - 1
        if bKilled then
          local nReward = tDBs[uGuid].Reward or nDefaultCashReward
          if 0 < nReward then
            MrxGui.AddMessage({
              sText = "[green]Occupied building destroyed! +$" .. nReward,
              nDuration = 4
            })
            MrxPmc.AddCashQty(nReward, true)
          else
          end
          tDBs[uGuid].Tweaked = nil
        end
        if tDBs[uGuid].Tweaked then
          tDBs[uGuid].Active = nil
          tDBs[uGuid].Blip = nil
        else
          tDBs[uGuid] = nil
        end
      end
      if bSend and Net.IsServer() and not bForceOnClient then
        Debug.Printf("- Server tracking removed state of this building")
        Net.SendEvent_RemoveDangerousBuilding(uGuid, bKilled)
      end
    end
  end
end

function RemoveAllDBs()
  RemoveDB(tDBs, false, true)
end

function GetAllDBs()
  if tDBs then
    for i, entry in pairs(tDBs) do
      Debug.Printf(tostring(Object.GetName(i)) .. " =\n" .. tostring(entry))
    end
  else
    Debug.Printf("No DBs are currently active")
  end
end

function GetRarity(uGuid)
  if uGuid and tDBs[uGuid] then
    return tDBs[uGuid].Rarity
  else
    return nGlobalRarity
  end
end

function SetProperties(uGuid, tProps)
  if not uGuid or not tProps then
    Debug.Printf("- Invalid values passed to SetProperties")
    return
  end
  local tSpawnerGuids = ConvertToTableOfGuids(uGuid)
  for i, uGuid in pairs(tSpawnerGuids) do
    ProcessProperties(uGuid, tProps)
  end
end

function _Process(tTable, data)
  if type(data) == "string" then
    data = Pg.GetGuidByName(data)
  end
  if data then
    table.insert(tTable, data)
  end
end

function ConvertToTableOfGuids(tData)
  local tTable = {}
  if type(tData) == "table" then
    for i, value in pairs(tData) do
      _Process(tTable, value)
    end
  else
    _Process(tTable, tData)
  end
  return tTable
end

function ProcessProperties(uGuid, tProps)
  if not Object.IsAlive(uGuid) then
    return
  end
  tDBs[uGuid] = tDBs[uGuid] or {}
  if tProps.Density then
    tProps.Density = math.ceil(tProps.Density)
    tProps.Density = math.min(100, tProps.Density)
    tProps.Density = math.max(0, tProps.Density)
    tProps.ChanceNotActive = 100 - tProps.Density
    tProps.SkipPercentChance = 100 - tProps.Density
  end
  if tProps.Faction then
    SetFaction(uGuid, tProps.Faction)
  end
  tDBs[uGuid].Reward = tProps.Reward
  if tProps.Rarity then
    SetRarity(uGuid, tProps.Rarity)
  end
  if tProps.WakeupFunction then
    SetWakeupFunction(uGuid, tProps.WakeupFunction)
  end
  if tProps.Group then
    Ai.TweakAttachedSpawnersInGroup(uGuid, tProps.Group, tProps)
  else
    Ai.TweakAttachedSpawners(uGuid, tProps)
  end
  if tProps.Properties then
    tDBs[uGuid].Tweaked = true
  end
  tDBs[uGuid].Properties = tProps
  tDBs[uGuid].Properties.Properties = nil
end

function SetFaction(uGuid, sFaction)
  local tGuids = ConvertToTableOfGuids(uGuid)
  for i, uGuid in pairs(tGuids) do
    sFaction = string.lower(sFaction)
    if not uGuid or not sFaction then
      Debug.Printf("DangerousBuilding.lua: SetFaction received bad arguments :(")
    else
      tDBs[uGuid] = tDBs[uGuid] or {}
      tDBs[uGuid].Faction = sFaction
      tDBs[uGuid].Tweaked = true
      SetDBFaction(uGuid, sFaction, tDBs[uGuid])
    end
  end
end

function SetWakeupFunction(uGuid, fFunction)
  local tGuids = ConvertToTableOfGuids(uGuid)
  for i, uGuid in pairs(tGuids) do
    if not uGuid or not fFunction then
      Debug.Printf("DangerousBuilding.lua: SetWakeupFunction received bad arguments :(")
    else
      tDBs[uGuid] = tDBs[uGuid] or {}
      tDBs[uGuid].WakeupFunc = fFunction
      tDBs[uGuid].Tweaked = true
    end
  end
end

function SetRarity(uGuid, iRarity)
  if iRarity and string.lower(iRarity) == "never" then
    iRarity = -1
  elseif iRarity and string.lower(iRarity) == "always" then
    iRarity = 0
  end
  if iRarity == "default" then
    iRarity = nDefaultRarity
  end
  if uGuid == "default" or uGuid == "all" or uGuid == "global" then
    nGlobalRarity = iRarity or nDefaultRarity
    if nGlobalRarity < 0 then
    end
    return
  end
  local tGuids = ConvertToTableOfGuids(uGuid)
  for i, uGuid in pairs(tGuids) do
    tDBs[uGuid] = tDBs[uGuid] or {}
    tDBs[uGuid].Rarity = iRarity
    tDBs[uGuid].Tweaked = true
  end
end

function SetDBFaction(uGuid, sFaction, tProps)
  if type(uGuid) == "string" then
    uGuid = Pg.GetGuidByName(uGuid)
  end
  if not uGuid or not sFaction then
    Debug.Printf("DangerousBuilding.lua: SetDBFaction received bad arguments :(")
    return
  end
  local tGroups = {
    Ground = "Ground",
    Balcony = "Balcony",
    AA = "AA",
    Window = "Balcony",
    RoofTop = "Balcony"
  }
  for group, spawnlist in pairs(tGroups) do
    local sList = tostring("SpawnList (" .. sFaction .. " " .. spawnlist .. ")")
    if tProps and tProps.SpawnList and (not tProps.Group or tProps.Group == group) then
      sList = tProps.SpawnList
    end
    Ai.TweakAttachedSpawnersInGroup(uGuid, group, {SpawnList = sList})
  end
end
