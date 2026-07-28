import("MrxCheatBootstrap")
import("MrxFactionManager")
import("MrxLayerManager")
import("MrxTask")
import("MrxTaskState")
import("MrxPlayState")
import("MrxRewardData")
import("MrxStarterManager")
import("MrxState")
import("MrxUtil")
import("WifMissionData")
import("WifStarterData")
import("WifHqData")
import("WifPmcInterior")
import("MrxUnlockFanfare")
import("MrxStatsManager")
import("WifMissionFlow")
import("WifRecommendationData")
import("MrxHqManager")
_bEnable = true
NETEVENT_SETGRAPPLE = 0
NETEVENT_AUTOSAVE = 1
NETEVENT_SETVEHICLEDISGUISE = 2

function EnableFlow(bEnable)
  _bEnable = bEnable
end

function Reset(bResetMore)
  _tMyFlowData = {}
  _tActiveMissions = {}
  _tCulledBindings = {}
  _sTrackedMissionName = nil
  _tMissionsToRepeat = {}
  Debug.Printf("@@@@@@@@@@ MrxMissionFlow.Reset: resetting refresh callback")
  _fRefreshCallback = nil
  _tRefreshCallbackArgs = nil
  Pda.Map:SetMissionTrackCallback({fCallback = _TrackMission})
  if bResetMore then
    _bCheckpointSaveMode = nil
    _bCurrentlyRefreshing = nil
    _bCurrentlyRefreshing = nil
    _bEnable = true
    _bGrappleEnabled = nil
    _bVehicleDisguiseEnabled = nil
    _bResourceCountersEnabled = nil
    _bPersistentRetry = nil
    _bSkipToMissionReached = nil
    _nBlockingSequences = nil
    _oParent = nil
    _sLastCompletedContractName = nil
    _tDeferredKeyAwards = {}
    _tFlowData = {}
    _tRetryLocations = {}
  end
end

function _TrackMission(sTrackedMissionName)
  Debug.Printf("@@@@@@@@@@ _TrackMission: sTrackedMissionName=" .. tostring(sTrackedMissionName))
  Debug.Printf("@@@@@@@@@@ _TrackMission: _sTrackedMissionName=" .. tostring(_sTrackedMissionName))
  if sTrackedMissionName == _sTrackedMissionName then
    return
  end
  local sMissionToAlter
  if sTrackedMissionName then
    DisableAllJobTracking()
    sMissionToAlter = sTrackedMissionName
  else
    sMissionToAlter = _sTrackedMissionName
  end
  Debug.Printf("@@@@@@@@@@ _TrackMission: mission to track/untrack=" .. tostring(sMissionToAlter))
  if sMissionToAlter then
    local tMissionData = _tActiveMissions[sMissionToAlter]
    Debug.Printf("@@@@@@@@@@ _TrackMission: tMissionData ~= nil? " .. tostring(tMissionData ~= nil))
    if tMissionData then
      oMission = tMissionData.oMission
      Debug.Printf("@@@@@@@@@@ _TrackMission: oMission ~= nil? " .. tostring(oMission ~= nil))
      if oMission then
        oMission:EnableTracking(sTrackedMissionName ~= nil)
      end
    end
  end
  _sTrackedMissionName = sTrackedMissionName
  Debug.Printf("@@@@@@@@@@ _TrackMission: _sTrackedMissionName (updated)=" .. tostring(_sTrackedMissionName))
end

function CleanupAllActiveMissions()
  for sMissionName, tMissionData in pairs(_tActiveMissions) do
    if tMissionData.oMission then
      local oContainer = tMissionData.oMission:GetParent()
      if oContainer then
        oContainer:Cleanup()
      end
    end
  end
end

function SetFlowData(tFlowData)
  _tFlowData = tFlowData
  local nBindings = 0
  for sBindingName, tBinding in pairs(_tFlowData) do
    nBindings = nBindings + 1
  end
  Debug.Printf("Setting flow data (" .. nBindings .. " bindings)")
end

function SetMissionParent(oParent)
  _oParent = oParent
end

function SetPreContractSaveFunction(fPreContractSave)
  _fPreContractSave = fPreContractSave
end

