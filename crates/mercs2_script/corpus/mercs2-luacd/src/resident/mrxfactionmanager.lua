import("MrxUtil")
import("MrxPmc")
import("MrxGui")
import("MrxSupport")
import("MrxAchievements")
import("MrxVoSequence")
import("MrxTutorialManager")
import("WifMissionFlow")
_knAttitudeMeterMin = 0
_knAttitudeMeterMax = 100
_knRelationMin = -100
_knRelationMax = 100
_tEvents = {}
_tInvestigatorBlips = {}
_tTressPassGeneric = {
  "Fiona.FactionZone.Generic01",
  "Fiona.FactionZone.Generic02"
}
_tAttitudes = {
  {
    sLabel = "Hostile",
    tRange = {
      "[",
      -100,
      -33,
      ")"
    },
    nPrices = nil,
    tRgbColor = {
      255,
      0,
      0
    }
  },
  {
    sLabel = "Neutral",
    tRange = {
      "[",
      -33,
      33,
      ")"
    },
    nPrices = 1.5,
    tRgbColor = {
      200,
      200,
      200
    }
  },
  {
    sLabel = "Friendly",
    tRange = {
      "[",
      33,
      100,
      "]"
    },
    nPrices = 1,
    tRgbColor = {
      0,
      127,
      255
    }
  }
}
_tFactions = {
  All = {
    bDynamic = true,
    sFactionTemplate = "Allied",
    sMarkerTexture = "HUD_faction_AN",
    sPdaIcon = "icon_an_mc",
    sInlineIcon = "[flagallies]",
    fGetInitialRelation = function()
      return GetRelation("Oil", "Pmc")
    end,
    sMaxRelationAchievement = "ACHIEVEMENT_STAND_UP_AND_SHOUT",
    tPursuitVO = {
      "Fiona.Cam.28",
      "Fiona.Cam.29",
      "Fiona.Cam.30",
      "Fiona.Cam.40",
      "Fiona.Cam.41",
      "Fiona.Cam.42"
    },
    tIntentVO = {
      "AlliedSoldier01.Reporting.CallIn01",
      "AlliedSoldier01.Reporting.CallIn02",
      "AlliedSoldier01.Reporting.CallIn03"
    },
    tReportVO = {
      "AlliedSoldier01.Reporting.LongReport01",
      "AlliedSoldier01.Reporting.LongReport02",
      "AlliedSoldier01.Reporting.LongReport03"
    },
    tReportedVO = {
      "Fiona.fio_g50",
      "Fiona.fio_g51",
      "Fiona.xfio122"
    },
    tJammedVO = {
      "AlliedSoldier01.Reporting.Jammed01",
      "AlliedSoldier01.Reporting.Jammed02"
    },
    tTresspassVO = {
      "Fiona.FactionZone.Allies01",
      "Fiona.FactionZone.Allies02"
    },
    nReportThreshold = 1,
    nReportFrequency = 15,
    sPdaFactionId = "AN",
    bCanReport = true
  },
  Chi = {
    bDynamic = true,
    sFactionTemplate = "China",
    sMarkerTexture = "HUD_faction_CH",
    sPdaIcon = "icon_ch_mc",
    sInlineIcon = "[flagchina]",
    fGetInitialRelation = function()
      return GetRelation("Gur", "Pmc")
    end,
    sMaxRelationAchievement = "ACHIEVEMENT_LONGING_FOR_FIRE",
    tPursuitVO = {
      "Fiona.Cam.28",
      "Fiona.Cam.29",
      "Fiona.Cam.30",
      "Fiona.Cam.43",
      "Fiona.Cam.44",
      "Fiona.Cam.45"
    },
    tIntentVO = {
      "ChinaSoldier01.Reporting.CallIn02",
      "ChinaSoldier01.Reporting.CallIn01"
    },
    tReportVO = {
      "ChinaSoldier01.Reporting.LongReport01",
      "ChinaSoldier01.Reporting.LongReport02",
      "ChinaSoldier01.Reporting.LongReport03"
    },
    tReportedVO = {
      "Fiona.fio_g38",
      "Fiona.xfio123",
      "Fiona.fio_g39"
    },
    tJammedVO = {
      "ChinaSoldier01.Reporting.Jammed01",
      "ChinaSoldier01.Reporting.Jammed02"
    },
    tTresspassVO = {
      "Fiona.FactionZone.China01",
      "Fiona.FactionZone.China02"
    },
    nReportThreshold = 1,
    nReportFrequency = 15,
    sPdaFactionId = "CH",
    bCanReport = true
  },
  Civ = {
    bDynamic = false,
    sFactionTemplate = "Civ",
    sMarkerTexture = "HUD_faction_CV",
    sInlineIcon = "[flagcivilian]"
  },
  Gur = {
    bDynamic = true,
    sFactionTemplate = "Guerilla",
    sMarkerTexture = "HUD_faction_GR",
    sPdaIcon = "icon_gr_mc",
    sInlineIcon = "[flagguerillas]",
    fGetInitialRelation = function()
      return GetAttitudeMedianValue("Friendly")
    end,
    sMaxRelationAchievement = "ACHIEVEMENT_FOREVER_FREE",
    tPursuitVO = {
      "Fiona.Cam.28",
      "Fiona.Cam.29",
      "Fiona.Cam.30",
      "Fiona.Cam.34",
      "Fiona.Cam.35",
      "Fiona.Cam.36"
    },
    tIntentVO = {
      "GurSoldier01.Reporting.CallIn01",
      "GurSoldier01.Reporting.CallIn02",
      "GurSoldier01.Reporting.CallIn03",
      "GuerillaSoldier_David01_Guerilla Soldier_AI Report_x_Merc(Generic)_x_x_x_03",
      "GuerillaSoldier_David01_Guerilla Soldier_AI Report_x_Merc(Generic)_x_x_x_04"
    },
    tReportVO = {
      "GurSoldier01.Reporting.LongReport01",
      "GurSoldier01.Reporting.LongReport02",
      "GurSoldier01.Reporting.LongReport03"
    },
    tReportedVO = {
      "Fiona.fio_g84",
      "Fiona.fio_g83",
      "Fiona.xfio124"
    },
    tJammedVO = {
      "GurSoldier01.Reporting.Jammed01",
      "Gur01.Reporting.Jammed02"
    },
    tTresspassVO = {
      "Fiona.FactionZone.Gur01",
      "Fiona.FactionZone.Gur02"
    },
    nReportThreshold = 1,
    nReportFrequency = 15,
    sPdaFactionId = "GR",
    bCanReport = true
  },
  Oil = {
    bDynamic = true,
    sFactionTemplate = "OC",
    sMarkerTexture = "HUD_faction_OC",
    sPdaIcon = "icon_oc_mc",
    sInlineIcon = "[flagoil]",
    fGetInitialRelation = function()
      return GetAttitudeMedianValue("Friendly")
    end,
    sMaxRelationAchievement = "ACHIEVEMENT_DIRTY_DEEDS",
    tPursuitVO = {
      "Fiona.Cam.28",
      "Fiona.Cam.29",
      "Fiona.Cam.30",
      "Fiona.Cam.37",
      "Fiona.Cam.38",
      "Fiona.Cam.39"
    },
    tIntentVO = {
      "OCSoldier01.Reporting.CallIn01",
      "OCSoldier01.Reporting.CallIn02",
      "OCSoldier01.Reporting.CallIn03"
    },
    tReportVO = {
      "OCSoldier01.Reporting.LongReport01",
      "OCSoldier01.Reporting.LongReport02",
      "Generic OC Soldier_Keith01_OC Soldier_AI Report_x_Merc(Generic)_x_x_x_04"
    },
    tReportedVO = {
      "Fiona.fio_g71",
      "Fiona.fio_g72",
      "Fiona.fio_g73",
      "Fiona.xfio125"
    },
    tJammedVO = {
      "OCSoldier01.Reporting.Jammed01",
      "OCSoldier01.Reporting.Jammed02"
    },
    tTresspassVO = {
      "Fiona.FactionZone.OC01",
      "Fiona.FactionZone.OC02",
      "Fiona.FactionZone.OC03"
    },
    nReportThreshold = 1,
    nReportFrequency = 15,
    sPdaFactionId = "OC",
    bCanReport = true
  },
  Pir = {
    bDynamic = true,
    sFactionTemplate = "Pirate",
    sMarkerTexture = "HUD_faction_PR",
    sPdaIcon = "icon_pr_mc",
    sInlineIcon = "[flagpirates]",
    fGetInitialRelation = function()
      return GetAttitudeMedianValue("Neutral")
    end,
    sMaxRelationAchievement = "ACHIEVEMENT_ISLAND_DOMINATION",
    tPursuitVO = {
      "Fiona.fio_g52",
      "Fiona.fio_g54"
    },
    tReportedVO = {
      "Fiona.fio_g60",
      "Fiona.fio_g61",
      "Fiona.fio_g62",
      "Fiona.xfio126"
    },
    tTresspassVO = {
      "Fiona.FactionZone.Pirates01",
      "Fiona.FactionZone.Pirates02"
    },
    nReportThreshold = 1,
    nReportFrequency = 15,
    sPdaFactionId = "PR",
    bCanReport = true
  },
  Pmc = {
    bDynamic = false,
    sFactionTemplate = "PMC",
    sMarkerTexture = "HUD_faction_CV",
    sInlineIcon = "[flagpmc]",
    sPdaFactionId = "PMC"
  },
  Vza = {
    bDynamic = false,
    sFactionTemplate = "VZ",
    sMarkerTexture = "HUD_faction_VZ",
    sInlineIcon = "[flagvz]",
    tPursuitVO = {
      "Fiona.Cam.28",
      "Fiona.Cam.29",
      "Fiona.Cam.30",
      "Fiona.Cam.31",
      "Fiona.Cam.32",
      "Fiona.Cam.33"
    },
    tIntentVO = {
      "VZSoldier01.Reporting.CallIn01",
      "VZSoldier01.Reporting.CallIn02",
      "VZSoldier01.Reporting.CallIn03"
    },
    tReportVO = {
      "VZSoldier01.Reporting.LongReport01",
      "VZSoldier01.Reporting.LongReport02",
      "VZSoldier01.Reporting.LongReport03"
    },
    tJammedVO = {
      "VZSoldier01.Reporting.Jammed01",
      "VZSoldier01.Reporting.Jammed02"
    },
    tTresspassVO = {
      "Fiona.FactionZone.VZ01",
      "Fiona.FactionZone.VZ02",
      "Fiona.FactionZone.VZ03"
    },
    nReportThreshold = 1,
    nReportFrequency = 20,
    sPdaFactionId = "VZ"
  }
}
_bSetupComplete = nil
_bVoPlayed = false

