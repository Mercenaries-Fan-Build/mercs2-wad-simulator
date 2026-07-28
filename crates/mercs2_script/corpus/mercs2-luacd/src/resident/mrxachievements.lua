import("MrxFactionManager")
tSupportUsed = {}
tSupportUsedInCoop = {}
tSupportFilter = {
  MrxSupportTransit = true,
  MrxBoatDelivery = true,
  MrxCrateDelivery = true,
  MrxSoldierDelivery = true,
  MrxSupportCopterDelivery = true,
  MrxSupportPickup = true
}
tAchievementsList = {
  {
    sName = "ACHIEVEMENT_MILLIONAIRE",
    nRequiredCount = 1
  },
  {
    sName = "ACHIEVEMENT_PLAY_COOP",
    nRequiredCount = 1
  },
  {
    sName = "ACHIEVEMENT_PIPELINE",
    nRequiredCount = 1
  },
  {
    sName = "ACHIEVEMENT_BURNOUT",
    nRequiredCount = 1
  },
  {
    sName = "ACHIEVEMENT_SCHOOLS_OUT",
    nRequiredCount = 1
  },
  {
    sName = "ACHIEVEMENT_RAGE",
    nRequiredCount = 1
  },
  {
    sName = "ACHIEVEMENT_HELLOHURRAY",
    nRequiredCount = 1
  },
  {
    sName = "ACHIEVEMENT_WILD_ONE",
    nRequiredCount = 1
  },
  {
    sName = "ACHIEVEMENT_RIDE_DRAGON",
    nRequiredCount = 1
  },
  {
    sName = "ACHIEVEMENT_ARMAGGEDON",
    nRequiredCount = 20
  },
  {
    sName = "ACHIEVEMENT_SHOOTTHRILL",
    nRequiredCount = 25
  },
  {
    sName = "ACHIEVEMENT_DAMAGE_INC",
    nRequiredCount = 20
  },
  {
    sName = "ACHIEVEMENT_NO_COMPROMISE",
    nRequiredCount = 1
  },
  {
    sName = "ACHIEVEMENT_RUNNING_WITH_DEVIL",
    nRequiredCount = 1
  },
  {
    sName = "ACHIEVEMENT_BETTER_RUN",
    nRequiredCount = 1
  },
  {
    sName = "ACHIEVEMENT_ANALOG_KID",
    nRequiredCount = 1
  },
  {
    sName = "ACHIEVEMENT_OIL_AND_GAZ",
    nRequiredCount = 1
  },
  {
    sName = "ACHIEVEMENT_HERO_AND_MADMAN",
    nRequiredCount = 1
  },
  {
    sName = "ACHIEVEMENT_HAIL_AND_KILL",
    nRequiredCount = 50
  },
  {
    sName = "ACHIEVEMENT_HOLY_SMOKE",
    nRequiredCount = 50
  },
  {
    sName = "ACHIEVEMENT_LITTLE_SAVAGE",
    nRequiredCount = 50
  },
  {
    sName = "ACHIEVEMENT_BALLS_TO_THE_WALL",
    nRequiredCount = 1
  },
  {
    sName = "ACHIEVEMENT_GONE_SHOOTIN",
    nRequiredCount = 1
  },
  {
    sName = "ACHIEVEMENT_HEAVY_METAL_THUNDER",
    nRequiredCount = 10
  },
  {
    sName = "ACHIEVEMENT_QUICK_OR_DEAD",
    nRequiredCount = 50
  },
  {
    sName = "ACHIEVEMENT_STAND_UP_AND_SHOUT",
    nRequiredCount = 1
  },
  {
    sName = "ACHIEVEMENT_FOREVER_FREE",
    nRequiredCount = 1
  },
  {
    sName = "ACHIEVEMENT_ISLAND_DOMINATION",
    nRequiredCount = 1
  },
  {
    sName = "ACHIEVEMENT_LONGING_FOR_FIRE",
    nRequiredCount = 1
  },
  {
    sName = "ACHIEVEMENT_DIRTY_DEEDS",
    nRequiredCount = 1
  },
  {
    sName = "ACHIEVEMENT_NO_MORE_MR_NICE_GUY",
    nRequiredCount = 1
  },
  {
    sName = "ACHIEVEMENT_JUSTICE_FOR_ALL",
    nRequiredCount = 1
  },
  {
    sName = "ACHIEVEMENT_FIND_ALL_LANDMARKS",
    nRequiredCount = 1
  },
  {
    sName = "ACHIEVEMENT_DIGITAL_MAN",
    nRequiredCount = 5
  },
  {
    sName = "ACHIEVEMENT_BURN_THE_SKY",
    nRequiredCount = 5
  },
  {
    sName = "ACHIEVEMENT_HIGHWAY_TO_HELL",
    nRequiredCount = 3
  },
  {
    sName = "ACHIEVEMENT_TECHNO_VIKING",
    nRequiredCount = 1
  },
  {
    sName = "ACHIEVEMENT_WHEELS_OF_STEEL",
    nRequiredCount = 1
  },
  {
    sName = "ACHIEVEMENT_NOTHIN_BUT_GOODTIME",
    nRequiredCount = 200
  },
  {
    sName = "ACHIEVEMENT_EVERYBODY_WANTSSOME",
    nRequiredCount = 1
  },
  {
    sName = "ACHIEVEMENT_BILLIONAIRE",
    nRequiredCount = 1
  }
}
EVENT_GRANTACHIEVEMENT = 0
EVENT_ACHIEVEMENTGRANT = 1
EVENT_ACHIEVEMENTADDCOUNT = 2