function UnlockMission(sMissionName, tSaveData, bBlockingSequence)
  ASSERT(_oParent)
  local sInputMissionName = sMissionName
  sMissionName = GetCaseSensitiveMissionId(sMissionName)
  if not sMissionName then
    Debug.Printf("Mission \"" .. sInputMissionName .. "\" unlock attempt FAILED; no mission by this name.")
    return false
  end
  sInputMissionName = nil
  if _oParent:GetChild(sMissionName) then
    Debug.Printf("Mission \"" .. sMissionName .. "\" unlock attempt FAILED; parent already has a child by this name.")
    return false
  end
  if _tActiveMissions[sMissionName] then
    Debug.Printf("Mission \"" .. sMissionName .. "\" unlock attempt FAILED; a mission by this name is currently active.")
    return false
  end
  Debug.Printf("Unlocking mission " .. sMissionName)
  local tContainerConfig = {}
  local tOriginalMissionConfig = WifMissionData.tMissionData[sMissionName]
  local tMissionConfig
  if type(tOriginalMissionConfig) == "table" then
    tMissionConfig = MrxUtil.CopyTable(tOriginalMissionConfig)
  else
    tMissionConfig = {}
  end
  local sStarterName = tMissionConfig.sStarter
  local tStarterData
  if sStarterName then
    tStarterData = WifStarterData[sStarterName]
  end
  local sMissionTitle = WifMissionData.GetMissionTitle(sMissionName)
  local oContainer = MrxTask:Create()
  local oMission = MrxTask:Create()
  tContainerConfig.sName = sMissionName
  tContainerConfig.sModuleName = "MrxTask"
  tContainerConfig.oParent = _oParent
  
  local function _OnContainerComplete()
    if Sys.GetForceNewGame and Sys.GetForceNewGame() then
      Debug.Printf("_OnContainerComplete() disabled.")
      return
    end
    local bIsContract = WifMissionData.IsMissionAContract(sMissionName)
    if bIsContract then
      MrxPlayState.Set(MrxPlayState._knFree)
      WifMissionFlow.SetRetryLocations(nil)
    end
    if tMissionConfig.sStarter then
      local oStarter = MrxStarterManager.RequestStarter(tMissionConfig.sStarter)
      if oStarter then
        oStarter:RemoveBriefing(sMissionName)
      end
    end
    AwardKey(sMissionName)
    local nCompletions = GetKeyValue(sMissionName)
    if type(tMissionConfig.tMilestones) == "table" then
      if WifMissionData.IsMissionAJob(sMissionName) then
        for i, tMilestoneData in pairs(tMissionConfig.tMilestones) do
          if not HasKey(tMilestoneData.sKey) then
            AwardKey(tMilestoneData.sKey)
          end
        end
      else
        for i, tMilestoneData in pairs(tMissionConfig.tMilestones) do
          if type(tMilestoneData.nMilestone) == "number" and tMilestoneData.nMilestone == nCompletions then
            AwardKey(tMilestoneData.sKey)
          end
        end
      end
    end
    if bIsContract then
      EnableAutosave()
    end
    Refresh()
    _tActiveMissions[sMissionName] = nil
    if Net.IsServer() then
      Net.SendEvent_RemovePDAMission(WifMissionData.GetMissionIndexFromId(sMissionName))
    end
    Pda.Map:RemoveMission({sName = sMissionName})
    if tMissionConfig.bRepeatable then
      local tSaveData
      if _tMissionsToRepeat[sMissionName] then
        tSaveData = {
          nState = MrxTaskState._knActive
        }
        _tMissionsToRepeat[sMissionName] = nil
        MrxUtil.TeleportHeroesToLocations(tMissionConfig.tStartLocations, UnlockMission, {
          sMissionName,
          tSaveData,
          false
        })
      else
        UnlockMission(sMissionName)
      end
    end
    SetLastCompletedContractName(sMissionName)
  end
  
  tContainerConfig.tOnComplete = {
    {_OnContainerComplete}
  }
  
  local function _OnContainerCancel()
    local bIsContract = WifMissionData.IsMissionAContract(sMissionName)
    if bIsContract then
      MrxPlayState.Set(MrxPlayState._knFree)
    end
    _tActiveMissions[sMissionName] = nil
    if tMissionConfig.sStarter then
      UnlockMission(sMissionName, nil, false)
    end
  end
  
  tContainerConfig.tOnCancel = {
    {_OnContainerCancel}
  }
  oContainer:Configure(tContainerConfig)
  local bBriefing = false
  local oBriefing
  if tStarterData then
    bBriefing = true
    local bMissionActive = type(tSaveData) == "table" and tSaveData.nState == MrxTaskState._knActive
    if not bMissionActive then
      oBriefing = MrxTask:Create()
    else
    end
  end
  local bSkipMode = MrxCheatBootstrap.IsSkipModeEnabled()
  local sSkipToMissionId, bSkipToBriefing = MrxCheatBootstrap.GetMissionSkipData()
  sSkipToMissionId = sSkipToMissionId and GetCaseSensitiveMissionId(sSkipToMissionId)
  tMissionConfig.sName = sMissionName .. "Mission"
  tMissionConfig.sModuleName = tMissionConfig.sModuleName
  tMissionConfig.oParent = oContainer
  tMissionConfig.tLayers = tMissionConfig.tLayers or {
    "Vz_State_" .. sMissionName
  }
  tMissionConfig.tRewards = MrxRewardData.GetRewards(sMissionName)
  tMissionConfig.tStartLocations = GetMissionStartLocations(sMissionName)
  if bSkipMode and WifMissionData.IsMissionAJob(sMissionName) and sSkipToMissionId ~= sMissionName then
    tMissionConfig.bSkipInitialNotifications = true
  end
  if oBriefing then
    tMissionConfig.oBriefing = oBriefing
  end
  
  local function _OnAssetsLoaded()
    MrxState.Exit(MrxState.STATE_WAITFORSTREAMING)
    MrxState.Exit(MrxState.STATE_WAITFORGAME)
  end
  
  tMissionConfig.fOnAssetsLoaded = _OnAssetsLoaded
  
  local function _OnMissionComplete()
    oContainer:Complete()
  end
  
  tMissionConfig.tOnComplete = tMissionConfig.tOnComplete or {}
  table.insert(tMissionConfig.tOnComplete, {_OnMissionComplete})
  local bWager = false
  if tMissionConfig.tRewards and (tMissionConfig.tRewards.nWager or tMissionConfig.tRewards.nWagerPercent) then
    bWager = true
  end
  if bWager then
    local function _OnWagerComplete(sMissionName, bWin)
      WifPmcInterior.SetEntranceLock(false)
      
      MrxHqManager.UnlockAllHq()
      WifPmcInterior.SetWagerStatus(sMissionName, bWin)
      WifPmcInterior.Enter(true)
    end
    
    tMissionConfig.tOnCancel = tMissionConfig.tOnCancel or {}
    table.insert(tMissionConfig.tOnCancel, {
      _OnWagerComplete,
      {sMissionName, false}
    })
    tMissionConfig.tOnComplete = tMissionConfig.tOnComplete or {}
    table.insert(tMissionConfig.tOnComplete, {
      _OnWagerComplete,
      {sMissionName, true}
    })
  end
  oMission:Configure(tMissionConfig)
  
  local function _AddBriefingToStarter()
    if tMissionConfig.sStarter then
      local oStarter = MrxStarterManager.RequestStarter(tMissionConfig.sStarter)
      if oStarter then
        oStarter:AddBriefing(sMissionName, sMissionTitle)
      end
    end
  end
  
  local function _OnBriefingComplete()
    if _fPreContractSave then
      _fPreContractSave()
    end
    if tMissionConfig.sStarter then
      local oStarter = MrxStarterManager.RequestStarter(tMissionConfig.sStarter)
      if oStarter then
        oStarter:SetMissionAccepted(sMissionName, true)
        oStarter:RemoveBriefing(sMissionName)
      end
    end
    if bWager then
      WifPmcInterior.SetEntranceLock(true)
      MrxHqManager.LockAllHq()
    end
    MrxState.Enter(MrxState.STATE_WAITFORSTREAMING, oMission.Activate, {oMission, tSaveData})
  end
  
  if bSkipMode and sSkipToMissionId and not _bSkipToMissionReached then
    local bOkToSkip = true
    if WifMissionData.GetIsCompleteable(sMissionName) then
      bOkToSkip = true
    else
      bOkToSkip = false
    end
    if (sMissionName == "ChiCon003" or sMissionName == "AllCon003") and sSkipToMissionId ~= "PmcCon004" then
      bOkToSkip = false
    end
    if sMissionName == sSkipToMissionId then
      _bSkipToMissionReached = true
      _AttemptSkipModeExit()
      bBriefing = bSkipToBriefing
    elseif bOkToSkip and (WifMissionData.IsMissionOnCriticalPath(sMissionName) or WifMissionData.IsMissionAContract(sMissionName) and WifMissionData.GetMissionFaction(sMissionName) == WifMissionData.GetMissionFaction(sSkipToMissionId)) then
      oContainer:Complete()
      return true
    end
  end
  oContainer:Activate()
  AddPdaMissionDetails(sMissionName)
  bBlockingSequence = MrxUtil.SetDefault(bBlockingSequence, true)
  
  local function _OnInitialTaskActivate()
    if bBlockingSequence then
      _EndBlockingSequence()
    end
  end
  
  if bBlockingSequence then
    _BeginBlockingSequence()
  end
  if bBriefing then
    if oBriefing then
      local tBriefingConfig = {
        sModuleName = "MrxTask",
        sName = sMissionName .. "Briefing",
        oParent = oContainer,
        sMissionName = sMissionName,
        sFactionId = tMissionConfig.sFactionId,
        fOnActivate = _OnInitialTaskActivate,
        fOnComplete = _OnBriefingComplete
      }
      if not tStarterData.sHqName then
        local tStarterLayers = tStarterData.tLayers
        if tStarterLayers then
          tBriefingConfig.tLayers = tBriefingConfig.tLayers or {}
          for _, sLayerName in ipairs(tStarterLayers) do
            table.insert(tBriefingConfig.tLayers, sLayerName)
          end
          tStarterLayers = nil
        end
      end
      oBriefing:Configure(tBriefingConfig)
      oBriefing:Activate()
      _AddBriefingToStarter()
    else
      oMission:Configure({fOnActivate = _OnInitialTaskActivate})
      _AddBriefingToStarter()
      _OnBriefingComplete()
    end
  else
    oMission:Configure({fOnActivate = _OnInitialTaskActivate})
    _OnBriefingComplete()
  end
  _tActiveMissions[sMissionName] = {oMission = oMission}
  return true