function Init()
  _tAttitudeLevelsToLabels = {}
  _tAttitudeLabelsToLevels = {}
  for i, tAttitudeData in ipairs(_tAttitudes) do
    _tAttitudeLevelsToLabels[i] = tAttitudeData.sLabel
    _tAttitudeLabelsToLevels[tAttitudeData.sLabel] = i
  end
  _tFactionTemplateToAbbrev = {}
  for sAbbrev, tFactionData in pairs(_tFactions) do
    tFactionData.uGuid = Pg.GetGuidByName(tFactionData.sFactionTemplate)
    _tFactionTemplateToAbbrev[tFactionData.sFactionTemplate] = sAbbrev
  end
end

function Reset()
  Report.Init({
    Callback = 0,
    SimultaneousReporters = 1,
    LossThreshold = 1,
    GoalPriority = 10
  })
end

function Setup()
  Report.Init({
    Callback = HandleReporter,
    SimultaneousReporters = 1,
    LossThreshold = 1,
    GoalPriority = 10
  })
  tDisabledReporters = tDisabledReporters or {}
  Pg.SetPursuitLevelTimes(120, 300)
  SetupNextFlyby()
  MrxAchievements.FactionMoodAchievements()
  Debug.Printf("Disguise.Init " .. tostring(HandleInvestigator))
  Disguise.Init({InvestigatorCallback = HandleInvestigator})
  Debug.Printf("FactionZone.Init " .. tostring(HandleTressPasser))
  FactionZone.Init({TresspasserCallback = HandleTressPasser})
  local tLevelThresholds = {}
  local tLevelNames = {}
  for i, tAttitudeData in ipairs(_tAttitudes) do
    local nRangeBegin = tAttitudeData.tRange[2]
    tLevelThresholds[i] = ConvertRelationToMeterValue(nRangeBegin)
    tLevelNames[i] = "[Generic.Attitudes." .. tAttitudeData.sLabel .. "]"
  end
  Hud.FactionDisplay:ConfigureThresholds({tLevelThresholds = tLevelThresholds, tLevelNames = tLevelNames})
  for sAbbrev, tFactionData in pairs(_tFactions) do
    SetRelation(sAbbrev, sAbbrev, _knRelationMax, true)
  end
  CivCasualtySetup()
  if not _tEvents.eJoin then
    _tEvents.eJoin = Event.CreatePersistent(Event.ScriptEvent, {
      "mpPlayerJoin",
      function(tData)
        return Net.IsServer() and not Player.IsLocal(tData[1])
      end
    }, SendPlayerJoinEvents)
  end
  _bSetupComplete = true
end

NETEVENT_SETMUTABLE = 0
NETEVENT_CIVKILLINIT = 1
NETEVENT_CIVKILL = 2

function SendPlayerJoinEvents()
  if not Net.IsServer() then
    return
  end
  local tArgs = {}
  local idx = 1
  for sFactionAbbrev, tData in pairs(_tFactions) do
    if CanAttitudeBeMutable(sFactionAbbrev) and IsAttitudeMutable(sFactionAbbrev) then
      tArgs[idx] = sFactionAbbrev
      idx = idx + 1
      if idx == 5 then
        Net.SendCustomEvent("MrxFactionManager", NETEVENT_SETMUTABLE, tArgs, true)
        idx = 1
        tArgs = {}
      end
    end
  end
  if 1 < idx then
    Net.SendCustomEvent("MrxFactionManager", NETEVENT_SETMUTABLE, tArgs, true)
  end
  Debug.Printf("Syncing CivKills on join: nKills=" .. tostring(nCivilianCasualties) .. ", nAmt=" .. tostring(nCivilianPenalty))
  Net.SendCustomEvent("MrxFactionManager", NETEVENT_CIVKILLINIT, {nCivilianCasualties, nCivilianPenalty}, true)
end

function GetFactionStringIndex(uStringHash)
  for sFactionAbbrev, tData in pairs(_tFactions) do
    if String.GetHash(sFactionAbbrev) == uStringHash then
      return sFactionAbbrev
    end
  end
  return "NO NAME"
end