function GetNameFromHash(nameHash)
  for i, tAchievement in ipairs(tAchievementsList) do
    if String.GetHash(tAchievement.sName) == nameHash then
      return tAchievement.sName
    end
  end
  Debug.Printf("Couldn't find achievement name")
  return nil
end

function NetEventCallback(nType, tArgs)
  if Net.IsClient() then
    if nType == EVENT_GRANTACHIEVEMENT then
      local sName = GetNameFromHash(tArgs[1])
      if sName then
        Net.GrantAchievement(sName)
      end
    elseif nType == EVENT_ACHIEVEMENTGRANT then
      local sName = GetNameFromHash(tArgs[1])
      if sName then
        AchievementGrant(sName)
      end
    end
  end
  if nType == EVENT_ACHIEVEMENTADDCOUNT then
    local sName = GetNameFromHash(tArgs[1])
    if sName then
      AchievementAddCount(sName, tArgs[2], nil, tArgs[3])
    end
  end
end

function NetGrantAchievement(sAchievementName, vPlayers)
  if not vPlayers then
    Net.GrantAchievement(sAchievementName)
    if Net.IsServer() then
      Net.SendCustomEvent("MrxAchievements", EVENT_GRANTACHIEVEMENT, {sAchievementName}, true)
    end
  else
    local tPlayers
    local bLocal = false
    local bRemote = false
    local tPlayers
    if "table" == type(vPlayers) then
      tPlayers = vPlayers
    elseif "userdata" == type(vPlayers) then
      tPlayers = {vPlayers}
    else
      return
    end
    for nIndex, uGuid in pairs(tPlayers) do
      if Player.IsLocal(uGuid) then
        bLocal = true
      else
        bRemote = true
      end
    end
    if bLocal then
      Net.GrantAchievement(sAchievementName)
    end
    if bRemote and Net.IsServer() then
      Net.SendCustomEvent("MrxAchievements", EVENT_GRANTACHIEVEMENT, {sAchievementName}, true)
    end
  end
end

function GetLocalRemote(vPlayers)
  local bLocal, bRemote, tPlayers
  if vPlayers then
    if "table" == type(vPlayers) then
      tPlayers = vPlayers
    elseif "userdata" == type(vPlayers) then
      tPlayers = {vPlayers}
    else
      return false, false
    end
    for nIndex, uGuid in pairs(tPlayers) do
      if Player.IsLocal(uGuid) then
        bLocal = true
      else
        bRemote = true
      end
    end
  else
    bLocal = true
    bRemote = true
  end
  return bLocal, bRemote
end

function AchievementGrant(sAchievementName, vPlayers)
  local bLocal, bRemote = GetLocalRemote(vPlayers)
  if bLocal then
    local bFound = false
    for i, tAchievement in ipairs(tAchievementsList) do
      if tAchievement.sName == sAchievementName then
        Pg.AchievementAddCount(sAchievementName, i - 1, 1, tAchievement.nRequiredCount)
        bFound = true
        break
      end
    end
    if not bFound then
      Debug.Printf("AchievementGrant(" .. sAchievementName .. ") Failed: No such an achievement.")
    end
  end
  if bRemote and Net.IsServer() then
    Net.SendCustomEvent("MrxAchievements", EVENT_ACHIEVEMENTGRANT, {sAchievementName}, true)
  end
end

function AchievementAddCount(sAchievementName, nDeltaCount, vPlayers, bCustomEvent)
  local bLocal, bRemote = GetLocalRemote(vPlayers)
  if bCustomEvent == nil then
    bCustomEvent = false
  end
  if bLocal then
    local bFound = false
    for i, tAchievement in ipairs(tAchievementsList) do
      if tAchievement.sName == sAchievementName then
        Pg.AchievementAddCount(sAchievementName, i - 1, nDeltaCount, tAchievement.nRequiredCount)
        bFound = true
        break
      end
    end
    if not bFound then
      Debug.Printf("AchievementAddCount(" .. sAchievementName .. ") Failed: No such an achievement.")
    end
  end
  if bRemote and bCustomEvent then
    Net.SendCustomEvent("MrxAchievements", EVENT_ACHIEVEMENTADDCOUNT, {
      sAchievementName,
      nDeltaCount,
      false
    }, true)
  end
end

function AchievementIsGranted(sAchievementName)
  local bFound = false
  local bGranted = false
  for i, tAchievement in ipairs(tAchievementsList) do
    if tAchievement.sName == sAchievementName then
      bFound = true
      bGranted = Pg.AchievementIsGranted(sAchievementName, i - 1, tAchievement.nRequiredCount)
      break
    end
  end
  if not bFound then
    Debug.Printf("AchievementIsGranted(" .. sAchievementName .. ") Failed: No such an achievement.")
  end
  return bGranted