end

function DestroyMission(sMissionName)
  _tActiveMissions[sMissionName] = nil
  Pda.Map:RemoveMission({sName = sMissionName})
  if Net.IsServer() then
    Net.SendEvent_RemovePDAMission(WifMissionData.GetMissionIndexFromId(sMissionName))
  end
  local sStarter = WifMissionData.GetMissionStarter(sMissionName)
  if sStarter then
    local oStarter = MrxStarterManager.RequestStarter(sStarter)
    if oStarter then
      oStarter:RemoveBriefing(sMissionName)
    end
  end
end

function GetMissionStartLocations(sMissionName)
  sMissionName = GetCaseSensitiveMissionId(sMissionName)
  local tMissionConfig = WifMissionData.tMissionData[sMissionName]
  if not tMissionConfig then
    return nil
  end
  if tMissionConfig.tStartLocations then
    return tMissionConfig.tStartLocations
  end
  return GetBriefingStartLocations(sMissionName)
end

function GetBriefingStartLocations(sMissionName, bGetEntrance)
  sMissionName = GetCaseSensitiveMissionId(sMissionName)
  local tMissionConfig = WifMissionData.tMissionData[sMissionName]
  if not tMissionConfig then
    return
  end
  local sStarterName = tMissionConfig.sStarter
  if not sStarterName then
    return
  end
  local tStarterData = WifStarterData[sStarterName]
  if type(tStarterData) ~= "table" then
    return
  end
  if tStarterData.sHqName then
    local tHqData = WifHqData.GetHqConfigFromId(tStarterData.sHqName)
    if tHqData and tHqData.tPortal then
      if bGetEntrance then
        return tHqData.tPortal.sEntrance
      else
        return {
          tHqData.tPortal.sStart1 or tHqData.tPortal.sStart,
          tHqData.tPortal.sStart2 or tHqData.tPortal.sStart
        }
      end
    end
  elseif tStarterData.bPmcStarter then
    if bGetEntrance then
      return "Starter_Pmc_Entrance"
    else
      return {
        "Starter_Pmc_Start1",
        "Starter_Pmc_Start2"
      }
    end
  end