function NetEventCallback(eventId, tArgs)
  _bWaitingOnMutable = true
  if not _bSetupComplete then
    Event.Create(Event.TimerRelative, {1}, NetEventCallback, {eventId, tArgs})
    return
  end
  if eventId == NETEVENT_SETMUTABLE then
    for idx, stringHash in pairs(tArgs) do
      SetAttitudeMutable(GetFactionStringIndex(stringHash))
    end
  elseif eventId == NETEVENT_CIVKILLINIT then
    Debug.Printf("NETEVENT_CIVKILLINIT: nCivKills=" .. tostring(tArgs[1]) .. ", nAmt=" .. tostring(tArgs[2]))
    nCivilianCasualties = tArgs[1]
    nCivilianPenalty = tArgs[2]
  elseif eventId == NETEVENT_CIVKILL then
    Debug.Printf("NETEVENT_CIVKILL: killerId=" .. tostring(tArgs[1]))
    local uKiller = Player.GetCharacter(Player.GetPlayer(tArgs[1]))
    ChargeCivCasualty(uKiller)
  end
  _bWaitingOnMutable = nil
end

function NetInitializeClientFactionRelations(nNext)
  if not Net.IsClient() then
    return
  end
  if _evTimerEvent then
    Event.Delete(_evTimerEvent)
  end
  local nNextIndex = 1
  if "number" == type(nNext) then
    nNextIndex = nNext
  end
  if not _bSetupComplete or _bWaitingOnMutable then
    _evTimerEvent = Event.Create(Event.TimerRelative, {0.5}, NetInitializeClientFactionRelations, {nNext})
    return
  end
  local nFactionIndex = 1
  for sFaction1, tData1 in pairs(_tFactions) do
    if nFactionIndex == nNextIndex then
      for sFaction2, tData2 in pairs(_tFactions) do
        SetRelation(sFaction1, sFaction2, GetRelation(sFaction1, sFaction2), true)
      end
      _evTimerEvent = Event.Create(Event.TimerRelative, {0.5}, NetInitializeClientFactionRelations, {
        nFactionIndex + 1
      })
      return
    end
    nFactionIndex = nFactionIndex + 1
  end
end

function SetAttitudeMutable(sAbbrev, bRestoreFromSave)
  local tFactionData = _tFactions[sAbbrev]
  if CanAttitudeBeMutable(sAbbrev) then
    tFactionData.bAttitudeMutable = true
    Hud.FactionDisplay:AddMeter({
      sFaction = sAbbrev,
      sTexture = tFactionData.sMarkerTexture
    })
    if not bRestoreFromSave then
      local nRelation = tFactionData.nInitialRelation
      if tFactionData.fGetInitialRelation then
        nRelation = tFactionData.fGetInitialRelation()
      end
      SetRelation(sAbbrev, "Pmc", nRelation, true)
      SetRelation(sAbbrev, sAbbrev, _knRelationMax, true)
    end
    if Net.IsServer() then
      Net.SendCustomEvent("MrxFactionManager", NETEVENT_SETMUTABLE, {sAbbrev})
    end
  end
end

function IsAttitudeMutable(sAbbrev)
  return _tFactions[sAbbrev].bAttitudeMutable == true
end

function CanAttitudeBeMutable(sAbbrev)
  return _tFactions[sAbbrev].bDynamic == true
end

function GetRelation(sSubjectAbbrev, sObjectAbbrev)
  return Ai.GetRelation(_tFactions[sSubjectAbbrev].uGuid, _tFactions[sObjectAbbrev].uGuid)
end

function TestAttitude(sSubjectAbbrev, sObjectAbbrev, sComparison, sAttitude)
  local nAttitudeLevel = GetAttitudeLevel(sSubjectAbbrev, sObjectAbbrev)
  local nTgtAttitudeLevel = _tAttitudeLabelsToLevels[sAttitude]
  ASSERT(nTgtAttitudeLevel)
  if sComparison == "==" then
    return nAttitudeLevel == nTgtAttitudeLevel
  elseif sComparison == "<" then
    return nAttitudeLevel < nTgtAttitudeLevel
  elseif sComparison == "<=" then
    return nAttitudeLevel <= nTgtAttitudeLevel
  elseif sComparison == ">" then
    return nAttitudeLevel > nTgtAttitudeLevel
  elseif sComparison == ">=" then
    return nAttitudeLevel >= nTgtAttitudeLevel
  end
end

function GetAttitudeLevel(sSubjectAbbrev, sObjectAbbrev)
  return ConvertRelationToAttitudeLevel(GetRelation(sSubjectAbbrev, sObjectAbbrev))
end

function GetAttitudeLabel(sSubjectAbbrev, sObjectAbbrev)
  return GetAttitudeFromLevel(GetAttitudeLevel(sSubjectAbbrev, sObjectAbbrev))
end

function GetMeterValue(sSubjectAbbrev, sObjectAbbrev)
  return ConvertRelationToMeterValue(GetRelation(sSubjectAbbrev, sObjectAbbrev))
end

function GetPriceScale(sSubjectAbbrev, sObjectAbbrev)
  local nLevel = GetAttitudeLevel(sSubjectAbbrev, sObjectAbbrev)
  return _tAttitudes[nLevel].nPrices
end

function SetRelation(sSubjectAbbrev, sObjectAbbrev, nRelation, bInitialize)
  if sObjectAbbrev == "Pmc" and not IsAttitudeMutable(sSubjectAbbrev) then
    Debug.Printf("CAN'T SET RELATION ... ATTITUDE IS NOT MUTABLE")
    return
  end
  local nOldRelation = GetRelation(sSubjectAbbrev, sObjectAbbrev)
  local nOldAttitudeLevel = ConvertRelationToAttitudeLevel(nOldRelation)
  Ai.SetRelation(_tFactions[sSubjectAbbrev].uGuid, _tFactions[sObjectAbbrev].uGuid, nRelation)
  local nNewRelation = GetRelation(sSubjectAbbrev, sObjectAbbrev)
  local nNewAttitudeLevel = ConvertRelationToAttitudeLevel(nNewRelation)
  if sObjectAbbrev == "Pmc" then
    Hud.FactionDisplay:SetValue({
      sFaction = sSubjectAbbrev,
      nValue = ConvertRelationToMeterValue(GetRelation(sSubjectAbbrev, sObjectAbbrev)),
      bInitialize = bInitialize
    })
    local sName = GetPlayerVisibleName(sSubjectAbbrev)
    if sName then
      local sIcon = _tFactions[sSubjectAbbrev].sPdaIcon
      Pda.Database:SetFactionAttitude({
        sName = sName,
        sTexture = sIcon,
        nAttitude = ConvertRelationToMeterValue(GetRelation(sSubjectAbbrev, sObjectAbbrev))
      })
    end
    if nNewRelation >= _knRelationMax then
      local sAchievement = _tFactions[sSubjectAbbrev].sMaxRelationAchievement
      if sAchievement then
        MrxAchievements.NetGrantAchievement(sAchievement)
      end
    end
  end
  if nOldAttitudeLevel ~= nNewAttitudeLevel or bInitialize then
    Event.Post("Attitude", {
      sSubjectAbbrev,
      sObjectAbbrev,
      GetAttitudeFromLevel(nOldAttitudeLevel),
      GetAttitudeFromLevel(nNewAttitudeLevel)
    })
  end
end

function ChangeRelation(sSubjectAbbrev, sObjectAbbrev, nRelation)
  local nOldRelation = GetRelation(sSubjectAbbrev, sObjectAbbrev)
  SetRelation(sSubjectAbbrev, sObjectAbbrev, nOldRelation + nRelation)