end

function AchievementAddCount_MASTER_HIJACK(vPlayers)
  AchievementAddCount("ACHIEVEMENT_MASTER_HIJACK", 1, vPlayers)
  AchievementAddCount("ACHIEVEMENT_MASTER_5_HIJACK", 1, vPlayers)
  AchievementAddCount("ACHIEVEMENT_MASTER_10_HIJACK", 1, vPlayers)
end

function FactionMoodAchievements()
  Debug.Printf("**********FACTION MOOD ACHIEVEMENT")
  MrxFactionManager.CreateAttitudeChangeEvent({
    "All",
    "Pmc",
    nil,
    "Hostile"
  }, function()
    nAlliedMood = 1
    Debug.Printf("**********FACTION MOOD ACHIEVEMENT: ALLIED NEG")
    Debug.Printf(nAlliedMood)
    Debug.Printf(nChinaMood)
    Debug.Printf(nGuerillaMood)
    Debug.Printf(nOCMood)
    if nAlliedMood == 1 and nChinaMood == 1 and nGuerillaMood == 1 and nOCMood == 1 and nPirMood == 1 then
      NetGrantAchievement("ACHIEVEMENT_NO_MORE_MR_NICE_GUY")
    end
  end)
  MrxFactionManager.CreateAttitudeChangeEvent({
    "Gur",
    "Pmc",
    nil,
    "Hostile"
  }, function()
    nGuerillaMood = 1
    Debug.Printf("**********FACTION MOOD ACHIEVEMENT: GUR NEG")
    Debug.Printf(nGuerillaMood)
    if nAlliedMood == 1 and nChinaMood == 1 and nGuerillaMood == 1 and nOCMood == 1 and nPirMood == 1 then
      NetGrantAchievement("ACHIEVEMENT_NO_MORE_MR_NICE_GUY")
    end
  end)
  MrxFactionManager.CreateAttitudeChangeEvent({
    "Chi",
    "Pmc",
    nil,
    "Hostile"
  }, function()
    nChinaMood = 1
    Debug.Printf("**********FACTION MOOD ACHIEVEMENT: CHI NEG")
    Debug.Printf(nChinaMood)
    if nAlliedMood == 1 and nChinaMood == 1 and nGuerillaMood == 1 and nOCMood == 1 and nPirMood == 1 then
      NetGrantAchievement("ACHIEVEMENT_NO_MORE_MR_NICE_GUY")
    end
  end)
  MrxFactionManager.CreateAttitudeChangeEvent({
    "Oil",
    "Pmc",
    nil,
    "Hostile"
  }, function()
    nOCMood = 1
    Debug.Printf("**********FACTION MOOD ACHIEVEMENT: CHI NEG")
    Debug.Printf(nOCMood)
    if nAlliedMood == 1 and nChinaMood == 1 and nGuerillaMood == 1 and nOCMood == 1 and nPirMood == 1 then
      NetGrantAchievement("ACHIEVEMENT_NO_MORE_MR_NICE_GUY")
    end
  end)
  MrxFactionManager.CreateAttitudeChangeEvent({
    "Pir",
    "Pmc",
    nil,
    "Hostile"
  }, function()
    nPirMood = 1
    Debug.Printf("**********FACTION MOOD ACHIEVEMENT: PIR NEG")
    Debug.Printf(nPirMood)
    if nAlliedMood == 1 and nChinaMood == 1 and nGuerillaMood == 1 and nOCMood == 1 and nPirMood == 1 then
      NetGrantAchievement("ACHIEVEMENT_NO_MORE_MR_NICE_GUY")
    end
  end)
  Event.CreatePersistent(Event.ScriptEvent, {
    "SupportUsed",
    function()
      return true
    end
  }, ProcessSupportEvent)
end

function ProcessSupportEvent(tData)
  local sModuleSupport = tData.sModuleName
  local sSupportName = tData.sSupportName
  if tSupportFilter[sModuleSupport] then
    return
  end
  if Net.IsActive() then
    if tSupportUsedInCoop[sSupportName] then
      return
    else
      tSupportUsedInCoop[sSupportName] = true
      Debug.Printf("********************* MrxSupport: DAMAGE INC ACHIEVEMENT")
      AchievementAddCount("ACHIEVEMENT_DAMAGE_INC", 1, nil, true)
    end
  end
end

function SaveSingleton()
  local tSupport = {}
  tSupport.tSupportUsed = tSupportUsed
  tSupport.tSupportUsedInCoop = tSupportUsedInCoop
  return tSupport
end

function LoadSingleton(tSaveData)
  if not tSaveData then
    return
  end
  if not tSaveData.tSupport then
    tSupportUsed = {}
    tSupportUsedInCoop = {}
  else
    tSupportUsed = tSaveData.tSupport.tSupportUsed
    tSupportUsedInCoop = tSaveData.tSupport.tSupportUsedInCoop
  end
end