end

function DoesMissionHaveABriefing(sMissionName)
  local tMissionConfig = WifMissionData.tMissionData[sMissionName]
  if tMissionConfig then
    local sStarterName = tMissionConfig.sStarter
    if sStarterName then
      local tStarterData = WifStarterData[sStarterName]
      if tStarterData then
        return true
      end
    end
  end
  return false
end

function AwardKey(sKeyName, vValue)
  if _bCurrentlyRefreshing then
    _tDeferredKeyAwards = _tDeferredKeyAwards or {}
    table.insert(_tDeferredKeyAwards, {sKeyName, vValue})
    return
  end
  if vValue == nil then
    if type(_tMyFlowData[sKeyName]) == "number" then
      vValue = _tMyFlowData[sKeyName] + 1
    else
      vValue = 1
    end
  end
  _tMyFlowData[sKeyName] = vValue
  MrxRewardData.GrantRewardKey(sKeyName)
end

function RemoveKey(sKeyName)
  if _tMyFlowData then
    _tMyFlowData[sKeyName] = nil
  end
end

function HasKey(sKeyName)
  if _tMyFlowData then
    return _tMyFlowData[sKeyName] ~= nil
  end
  return false
end

function GetKeyValue(sKeyName)
  if _tMyFlowData then
    return _tMyFlowData[sKeyName] or 0
  end
  return 0
end