end

function CreateAttitudeChangeEvent(tParams, fCallback, tCallbackData)
  return _CreateAttitudeChangeEvent(false, tParams, fCallback, tCallbackData)
end

function CreatePersistentAttitudeChangeEvent(tParams, fCallback, tCallbackData)
  return _CreateAttitudeChangeEvent(true, tParams, fCallback, tCallbackData)
end

function _CreateAttitudeChangeEvent(bPersistent, tParams, fCallback, tCallbackData)
  if tParams and tParams[2] ~= "Pmc" then
    return
  end
  local fEventCreationFunction = Event.Create
  if bPersistent then
    fEventCreationFunction = Event.CreatePersistent
  end
  return fEventCreationFunction(Event.ScriptEvent, {
    "Attitude",
    function(tData)
      if tParams then
        for i = 1, 4 do
          if tParams[i] and tParams[i] ~= tData[i] then
            return false
          end
        end
      end
      return true
    end
  }, fCallback, tCallbackData)
end

function ConvertRelationToMeterValue(nRelation)
  local nPercentage = (nRelation - _knRelationMin) / (_knRelationMax - _knRelationMin)
  return (_knAttitudeMeterMax - _knAttitudeMeterMin) * nPercentage + _knAttitudeMeterMin
end

function ConvertRelationToAttitudeLevel(nRelation)
  for nLevel, tAttitude in ipairs(_tAttitudes) do
    local nRangeBegin = tAttitude.tRange[2]
    local bRangeBeginInclusive = tAttitude.tRange[1] == "["
    local nRangeEnd = tAttitude.tRange[3]
    local bRangeEndInclusive = tAttitude.tRange[4] == "]"
    local bWithinLowerBound = nRelation > nRangeBegin
    if bRangeBeginInclusive then
      bWithinLowerBound = nRelation >= nRangeBegin
    end
    local bWithinUpperBound = nRelation < nRangeEnd
    if bRangeEndInclusive then
      bWithinUpperBound = nRelation <= nRangeEnd
    end
    if bWithinLowerBound and bWithinUpperBound then
      return nLevel
    end
  end
end

function GetAttitudeFromLevel(nLevel)
  return _tAttitudeLevelsToLabels[nLevel]
end

function GetFactionAbbrevs()
  local tReturn = {}
  for sAbbrev, tFactionData in pairs(_tFactions) do
    table.insert(tReturn, sAbbrev)
  end
  table.sort(tReturn)
  return tReturn
end

function GetFactionAbbrev(sFactionTemplate)
  return _tFactionTemplateToAbbrev[sFactionTemplate]
end

function GetFactionAbbrevFromFactionGuid(sFactionGuid)
  for sAbbrev, tFactionData in pairs(_tFactions) do
    if tFactionData.uGuid == sFactionGuid then
      return sAbbrev
    end
  end
  return nil
end

function GetFactionTemplateName(sFactionAbbrev)
  return _tFactions[sFactionAbbrev].sFactionTemplate
end

function GetBribableFactions()
  local tBribableFactions = {}
  for sAbbrev, tFactionData in pairs(_tFactions) do
    if IsAttitudeMutable(sAbbrev) and TestAttitude(sAbbrev, "Pmc", "<", "Friendly") then
      table.insert(tBribableFactions, sAbbrev)
    end
  end
  return tBribableFactions
end

function GetAttitudes()
  local tReturn = {}
  for i, tAttitudeData in ipairs(_tAttitudes) do
    tReturn[tAttitudeData.sLabel] = GetAttitudeMedianValue(tAttitudeData.sLabel)
  end
  return tReturn
end

function GetAttitudeMedianValue(sLabel)
  local nLevel = _tAttitudeLabelsToLevels[sLabel]
  local tAttitude = _tAttitudes[nLevel]
  local nRangeBegin = tAttitude.tRange[2]
  local nRangeEnd = tAttitude.tRange[3]
  local nRangeMedian = (nRangeEnd - nRangeBegin) / 2 + nRangeBegin
  return nRangeMedian
end

function GetRgbColor(sSubjectAbbrev, sObjectAbbrev)
  local nLevel = GetAttitudeLevel(sSubjectAbbrev, sObjectAbbrev)
  return _tAttitudes[nLevel].tRgbColor
end

function SaveSingleton()
  local tRelations = {}
  local tMutableFactions = {}
  for sSubjectAbbrev, tSubjectFactionData in pairs(_tFactions) do
    tRelations[sSubjectAbbrev] = {}
    for sObjectAbbrev, tObjectFactionData in pairs(_tFactions) do
      if sSubjectAbbrev ~= sObjectAbbrev then
        tRelations[sSubjectAbbrev][sObjectAbbrev] = GetRelation(sSubjectAbbrev, sObjectAbbrev)
      end
    end
    if IsAttitudeMutable(sSubjectAbbrev) then
      table.insert(tMutableFactions, sSubjectAbbrev)
    end
  end
  return {
    tRelations = tRelations,
    tMutableFactions = tMutableFactions,
    nCivilianCasualties = nCivilianCasualties,
    nCivilianPenalty = nCivilianPenalty
  }
end

function LoadSingleton(tSaveData)
  if not tSaveData then
    return
  end
  local tMutableFactions = tSaveData.tMutableFactions
  if tMutableFactions then
    for i, sAbbrev in ipairs(tMutableFactions) do
      SetAttitudeMutable(sAbbrev, true)
    end
  end
  local tRelations = tSaveData.tRelations
  if not tRelations then
    return
  end
  for sSubjectAbbrev, tSubjectFactionData in pairs(_tFactions) do
    for sObjectAbbrev, tObjectFactionData in pairs(_tFactions) do
      if tRelations[sSubjectAbbrev] and sSubjectAbbrev ~= sObjectAbbrev then
        local nRelation = tRelations[sSubjectAbbrev][sObjectAbbrev]
        if nRelation then
          SetRelation(sSubjectAbbrev, sObjectAbbrev, nRelation, true)
        end
      end
    end
  end
  nCivilianCasualties = tSaveData.nCivilianCasualties
  nCivilianPenalty = tSaveData.nCivilianPenalty
end

function GetPlayerVisibleName(sAbbrev)
  return "[Generic.Factions." .. sAbbrev .. ".Long]"
end

function GetShortPlayerVisibleName(sAbbrev)
  return "[Generic.Factions." .. sAbbrev .. ".Short]"
end

function GetAdjective(sAbbrev)
  return "[Generic.Factions." .. sAbbrev .. ".Adjectival]"
end

function GetInlineIcon(sAbbrev)
  return _tFactions[sAbbrev].sInlineIcon
end

function GetMarkerTexture(sAbbrev)
  return _tFactions[sAbbrev].sMarkerTexture
end

function CivCasualtySetup()
  Debug.Printf("***********************  CivCasualtySetup function has been called")
  uEvent = Event.CreatePersistent(Event.ObjectDeath, {
    "civ && human"
  }, ResolveCivCasualty)
end

function ResolveCivCasualty(uTarget, uCause, uKiller)
  bKilledByPlayer = false
  if uKiller then
    bKilledByPlayer = uKiller == Player.GetPrimaryCharacter() or uKiller == Player.GetSecondaryCharacter()
  end
  if bKilledByPlayer then
    if Net.IsActive() then
      Debug.Printf("Local player killed civilian!")
      if uKiller == Player.GetPrimaryCharacter() then
        Net.SendCustomEvent("MrxFactionManager", NETEVENT_CIVKILL, {0})
      elseif uKiller == Player.GetSecondaryCharacter() then
        Net.SendCustomEvent("MrxFactionManager", NETEVENT_CIVKILL, {1})
      end
    end
    ChargeCivCasualty(uKiller)
  end