function Refresh(fCallback, tCallbackArgs)
  if not (_bEnable and _tFlowData) or _bCurrentlyRefreshing then
    return
  end
  _bCurrentlyRefreshing = true
  if fCallback then
    Debug.Printf("@@@@@@@@@@ MrxMissionFlow.Refresh: setting refresh callback")
    _fRefreshCallback = fCallback
    _tRefreshCallbackArgs = tCallbackArgs
  end
  _BeginBlockingSequence()
  local nBindings = 0
  for sBindingName, tBinding in pairs(_tFlowData) do
    nBindings = nBindings + 1
  end
  Debug.Printf("Refreshing (" .. nBindings .. " bindings)...")
  local bActionTaken = false
  local tTemp = {}
  for sKey, tBinding in pairs(_tFlowData) do
    if type(tBinding.fPrereq) == "function" and type(tBinding.fConseq) == "function" and tBinding.fPrereq() then
      tBinding.fConseq()
      bActionTaken = true
      Debug.Printf("Executing action (binding \"" .. sKey .. "\") based on fulfilled prereqs.")
      if not tBinding.bRecurring then
        table.insert(_tCulledBindings, sKey)
        Debug.Printf("Culling binding \"" .. sKey .. "\"")
      else
        tTemp[sKey] = tBinding
      end
    else
      tTemp[sKey] = tBinding
    end
  end
  _tFlowData = tTemp
  _bCurrentlyRefreshing = nil
  if _tDeferredKeyAwards then
    for i, tKeyValuePair in ipairs(_tDeferredKeyAwards) do
      local sKeyName = tKeyValuePair[1]
      local vValue = tKeyValuePair[2]
      AwardKey(sKeyName, vValue)
    end
    _tDeferredKeyAwards = nil
  end
  if bActionTaken then
    Refresh()
  else
    Debug.Printf("No additional actions taken.")
  end
  _EndBlockingSequence()
end

function SetMissionToRepeat(sMissionName)
  _tMissionsToRepeat[sMissionName] = true
end

function AcceptMissions(tAcceptedMissions, sLastAcceptedMission)
  DisableAllJobTracking()
  local sContractToActivate
  for _, sMissionName in ipairs(tAcceptedMissions) do
    if WifMissionData.IsMissionAContract(sMissionName) then
      sContractToActivate = sMissionName
    end
  end
  if MrxPlayState.IsFree() then
    local sSelectedMission = sContractToActivate or sLastAcceptedMission
    if sSelectedMission then
      _sTrackedMissionName = sSelectedMission
    end
  end
  for _, sMissionName in ipairs(tAcceptedMissions) do
    local oContainer = _oParent:GetChild(sMissionName)
    local oBriefing = oContainer:GetChild(sMissionName .. "Briefing")
    oBriefing:Complete()
  end
end

function DisableAllJobTracking()
  for sMissionName, tMissionData in pairs(_tActiveMissions) do
    local oMission = tMissionData.oMission
    if oMission and oMission:IsActive() and oMission.IsJob() then
      oMission:EnableTracking(false)
    end
  end
end

function SaveSingleton()
  local tActiveMissionSaveData = {}
  for sMissionName, tMissionData in pairs(_tActiveMissions) do
    tActiveMissionSaveData[sMissionName] = tMissionData.oMission:SaveInstance()
  end
  return {
    tMyFlowData = _tMyFlowData,
    tActiveMissions = tActiveMissionSaveData,
    tCulledBindings = _tCulledBindings
  }
end

function LoadSingleton(tSaveData)
  if type(tSaveData) == "table" then
    _tMyFlowData = tSaveData.tMyFlowData
    local nBindingsToCull = 0
    for _, sKeyToCull in pairs(tSaveData.tCulledBindings) do
      if _tFlowData[sKeyToCull] then
        _tFlowData[sKeyToCull].bToBeCulled = true
        nBindingsToCull = nBindingsToCull + 1
      end
    end
    Debug.Printf("Culling " .. nBindingsToCull .. " bindings....")
    if 0 < nBindingsToCull then
      local tTemp = {}
      for sKey, tBinding in pairs(_tFlowData) do
        if not tBinding.bToBeCulled then
          tTemp[sKey] = tBinding
        else
          Debug.Printf("Culling binding \"" .. sKey .. "\"")
          table.insert(_tCulledBindings, sKey)
        end
      end
      _tFlowData = tTemp
    end
    for sMissionName, tMissionSaveData in pairs(tSaveData.tActiveMissions) do
      UnlockMission(sMissionName, tMissionSaveData, false)
    end
  end
end

function GetCaseSensitiveMissionId(sMissionId)
  for sCaseSensitiveId in pairs(WifMissionData.tMissionData) do
    if string.lower(sMissionId) == string.lower(sCaseSensitiveId) then
      return sCaseSensitiveId
    end
  end
end

function SetLastCompletedContractName(sName)
  _sLastCompletedContractName = sName
end

function GetLastCompletedContractName()
  return _sLastCompletedContractName
end

function EnableAutosave()
  _bDoMissionAutosave = true
end

function IsCheckpointSaveModeEnabled()
  return _bCheckpointSaveMode
end

function EnableCheckpointSaveMode(bEnable)
  _bCheckpointSaveMode = bEnable
end

function SetRetryLocations(tLocations)
  _tRetryLocations = tLocations
end

function GetRetryLocations()
  return _tRetryLocations
end

function SetPersistentRetryLocations(tLocations)
  _tPersistentRetryLocations = tLocations