end

function ChargeCivCasualty(uKiller)
  nCivilianPenalty = nCivilianPenalty or -5000
  nCivilianCasualties = nCivilianCasualties or 0
  if nCivilianCasualties == 0 then
  end
  nCivilianCasualties = nCivilianCasualties + 1
  if nCivilianCasualties > 19 then
    nCivilianCasualties = 1
    nCivilianPenalty = nCivilianPenalty * 2
    nCivilianPenalty = math.max(nCivilianPenalty, -1000000)
  end
  Event.Post("CollateralDamage", {uKiller})
  MrxPmc.AddCashQty(nCivilianPenalty, true, "[Generic.Collateral]")
  MrxTutorialManager.StartTutorial("CollateralDamage", true)
  local tCues = {}
  tCues[1] = {
    "Fiona.CollateralDamage.Generic02"
  }
  if MrxUtil.GetCharacterIdentity(uKiller) == "mattias" then
    tCues[5] = {
      {
        "Fiona.CollateralDamage.Generic01"
      },
      0.2,
      {
        mattias = "Mattias.CollateralDamage.Generic01"
      }
    }
  else
    tCues[5] = {
      "Fiona.CollateralDamage.Generic01"
    }
  end
  tCues[10] = {
    "Fiona.CollateralDamage.Generic03"
  }
  tCues[15] = {
    "Fiona.CollateralDamage.Generic04"
  }
  if tCues[nCivilianCasualties] then
    MrxVoSequence.Start(tCues[nCivilianCasualties], false, MrxVoSequence.knPriorityFreeplay)
  elseif math.randf() < 0.4 then
    tCues = {
      {
        chris = MrxUtil.GetRandomTableElement({
          "Chris.Reporting.Sorry.01",
          "Chris.Reporting.Sorry.02",
          "Chris_Phil01_Chris_Destroy_Civilians_Self_Vehicle_x_x_02",
          "Chris_Phil01_Chris_Destroy_Civilians_Self_Vehicle_x_x_03",
          "Chris_Phil01_Chris_Destroy_Civilians_Self_Vehicle_x_x_06",
          "Chris_Phil01_Chris_Destroy_Civilians_Self_x_x_x_01",
          "Chris_Phil01_Chris_Destroy_Civilians_Self_x_x_x_03",
          "Chris_Phil01_Chris_Destroy_Civilians_Self_x_x_x_04",
          "Chris_Phil01_Chris_Destroy_Civilians_Self_x_x_x_05",
          "Chris_Phil01_Chris_Destroy_Civilians_Self_x_x_x_06"
        }),
        jennifer = MrxUtil.GetRandomTableElement({
          "Jen.Reporting.Sorry.01",
          "Jen.Reporting.Sorry.02",
          "Jen_Jen01_Jen_Destroy_Civilians_Jen_Vehicle_x_x_02",
          "Jen_Jen01_Jen_Destroy_Civilians_Jen_Vehicle_x_x_03",
          "Jen_Jen01_Jen_Destroy_Civilians_Jen_Vehicle_x_x_05",
          "Jen_Jen01_Jen_Destroy_Civilians_Jen_Vehicle_x_x_06",
          "Jen_Jen01_Jen_Destroy_Civilians_Jen_x_x_x_01",
          "Jen_Jen01_Jen_Destroy_Civilians_Jen_x_x_x_02",
          "Jen_Jen01_Jen_Destroy_Civilians_Jen_x_x_x_03",
          "Jen_Jen01_Jen_Destroy_Civilians_Jen_x_x_x_04",
          "Jen_Jen01_Jen_Destroy_Civilians_Jen_x_x_x_05",
          "Jen_Jen01_Jen_Destroy_Civilians_Jen_x_x_x_06",
          "Jen_Jen01_Jen_Destroy_Civilians_Jen_x_x_x_07",
          "Jen_Jen01_Jen_Destroy_Civilians_Jen_x_x_x_08"
        }),
        mattias = MrxUtil.GetRandomTableElement({
          "Mattias.Reporting.Sorry.01",
          "Mattias.Reporting.Sorry.02",
          "Mattias_Peter01_Mattias_Destroy_Civilians_Mattias_Vehicle_x_x_02",
          "Mattias_Peter01_Mattias_Destroy_Civilians_Mattias_Vehicle_x_x_03",
          "Mattias_Peter01_Mattias_Destroy_Civilians_Mattias_x_x_x_01",
          "Mattias_Peter01_Mattias_Destroy_Civilians_Mattias_x_x_x_02",
          "Mattias_Peter01_Mattias_Destroy_Civilians_Mattias_x_x_x_03",
          "Mattias_Peter01_Mattias_Destroy_Civilians_Mattias_x_x_x_04",
          "Mattias_Peter01_Mattias_Destroy_Civilians_Mattias_x_x_x_05",
          "Mattias_Peter01_Mattias_Destroy_Civilians_Mattias_x_x_x_06",
          "Mattias_Peter01_Mattias_Destroy_Civilians_Mattias_x_x_x_07"
        })
      }
    }
    MrxVoSequence.Start(tCues, false, MrxVoSequence.knPriorityFreeplay)
  end
end

function GetFactionStringAbbrev(uGuid, uAbbrev)
  if uAbbrev then
    for sFactionAbbrev, tData in pairs(_tFactions) do
      if String.GetHash(sFactionAbbrev) == uAbbrev then
        return sFactionAbbrev
      end
    end
  else
    return GetFactionAbbrev(GetFaction(uGuid))
  end
  Debug.Printf("couldn't find faction abbrev")
  return "NO NAME"
end

function NetSafeRemoveReportingDisplay(uGuid, uAbbrev, bCancelTimer)
  local sAbbrev = GetFactionStringAbbrev(uGuid, uAbbrev)
  RemoveReportingDisplay(uGuid, sAbbrev, bCancelTimer)
end

function NetSafeHandleReporter0(uGuid, uAbbrev)
  local sAbbrev = GetFactionStringAbbrev(uGuid, uAbbrev)
  HandleReporter0(uGuid, sAbbrev)
end

function NetSafeHandleReporter1(uGuid, uAbbrev, bStartTimer)
  local sAbbrev = GetFactionStringAbbrev(uGuid, uAbbrev)
  HandleReporter1(uGuid, sAbbrev, bStartTimer)
end

function NetSafeHandleReporter2(uGuid, uAbbrev)
  local sAbbrev = GetFactionStringAbbrev(uGuid, uAbbrev)
  HandleReporter2(uGuid, sAbbrev)
end

function NetSafeFinishedReporting(uGuid, uAbbrev)
  local sAbbrev = GetFactionStringAbbrev(uGuid, uAbbrev)
  FinishedReporting(uGuid, sAbbrev)
end

function RemoveReportingDisplay(uGuid, sAbbrev, bCancelTimer)
  if uMarker then
    Marker.Remove(uMarker)
    uMarker = nil
  end
  if uMarkerBeam then
    Marker.Remove(uMarkerBeam)
    uMarkerBeam = nil
  end
  Marker.HaltPulse(uGuid)
  _G.Minimap:DeleteObjective("Reporter")
  if bCancelTimer == true and bActiveReporter then
    bActiveReporter = false
    VO.Cancel(uGuid, false)
    Hud.FactionDisplay:HideMeter({sFaction = sAbbrev})
  end