end

function GetPersistentRetryLocations()
  return _tPersistentRetryLocations
end

function SetGrappleEnabled(bEnable)
  if Net.IsServer() then
    if bEnable == false then
      SetGrapple = 0
    elseif bEnable == true then
      SetGrapple = 1
    end
    Net.SendCustomEvent("MrxMissionFlow", NETEVENT_SETGRAPPLE, {SetGrapple}, true)
    Event.Delete(_evSetGrapple)
    _evSetGrapple = nil
    _evSetGrapple = Event.CreatePersistent(Event.ScriptEvent, {
      "mpPlayerJoin",
      function(tData)
        return Net.IsServer() and not Player.IsLocal(tData[1])
      end
    }, Net.SendCustomEvent, {
      "MrxMissionFlow",
      NETEVENT_SETGRAPPLE,
      {SetGrapple},
      true
    })
  end
  local tPlayers = Player.GetAllPlayers()
  for _, uGuid in ipairs(tPlayers) do
    Player.SetGrappleEnabled(uGuid, bEnable)
  end
  _bGrappleEnabled = bEnable
end

function SetVehicleDisguiseEnabled(bEnable)
  if Net.IsServer() then
    if bEnable == false then
      SetVehicleDisguise = 0
    elseif bEnable == true then
      SetVehicleDisguise = 1
    end
    Net.SendCustomEvent("MrxMissionFlow", NETEVENT_SETVEHICLEDISGUISE, {SetVehicleDisguise}, true)
    Event.Delete(_evSetVehicleDisguise)
    _evSetVehicleDisguise = nil
    _evSetVehicleDisguise = Event.CreatePersistent(Event.ScriptEvent, {
      "mpPlayerJoin",
      function(tData)
        return Net.IsServer() and not Player.IsLocal(tData[1])
      end
    }, Net.SendCustomEvent, {
      "MrxMissionFlow",
      NETEVENT_SETVEHICLEDISGUISE,
      {SetVehicleDisguise},
      true
    })
  end
  Player.SetVehicleDisguise(bEnable)
  _bVehicleDisguiseEnabled = bEnable
end

function NetEventCallback(nEventId, tArgs)
  if nEventId == NETEVENT_SETGRAPPLE and tArgs[1] == 0 then
    SetGrappleEnabled(false)
  elseif nEventId == NETEVENT_SETGRAPPLE and tArgs[1] == 1 then
    SetGrappleEnabled(true)
  elseif nEventId == NETEVENT_AUTOSAVE then
    Autosave()
  elseif nEventId == NETEVENT_SETVEHICLEDISGUISE and tArgs[1] == 0 then
    SetVehicleDisguiseEnabled(false)
  elseif nEventId == NETEVENT_SETVEHICLEDISGUISE and tArgs[1] == 1 then
    SetVehicleDisguiseEnabled(true)
  end
end

function Autosave()
  local inMission = MrxPlayState.Get() == MrxPlayState._knMission
  local lastMission = WifMissionFlow.GetLastCompletedContractName() or "none"
  local fMissionTime = MrxPlayState.GetTotalTimeElapsed()
  local oCurrentContract = MrxPlayState.GetCurrentMission()
  if oCurrentContract and oCurrentContract:IsActive() then
    local sMissionId = oCurrentContract:GetMissionId()
    if sMissionId then
      local tRewards = MrxRewardData.GetRewards(sMissionId)
      if tRewards then
        local tWagerData = MrxRewardData.GetWagerData(tRewards)
        if tWagerData then
          return
        end
      end
    end
  end
  Sys.RequestAutosave(inMission, lastMission, fMissionTime, MrxStatsManager.GetPercentCompleted())
  Pg.SaveGame("autosave")
end

function IsGrappleEnabled()
  return _bGrappleEnabled
end

function IsVehicleDisguiseEnabled()
  return _bVehicleDisguiseEnabled
end

function EnableResourceCounters(bEnable)
  Hud.ResourceCounter:SetSuppressed({
    bSuppressCash = not bEnable,
    bSuppressFuel = not bEnable
  })
  _bResourceCountersEnabled = bEnable
end

function AreResourceCountersEnabled()
  return _bResourceCountersEnabled
end

function GetMissionStates()
  local tReturn = {}
  local index = 1
  for sMissionId, tMissionData in pairs(WifMissionData.tMissionData) do
    local sValue = "incomplete"
    if HasKey(sMissionId) then
      sValue = "complete"
    end
    tReturn[index] = {sMissionId, sValue}
    index = index + 1
  end
  local oCurrentMission = MrxPlayState.GetCurrentMission()
  if oCurrentMission and oCurrentMission:IsActive() then
    local sMissionId = oCurrentMission:GetMissionId()
    tReturn[index] = {sMissionId, "active"}
    index = index + 1
  end
  return tReturn