end

function HandleReporter0(uGuid, sAbbrev, bDontSendMessage)
  local tFactionData = _tFactions[sAbbrev]
  Debug.Printf("REPORTING - state 0" .. tostring(uGuid))
  if not uMarker then
    uMarker = Marker.AddBlip(uGuid, tFactionData.sMarkerTexture, 32, 255, 255, 255, 255, 2, nil, nil, 32, nil, true)
  end
  local nPulseR = 255
  local nPulseG = 0
  local nPulseB = 0
  Marker.Pulse(uGuid, nPulseR, nPulseG, nPulseB)
  local tFactionAbbrevToTexture = {
    Pmc = "MiniMap_Icon_Faction_PMC",
    Gur = "MiniMap_Icon_Faction_GR",
    Oil = "MiniMap_Icon_Faction_OC",
    Pir = "MiniMap_Icon_Faction_PR",
    All = "MiniMap_Icon_Faction_AN",
    Chi = "MiniMap_Icon_Faction_CH",
    Vza = "MiniMap_Icon_Faction_VZ"
  }
  local sTexture = tFactionAbbrevToTexture[sAbbrev]
  Hud.Radar:AddObjective({
    sName = "Reporter",
    uGuid = uGuid,
    nX = 0,
    nY = 250,
    nZ = 0,
    nR = 255,
    nG = 255,
    nB = 255,
    nWidth = 6,
    nHeight = 6,
    sTexture = sTexture,
    bSticky = true,
    nSortOrder = 2
  })
  Hud.Radar:AnimateObjectiveSonar({
    sName = "Reporter",
    nDuration = 0,
    sTexture = "temp_radar_pulse",
    nTotalBlips = 999999,
    nVisibleBlips = 8,
    nMinWidth = 4,
    nMaxWidth = 32,
    nBlipDelay = 0.5,
    nAlphaAtMin = 1,
    nAlphaAtMax = 1,
    nGrowSpeed = 32,
    nRed = nPulseR,
    nGreen = nPulseG,
    nBlue = nPulseB
  })
  if tFactionData.tIntentVO then
    local tSequence = {
      {
        MrxUtil.GetRandomTableElement(tFactionData.tIntentVO),
        uGuid
      }
    }
    if tSequence then
      MrxVoSequence.Start(tSequence, nil, MrxVoSequence.knPriorityFreeplay)
    end
  end
  if Net.IsServer() and not bDontSendMessage then
    Net.SetPursuitReportingState(uGuid, 0, sAbbrev)
  end
end

function HandleInvestigator(uPlayerGuid, uInvestigatorGuid, state)
end

function HandleTressPasser(uTressPasserGuid, bTresspassing, uFactionGuid)
  Debug.Printf("HandleTressPasser" .. " " .. tostring(uFactionGuid))
  local sAbbrev = GetFactionAbbrevFromFactionGuid(uFactionGuid)
  local tFactionData = _tFactions[sAbbrev]
  local tFactionTressPass = tFactionData.tTresspassVO or _tTressPassGeneric
  Debug.Printf("...........................................Entering Trespasser Zone! ", _bVoPlayed)
  if _bVoPlayed == false then
    _bVoPlayed = true
    Debug.Printf("...........................................Playing VO! ", _bVoPlayed)
    if bTresspassing then
      Debug.Printf("Tresspassing - true" .. " " .. tostring(uTressPasserGuid) .. " Faction " .. tostring(sAbbrev))
      local sCue = MrxUtil.GetRandomTableElement(tFactionTressPass)
      local tSequence = {
        {sCue, uTressPasserGuid}
      }
      MrxVoSequence.Start(tSequence, nil, MrxVoSequence.knPriorityContract)
      Event.Create(Event.TimerRelative, {60}, function()
        _bVoPlayed = false
        Debug.Printf("...........................................Timer Event Expired! ", _bVoPlayed)
      end)
    else
      Debug.Printf("Tresspassing - false" .. " " .. tostring(uTressPasserGuid) .. " Faction " .. tostring(sAbbrev))
    end
  else
    Debug.Printf("...........................................Skipping VO _bVoPlayed is true! ", _bVoPlayed)
  end
end

function HandleReporter1(uGuid, sAbbrev, bStartTimer)
  Debug.Printf("REPORTING - state 1" .. tostring(uGuid))
  local tFactionData = _tFactions[sAbbrev]
  if AmIBeingJammed(uGuid) then
    if tFactionData.tJammedVO then
      local sCue = MrxUtil.GetRandomTableElement(tFactionData.tJammedVO)
      local tSequence = {
        {sCue, uGuid},
        {
          HandleReporter2,
          {uGuid, sAbbrev}
        }
      }
      MrxVoSequence.Start(tSequence, nil, MrxVoSequence.knPriorityFreeplay)
    elseif sAbbrev == "Pir" then
      MrxVoSequence.Start("Fiona.PirateCoverage.Reporting02", nil, MrxVoSequence.knPriorityFreeplay)
      HandleReporter2(uGuid, sAbbrev)
    else
      HandleReporter2(uGuid, sAbbrev)
    end
  else
    if Net.IsClient() then
      if bStartTimer == true then
        Hud.FactionDisplay:StartTimer({nDuration = 10, sFaction = sAbbrev})
      end
    else
      Hud.FactionDisplay:StartTimer({
        nDuration = 10,
        sFaction = sAbbrev,
        fCallback = FinishedReporting,
        tCallbackData = {uGuid, sAbbrev}
      })
    end
    bActiveReporter = uGuid
    if tFactionData.tReportVO then
      local sCue = MrxUtil.GetRandomTableElement(tFactionData.tReportVO)
      local tSequence = {
        {sCue, uGuid}
      }
      MrxVoSequence.Start(tSequence, nil, MrxVoSequence.knPriorityFreeplay)
    elseif sAbbrev == "Pir" then
      MrxVoSequence.Start({
        "Fiona.PirateCoverage.Reporting03"
      }, nil, MrxVoSequence.knPriorityFreeplay)
    else
      MrxVoSequence.Start({
        "Fiona.Misc.Reporting01"
      }, nil, MrxVoSequence.knPriorityFreeplay)
    end
    if Net.IsServer() then
      Net.SetPursuitReportingState(uGuid, 1, sAbbrev)
    end
  end
end

function HandleReporter2(uGuid, sAbbrev)
  local tFactionData = _tFactions[sAbbrev]
  Debug.Printf("REPORTING - state 2" .. tostring(uGuid))
  RemoveReportingDisplay(uGuid, sAbbrev)
  if bActiveReporter then
    bActiveReporter = false
    VO.Cancel(uGuid, false)
    Hud.FactionDisplay:HideMeter({sFaction = sAbbrev})
  end
  if tFactionData.nReportFrequency then
    SetReportDelay(tFactionData.nReportFrequency)
  else
    SetReportDelay(15)
  end
  if Net.IsServer() then
    Net.SetPursuitReportingState(uGuid, 2, sAbbrev)
  end
end

function AmIBeingJammed(uGuid)
  if not MrxSupport.TestAALevel("jammer") then
    return false
  end
  local sFaction = GetFaction(uGuid)
  if sFaction then
    for entry, value in pairs(MrxSupport.tAA) do
      if type(entry) == "userdata" then
        local uDriver = Vehicle.GetDriver(entry)
        if uDriver then
          local sAAFaction = GetFaction(uDriver)
          if sAAFaction and sAAFaction ~= sFaction then
            return true
          end
        end
      end
    end
  end
end

function HandleReporter(uGuid, state)
  Debug.Printf("HandleReporter, state " .. tostring(state))
  local sAbbrev = GetFactionStringAbbrev(uGuid)
  local tFactionData = _tFactions[sAbbrev]
  if state == 0 then
    if ValidateReporter(uGuid) then
      HandleReporter0(uGuid, sAbbrev)
    else
      Report.Failed(uGuid)
    end
  elseif state == 1 then
    if not ValidateReporter(uGuid) then
      Debug.Printf("That reporter did not pass the validation check")
      return
    elseif bActiveReporter and bActiveReporter ~= uGuid ~= uGuid then
      Debug.Printdf("bActiveReporter ~= uGuid (we're getting a status change for a non-reporter)")
      HandleReporter0(uGuid, sAbbrev, true)
    else
      HandleReporter1(uGuid, sAbbrev)
    end
  elseif state == 2 then
    HandleReporter2(uGuid, sAbbrev)
  end
end

function ValidateReporter(uGuid)
  local sAbbrev = GetFactionStringAbbrev(uGuid)
  local tFactionData = _tFactions[sAbbrev]
  if CanAttitudeBeMutable(sAbbrev) and not IsAttitudeMutable(sAbbrev) then
    Debug.Printf("Report cancelled: This faction does not report!")
    return false
  elseif tDisabledReporters[uGuid] then
    Debug.Printf("Report cancelled: This reporter is disabled!")
    local sFaction = GetFaction(uGuid) or "Civ"
    Report.SetDelay(Pg.GetGuidByName(sFaction), 3)
    return false
  elseif not (sFaction ~= "Civ" and sFaction ~= "VZ" and not bReportingDisabled and _tFactions[sAbbrev]) or not _tFactions[sAbbrev].bCanReport then
    Debug.Printf("Report cancelled: VZ, civilian, or reporting is disabled globally!")
    return false
  elseif Object.HasLabel(uGuid, "Female") then
    Debug.Printf("Report cancelled: Female reporter (no female VO)")
    return false
  else
    return true
  end
end

function FinishedReporting(uGuid, sFactionAbbrev)
  Debug.Printf("REPORTING - FinishedReporting" .. tostring(uGuid) .. " " .. tostring(sFactionAbbrev))
  bActiveReporter = false
  RemoveReportingDisplay(uGuid, sFactionAbbrev)
  if _tFactions[sFactionAbbrev] and CanAttitudeBeMutable(sFactionAbbrev) then
    local tInfractions = Report.GetInfractions(uGuid)
    if tInfractions then
      for item, table in pairs(tInfractions) do
        Debug.Printf(tostring(item) .. ":\n" .. tostring(table) .. "--------------------")
      end
      local nMoodAdjustment = 0
      nMoodAdjustment = nMoodAdjustment + tInfractions.DamageObject[2] * 1
      nMoodAdjustment = nMoodAdjustment + tInfractions.DestroyObject[2] * 25
      nMoodAdjustment = nMoodAdjustment + tInfractions.Trespassing[2] * 20
      nMoodAdjustment = nMoodAdjustment + tInfractions.Hijack[2] * 10
      nMoodAdjustment = nMoodAdjustment + tInfractions.SpecialEvent[1] * tInfractions.SpecialEvent[2]
      nMoodAdjustment = nMoodAdjustment + tInfractions.DestroyPerson[2] * 50
      nMoodAdjustment = nMoodAdjustment + tInfractions.DamagePerson[2] * 3
      nMoodAdjustment = math.max(nMoodAdjustment, -60)
      ChangeRelation(sFactionAbbrev, "Pmc", -nMoodAdjustment)
      Debug.Printf("MrxFactionManager: You've been reported (" .. tostring(GetFaction(uGuid)) .. " -" .. nMoodAdjustment .. ")")
    end
  end
  Report.Completed(uGuid)
  if _tFactions[sFactionAbbrev] and _tFactions[sFactionAbbrev].nReportFrequency then
    SetReportDelay(_tFactions[sFactionAbbrev].nReportFrequency * 2)
  else
    SetReportDelay(30)
  end
  if GetRelation(sFactionAbbrev, "Pmc") <= _knRelationMin then
    IncrementPursuit(sFactionAbbrev)
  elseif _tFactions[sFactionAbbrev].tReportedVO then
    local sCue = MrxUtil.GetRandomTableElement(_tFactions[sFactionAbbrev].tReportedVO)
    MrxVoSequence.Start(sCue, nil, MrxVoSequence.knPriorityFreeplay)
  end
  Event.Post("HeroReported", {
    _tFactions[sFactionAbbrev].sFactionTemplate,
    uGuid
  })
  if Net.IsServer() then
    Net.SetPursuitReportingState(uGuid, 3, sFactionAbbrev)
  end
end

function expand(a)
  for i, entry in pairs(a) do
    Debug.Printf(tostring(i) .. " = " .. tostring(entry[1]) .. " , " .. tostring(entry[2]))
  end
end

function SetReportDelay(nDelay)
  tFactionGuids = tFactionGuids or {
    Pg.GetGuidByName("Allied"),
    Pg.GetGuidByName("China"),
    Pg.GetGuidByName("OC"),
    Pg.GetGuidByName("Guerilla"),
    Pg.GetGuidByName("Pirate"),
    Pg.GetGuidByName("VZ"),
    Pg.GetGuidByName("Civ")
  }
  for i, faction in pairs(tFactionGuids) do
    Report.SetDelay(faction, nDelay)
  end
end

function expand(a)
  for i, entry in pairs(a) do
    Debug.Printf(tostring(i) .. " = " .. tostring(entry[1]) .. " , " .. tostring(entry[2]))
  end
end

function GetFaction(uGuid)
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
  for i, faction in pairs(tFactions) do
    if Object.HasLabel(uGuid, faction) then
      return faction
    end
  end
  Debug.Printf("MrxFactionManager: Guid does not have a faction label\"" .. tostring(uGuid) .. "\"")
  return "Civ"
end

function GetPerceivedFaction(uGuid)
  if Object.IsDisguised(uGuid) then
    local uVeh = Vehicle.GetFromRider(uGuid)
    return GetFaction(uVeh)
  end
  return GetFaction(uGuid)
end

function DisableReporter(uGuid)
  if type(uGuid) == "string" then
    uGuid = Pg.GetGuidByName(uGuid)
  end
  if not uGuid then
    Debug.Printf("MrxFactionManager: Cannot find \"" .. tostring(uGuid) .. "\"")
    return
  end
  if uGuid == bActiveReporter then
    HandleReporter(bActiveReporter, 2)
  end
  tDisabledReporters[uGuid] = true
end

function EnableReporter(uGuid)
  if type(uGuid) == "string" then
    uGuid = Pg.GetGuidByName(uGuid)
  end
  if not uGuid then
    Debug.Printf("MrxFactionManager: Cannot find \"" .. tostring(uGuid) .. "\"")
    return
  end
  tDisabledReporters = tDisabledReporters or {}
  tDisabledReporters[uGuid] = nil
end

function DisableReporting(bDisable)
  bReportingDisabled = bDisable
  if bReportingDisabled then
    Debug.Printf("Reporting DISABLED")
    if bActiveReporter then
      HandleReporter(bActiveReporter, 2)
    end
  else
    Debug.Printf("Reporting ENABLED")
    for faction, data in pairs(_tFactions) do
      if data.sFactionTemplate then
        Report.SetDelay(Pg.GetGuidByName(data.sFactionTemplate), 1)
      end
    end
  end