end

function _BeginBlockingSequence()
  _nBlockingSequences = _nBlockingSequences or 0
  _nBlockingSequences = _nBlockingSequences + 1
end

function _EndBlockingSequence()
  if type(_nBlockingSequences) == "number" and _nBlockingSequences >= 1 then
    _nBlockingSequences = _nBlockingSequences - 1
    if _nBlockingSequences == 0 then
      Debug.Printf("@@@@@@@@@@ _EndBlockingSequence: _bDoMissionAutosave=" .. tostring(_bDoMissionAutosave))
      if MrxCheatBootstrap.IsSkipModeEnabled() then
        _AttemptSkipModeExit()
      else
        if Sys.RequestAutosave and _bDoMissionAutosave then
          _bDoMissionAutosave = nil
          Autosave()
          Net.SendCustomEvent("MrxMissionFlow", NETEVENT_AUTOSAVE, {})
        end
        _RefreshComplete()
      end
    end
  end
end

function _AttemptSkipModeExit()
  if _bSkipToMissionReached and _nBlockingSequences == 0 then
    MrxCheatBootstrap.EnableSkipMode(false)
    _bSkipToMissionReached = nil
    MrxLayerManager.ProcessMarkedLayers(_RefreshComplete)
  end
end

function _RefreshComplete()
  if _fRefreshCallback then
    MrxUtil.CallWithOptionalArgs(_fRefreshCallback, _tRefreshCallbackArgs)
    Debug.Printf("@@@@@@@@@@ MrxMissionFlow._RefreshComplete: resetting refresh callback")
    _fRefreshCallback = nil
    _tRefreshCallbackArgs = nil
  else
    Debug.Printf("@@@@@@@@@@ MrxMissionFlow._RefreshComplete: exited, but no refresh callback set")
  end
end

function AddPdaMissionDetails(sMissionId, tObjectives, bSelectedMission, nLevel)
  if type(sMissionId) ~= "string" then
    sMissionId = WifMissionData.GetMissionIdFromIndex(sMissionId)
  end
  if WifMissionData.IsMissionSuppressedInPda(sMissionId) then
    return
  end
  if Net.IsClient() and nLevel then
    if not _tMyFlowData then
      _tMyFlowData = {}
    end
    _tMyFlowData[sMissionId] = nLevel
  end
  local sHeader = BuildMissionHeader(sMissionId, tObjectives)
  local sDesc = BuildMissionDescription(sMissionId, false, true, tObjectives)
  local sFactionId = WifMissionData.GetMissionFaction(sMissionId)
  local sPdaFaction = MrxFactionManager.GetPdaFactionIdFromFactionId(sFactionId) or "VZ"
  local sTexture = WifMissionData.GetMissionPdaTexture(sMissionId)
  if not sTexture then
    local tFactionIdToTexture = {
      Pmc = "icon_pmc_mc",
      All = "icon_an_mc",
      Chi = "icon_ch_mc",
      Gur = "icon_gr_mc",
      Oil = "icon_oc_mc",
      Pir = "icon_pr_mc",
      Vza = "icon_vz_mc"
    }
    sTexture = tFactionIdToTexture[sFactionId]
  end
  local bContract = WifMissionData.IsMissionAContract(sMissionId)
  if Net.IsServer() then
    bSelectedMission = Pda.Map:GetSelectedMission() == sMissionId
  end
  local nSortOrder = WifMissionData.GetPdaSortOrder(sMissionId)
  if not nSortOrder and bContract then
    local bCriticalPath = WifMissionData.IsMissionOnCriticalPath(sMissionId)
    if bSelectedMission then
      nSortOrder = WifMissionData.knPdaSortOrderActiveContract
    elseif bCriticalPath then
      nSortOrder = WifMissionData.knPdaSortOrderCritPathContract
    else
      nSortOrder = WifMissionData.knPdaSortOrderContract
    end
  end
  Pda.Map:AddMission({
    sName = sMissionId,
    sLabel = sHeader,
    sDesc = sDesc,
    sFaction = sPdaFaction,
    sDefaultBlipTexture = sTexture,
    sDefaultBlipLabel = "Default Mission Blip description",
    bTrackable = false,
    nSortOrder = nSortOrder
  })
  if bContract then
    local uBriefingEntrance
    local sBriefingEntrance = GetBriefingStartLocations(sMissionId, true)
    if sBriefingEntrance then
      uBriefingEntrance = Pg.GetGuidByName(sBriefingEntrance)
    end
    if uBriefingEntrance and not bSelectedMission then
      Pda.Map:AddBlip({
        sMission = sMissionId,
        sName = sMissionId .. "_PreMission",
        uGuid = uBriefingEntrance,
        sTexture = "",
        nSortOrder = 5
      })
    else
      Pda.Map:RemoveBlip({
        sName = sMissionId .. "_PreMission"
      })
    end
  end
  if Net.IsServer() then
    tObjectives = tObjectives or {}
    if WifMissionData.GetMissionRepeatable(sMissionId) then
      local nLevel = GetKeyValue(sMissionId)
      Net.SendEvent_AddPDAMission(WifMissionData.GetMissionIndexFromId(sMissionId), tObjectives, bSelectedMission, nLevel)
    else
      Net.SendEvent_AddPDAMission(WifMissionData.GetMissionIndexFromId(sMissionId), tObjectives, bSelectedMission)
    end
  end
  if Net.IsClient() and bSelectedMission then
    Pda.Map:SetSelectedMission({sName = sMissionId})
  end