end

function SetFactionReporting(sFaction, bDisable)
  if _tFactions[sFaction] then
    Debug.Printf(tostring(sFaction) .. " reporting set to " .. tostring(bDisable))
    _tFactions[sFaction].bCanReport = bDisable
  else
    Debug.Printf("Factionn ot found!")
  end
end

function IncrementPursuit(sFactionAbbrev)
  local uFaction = Pg.GetGuidByName(_tFactions[sFactionAbbrev].sFactionTemplate)
  local tCurrentPursuit = Pg.GetPursuitState()
  local nLevel = tCurrentPursuit.Level
  nLevel = nLevel + 1
  nLevel = Math.min(nLevel, 3)
  Pg.SetPursuit(uFaction, nLevel, true)
  Pg.SetPursuitSeconds(uFaction, 5, true)
  Hud.FactionDisplay:StartPursuit({nDuration = 5, sFaction = sFactionAbbrev})
  if _tFactions[sFactionAbbrev].tPursuitVO then
    local sCue = MrxUtil.GetRandomTableElement(_tFactions[sFactionAbbrev].tPursuitVO)
    MrxVoSequence.Start(sCue, nil, MrxVoSequence.knPriorityFreeplay)
  end
end

function LockPursuit(uGuid, nLevel)
  Pg.LockPursuit(uGuid, nLevel)
  local sFaction = GetFaction(uGuid)
  local sFactionAbbrev = GetFactionAbbrev(sFaction)
  Hud.FactionDisplay:StartPursuit({sFaction = sFactionAbbrev, nDuration = -1})
end

function ClearPursuitLock()
  for sName in pairs(_tFactions) do
    Hud.FactionDisplay:HideMeter({sFaction = sName})
  end
  Pg.ClearPursuitLock(true)
end

function SetCustomPursuit(uFaction, nDuration, tSettings)
  Pg.SetCustomPursuit(uFaction, nDuration, tSettings)
  local sFaction = GetFaction(uFaction)
  local sFactionAbbrev = GetFactionAbbrev(sFaction)
  Hud.FactionDisplay:StartPursuit({sFaction = sFactionAbbrev, nDuration = -1})
end

function ClearCustomPursuit()
  for sName in pairs(_tFactions) do
    Hud.FactionDisplay:HideMeter({sFaction = sName})
  end
  Pg.ClearCustomPursuit()
end

tFlybys = {
  {
    {
      altitude = 70,
      speed = 120,
      template = "Support Vehicle (Tucano)",
      bMulti = true
    },
    {
      altitude = 100,
      speed = 130,
      template = "Support Vehicle (Tucano)",
      bMulti = true
    },
    {
      altitude = 130,
      speed = 140,
      template = "Support Vehicle (Tucano)",
      bMulti = true
    }
  },
  {
    {
      altitude = 60,
      speed = 100,
      template = "Support Vehicle (OV10)"
    },
    {
      altitude = 90,
      speed = 110,
      template = "Support Vehicle (OV10)"
    },
    {
      altitude = 120,
      speed = 120,
      template = "Support Vehicle (OV10)"
    }
  },
  {
    {
      altitude = 50,
      speed = 80,
      template = "Support Vehicle (Cessna)"
    },
    {
      altitude = 80,
      speed = 80,
      template = "Support Vehicle (Cessna)"
    },
    {
      altitude = 110,
      speed = 80,
      template = "Support Vehicle (Cessna)"
    },
    {
      altitude = 300,
      speed = 120,
      template = "Support Vehicle (727)"
    }
  },
  {
    {
      altitude = 90,
      speed = 120,
      template = "Support Vehicle (A10)",
      bMulti = true
    },
    {
      altitude = 60,
      speed = 120,
      template = "Support Vehicle (A10)",
      bMulti = true
    },
    {
      altitude = 100,
      speed = 200,
      template = "Support Vehicle (F35)",
      bMulti = true
    },
    {
      altitude = 300,
      speed = 160,
      template = "Support Vehicle (B2)"
    },
    {
      altitude = 180,
      speed = 220,
      template = "Support Vehicle (F117)"
    },
    {
      altitude = 250,
      speed = 120,
      template = "Support Vehicle (C130)"
    },
    {
      altitude = 180,
      speed = 120,
      template = "Support Vehicle (AC130)"
    },
    {
      altitude = 50,
      speed = 60,
      template = "Support Vehicle (Predator)"
    },
    {
      altitude = 60,
      speed = 60,
      template = "Support Vehicle (Predator)"
    },
    {
      altitude = 60,
      speed = 60,
      template = "Support Vehicle (Predator)"
    }
  },
  {
    {
      altitude = 100,
      speed = 200,
      template = "Support Vehicle (Q5)",
      bMulti = true
    },
    {
      altitude = 200,
      speed = 200,
      template = "Support Vehicle (Q5)",
      bMulti = true
    },
    {
      altitude = 160,
      speed = 240,
      template = "Support Vehicle (Q5)",
      bMulti = true
    }
  }
}

function RandomFlyby()
  if not bReportingDisabled then
    local RandomFactionIndex = math.randi(3)
    if WifMissionFlow.HasKey("Invasion") then
      RandomFactionIndex = RandomFactionIndex + 2
    end
    local tFaction = tFlybys[RandomFactionIndex]
    local tData = tFaction[Math.randi(table.getn(tFaction))]
    local count = 1
    if tData.bMulti then
      count = Math.randi(3)
    end
    while 0 < count do
      local uChar = Player.GetLocalCharacter()
      if uChar then
        local tx, ty, tz = Pg.FindPointFromCamera(300, tData.altitude, 10, Player.GetLocalPlayer(), math.randi(360))
        local sx, sy, sz = Pg.FindPointFromCamera(300, tData.altitude, 10, Player.GetLocalPlayer(), math.randi(360))
        fx, fy, fz = tx, ty, tz
        if sx and sz and fy then
          fy = fy + Math.randi(15) - Math.randi(15)
          Airstrike.Flyby(tData.template, sx, sz, fx, fz, fy + tData.altitude, tData.speed)
        else
          Debug.Printf("Flyby error! sx = " .. tostring(sx) .. " sz = " .. tostring(sz) .. " fx = " .. tostring(fx) .. " fz = " .. tostring(fz) .. " fy = " .. tostring(fy))
        end
      else
        Debug.Printf("Flyby error! player character is nil ")
      end
      count = count - 1
    end
  end
  SetupNextFlyby()
end

function SetupNextFlyby()
  Event.Create(Event.TimerRelative, {
    19 + Math.randi(71)
  }, RandomFlyby)
end

function GetPdaFactionIdFromFactionId(sFactionId)
  if sFactionId then
    local tFactionData = _tFactions[sFactionId]
    if tFactionData then
      return tFactionData.sPdaFactionId
    end
  end
  return nil
end

function GetFactionIdFromIndex(nIndex)
  local index = 1
  for _id, tFactionConfig in pairs(_tFactions) do
    if index == nIndex then
      return _id
    end
    index = index + 1
  end
  return nil
end

function GetIndexFromFactionId(sFactionId)
  local index = 1
  for _id, tFactionConfig in pairs(_tFactions) do
    if _id == sFactionId then
      return index
    end
    index = index + 1
  end
  return nil
end