end

function BuildMissionHeader(sMissionId, tObjectives)
  local sHeader
  if WifMissionData.IsMissionAContract(sMissionId) then
    sHeader = "\"" .. WifMissionData.GetMissionTitle(sMissionId) .. "\""
    if WifMissionData.GetMissionRepeatable(sMissionId) then
      local nLevel = GetKeyValue(sMissionId) + 1
      local nLevels = WifMissionData.GetMissionLevels(sMissionId)
      if nLevel > nLevels then
        nLevel = nLevels
      end
      sHeader = sHeader .. " ([Generic.Level] " .. nLevel .. ")"
    end
  elseif tObjectives and #tObjectives ~= 0 then
    sHeader = tObjectives[1][1]
  else
    sHeader = "\"" .. WifMissionData.GetMissionTitle(sMissionId) .. "\""
  end
  return sHeader
end

function BuildMissionDescription(sMissionId, bPrependHeader, bIncludeRecommendations, tObjectives)
  local sDesc = ""
  if bPrependHeader then
    sDesc = sDesc .. BuildMissionHeader(sMissionId) .. [[


]]
  end
  local sFactionId = WifMissionData.GetMissionFaction(sMissionId)
  local sFactionName = MrxFactionManager.GetShortPlayerVisibleName(sFactionId)
  local sFactionIcon = MrxFactionManager.GetInlineIcon(sFactionId)
  if sFactionName and sFactionIcon then
    sDesc = sDesc .. "[Briefing.Faction]: " .. sFactionIcon .. " " .. sFactionName .. [[


]]
  end
  sDesc = sDesc .. "[" .. sMissionId .. ".Terms.Summary]\n"
  if tObjectives and 0 < #tObjectives and WifMissionData.IsMissionAContract(sMissionId) then
    sDesc = sDesc .. [[

[PDA.Map.ObjectiveListHeader]
]]
    for nIndex, tObjective in pairs(tObjectives) do
      local sObjectiveDesc = tObjective[1]
      local sInlineIcon = tObjective[2] or ""
      if Net.IsClient() and type(tObjective[2]) ~= "string" then
        sInlineIcon = MrxUtil.GetInlineIconNameByIndex(tObjective[2])
      end
      sDesc = sDesc .. sInlineIcon .. " " .. sObjectiveDesc .. "\n"
    end
  end
  if bIncludeRecommendations then
    local sRecString = WifRecommendationData.GenerateRecommendationString(sMissionId)
    if sRecString then
      sDesc = sDesc .. [[

[PDA.Map.RecommendationsHeader]
]] .. sRecString
    end
  end
  local tRewards = MrxRewardData.GetRewards(sMissionId)
  if tRewards then
    local tWagerData = MrxRewardData.GetWagerData(tRewards)
    if tWagerData then
      local sWagerText = MrxUtil.FormatMoney(tRewards.nWagered or tWagerData.nDefaultWager)
      sDesc = sDesc .. [[

[Briefing.WagerPrefix] ]] .. sWagerText .. "\n"
    end
  end
  local sRewardString = MrxRewardData.GenerateRewardString(sMissionId)
  if sRewardString then
    sDesc = sDesc .. [[

[Generic.Rewards]:
]] .. sRewardString
  end
  return sDesc
end

function RefreshAllPdaMissionDetails()
  if Net.IsClient() then
    return
  end
  for sMissionId, tMissionData in pairs(_tActiveMissions) do
    local oMission = tMissionData.oMission
    if oMission:IsActive() then
      if not oMission.RefreshPdaDisplay then
        Debug.Printf("@@@@@@@@@@ RefreshPdaDisplay is nil for mission " .. sMissionId)
      else
        oMission:RefreshPdaDisplay()
      end
    else
      AddPdaMissionDetails(sMissionId)
    end
  end
end

function RemovePDAMission(sMissionName)
  if type(sMissionName) ~= "string" then
    sMissionName = WifMissionData.GetMissionIdFromIndex(sMissionName)
  end
  Pda.Map:RemoveMission({sName = sMissionName})
end
