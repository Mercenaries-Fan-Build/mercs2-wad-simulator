inherit("MrxTaskMission")
import("MrxLayerManager")
import("MrxPlayState")
import("MrxRewardData")
import("MrxVoSequence")
import("WifMissionFlow")
import("MrxFactionManager")

function LoadAssets(self, tSaveData)
  if tSaveData ~= nil then
    self._nTargetsComplete = tSaveData._nTargetsComplete
  end
  if type(self._tTargets) == "table" then
    local tLayers = {}
    local tLayerKeys = self._GetPerTargetLayerKeys()
    for sName, tTargetData in pairs(self._tTargets) do
      local bTargetComplete = type(tSaveData) == "table" and type(tSaveData.tTargets) == "table" and tSaveData.tTargets[sName]
      if bTargetComplete then
        tTargetData.bComplete = true
      else
        for _, sKey in ipairs(tLayerKeys) do
          if tTargetData[sKey] then
            table.insert(tLayers, tTargetData[sKey])
          end
        end
      end
    end
    MrxLayerManager.Add(tLayers, self.AssetsLoaded, {self})
  else
    MrxTaskMission.LoadAssets(self, tSaveData)
  end
end

function Activated(self)
  local tConfig = self:GetConfig()
  self._bTrackOnActivate = MrxUtil.SetDefault(tConfig.bTrackOnActivate, false)
  self._bSkipInitialNotifications = MrxUtil.SetDefault(tConfig.bSkipInitialNotifications, false)
  self._tTargetGuidsToNames = {}
  if type(self._tTargets) == "table" then
    for sName, tTargetData in pairs(self._tTargets) do
      if not tTargetData.bComplete then
        local uGuid = Pg.GetGuidByName(sName)
        if uGuid == nil then
          Debug.Printf("Can't find guid for name:\"" .. sName .. "\" Adding to the table anyway so that the script will generate an assert")
          self._tTargetGuidsToNames[uGuid] = sName
        else
          self._tTargetGuidsToNames[uGuid] = sName
        end
      end
    end
  end
  self._bNearVoInProgress = false
  MrxTaskMission.Activated(self)
end

function _AddTarget(self, sTarget, sTargetLayer)
  if not self._tTargets then
    self._tTargets = {}
    self._nTargetsComplete = 0
  end
  if self._tTargets[sTarget] then
    Debug.Printf("Warning: target \"" .. sTarget .. "\" already added!  Overwriting...")
  end
  self._tTargets[sTarget] = {sTargetLayer = sTargetLayer}
  return sTarget
end

function _SetTargetMilestoneKey(self, sTarget, sMilestoneKey)
  local tTarget = self:_GetTargetData(sTarget)
  if tTarget then
    tTarget.sMilestoneKey = sMilestoneKey
  end
end

function _GetTargetData(self, vTarget)
  local sType = type(vTarget)
  if sType == "string" then
    return self._tTargets[vTarget]
  elseif sType == "userdata" then
    local sTargetName = self._tTargetGuidsToNames[vTarget]
    if sTargetName then
      return self._tTargets[sTargetName]
    end
  else
    return self._tTargets
  end
end

function _GetTargetList(self)
  local tTargets = {}
  for sTargetName, tTargetData in pairs(self._tTargets) do
    if not tTargetData.bComplete == true then
      table.insert(tTargets, sTargetName)
    end
  end
  return tTargets
end

function _GetPartsCompletedList(self)
  local tTargets = {}
  if not self._tTargets then
    return nil
  end
  for sTargetName, tTargetData in pairs(self._tTargets) do
    if tTargetData.bComplete == true then
      table.insert(tTargets, sTargetName)
    end
  end
  return tTargets
end

function _TargetComplete(self, uGuid)
  local bKeysAwarded = false
  local bQuotaMet = self._oObjective:IsQuotaMet()
  if not self._nTargetsComplete then
    self._nTargetsComplete = 0
  end
  self._nTargetsComplete = self._nTargetsComplete + 1
  local bAutosaveAfterEveryTarget = self._GetAutosaveMode()
  
  local function _Presentation()
    local bRefreshFlow = bKeysAwarded and not bQuotaMet
    if not bRefreshFlow and not bAutosaveAfterEveryTarget then
      return
    end
    local oContract = MrxPlayState.GetCurrentMission()
    if oContract then
      oContract._Checkpoint(nil, true, true)
    end
    if bRefreshFlow then
      WifMissionFlow.EnableAutosave()
      WifMissionFlow.Refresh()
    else
      WifMissionFlow.Autosave()
    end
  end
  
  MrxRewardData.GrantRewardKey(self:GetMissionId() .. "_PerTarget")
  if type(self._tTargets) == "table" then
    local tTargetData = self:_GetTargetData(uGuid)
    if tTargetData then
      tTargetData.bComplete = true
      if tTargetData.sTargetLayer then
        MrxLayerManager.MarkForRemoval(tTargetData.sTargetLayer)
      end
      if tTargetData.sMilestoneKey then
        WifMissionFlow.AwardKey(tTargetData.sMilestoneKey)
        bKeysAwarded = true
      end
    end
  end
  local tConfig = self:GetConfig()
  local tMilestones = tConfig.tMilestones
  if type(tMilestones) == "table" then
    for i, tMilestoneData in pairs(tMilestones) do
      if type(tMilestoneData.nMilestone) == "number" and tMilestoneData.nMilestone == self._nTargetsComplete then
        WifMissionFlow.AwardKey(tMilestoneData.sKey)
        bKeysAwarded = true
      end
    end
  end
  if self._tTargetCompleteVo then
    _PlayRandomVoSequenceFromTable(self._tTargetCompleteVo, self._nTargetsComplete, _Presentation)
  else
    _Presentation()
  end
end

function _ExcludeCompletedTargets(self)
  return
end

function _SetTargetCompleteVo(self, tVo)
  if type(tVo) == "table" then
    self._tTargetCompleteVo = tVo
  end
end

function _SetTargetNearbyVo(self, tVo)
  if type(tVo) == "table" then
    self._tTargetNearbyVo = tVo
  end
end

function _SetShortDescription(self, sDspShortDesc)
  self._sDspShortDesc = sDspShortDesc
end

function EnableTracking(self, bEnable)
  if self._oObjective then
    self._oObjective:EnableTracking(bEnable)
  end
end

function _AddToPda(self)
end

function CreateChild(self, tConfig)
  if tConfig then
    tConfig.bOptional = true
  end
  return MrxTaskMission.CreateChild(self, tConfig)
end

function _GetMissionType()
  return MrxTaskMission._knJob
end

function IsJob()
  return true
end

function _GetPerTargetLayerKeys()
  return {
    "sTargetLayer"
  }
end

function _GetNearRadius()
  return 30
end

function _GetFarRadius()
  return 60
end

function _GetAutosaveMode()
  return true
end

function _GetNearbyVoPlaybackMode()
  return false
end

function SaveInstance(self)
  local tSaveData = MrxTaskMission.SaveInstance(self)
  if type(self._tTargets) == "table" then
    for sTargetName, tTargetData in pairs(self._tTargets) do
      Debug.Printf("SaveInstance() " .. sTargetName)
      if "table" == type(tTargetData) and tTargetData.bComplete then
        if not tSaveData.tTargets then
          tSaveData.tTargets = {}
        end
        tSaveData.tTargets[sTargetName] = true
      end
    end
  end
  tSaveData._nTargetsComplete = self._nTargetsComplete
  return tSaveData
end

function _RemoveSecondaryNearbyEvent(self)
  if self.uSecondaryProximity then
    for i, handle in ipairs(self._tEvents) do
      if handle == self.uSecondaryProximity then
        Event.Delete(handle)
        table.remove(self._tEvents, i)
        break
      end
    end
    self.uSecondaryProximity = nil
  end
end

function _CreateSecondaryNearbyEvent(self)
  if self.uSecondaryProximity then
    self:_RemoveSecondaryNearbyEvent()
  end
  self.uSecondaryProximity = self:_CreatePersistentEvent(Event.ObjectProximity, {
    self._uFarTgtFilter,
    Player.GetSecondaryCharacter(),
    "<",
    self._GetNearRadius()
  }, self._NearbyRadiusEntry, {self})
end

function _PlayerJoin(self)
  if Player.GetSecondaryCharacter() then
    self:_CreateSecondaryNearbyEvent()
  else
    self:_RemoveSecondaryNearbyEvent()
  end
end

function _CreateNearbyEvent(self)
  self._uFarTgtFilter = ObjectFilter.Copy(self._oObjective:GetTargetObjectFilter())
  ObjectFilter.RemoveObject(self._uFarTgtFilter, Player.GetAnyCharacter())
  ObjectFilter.RemoveObject(self._uFarTgtFilter, Player.GetAllCharacters())
  local uHandle = self:_CreatePersistentEvent(Event.ObjectProximity, {
    self._uFarTgtFilter,
    Player.GetLocalCharacter(),
    "<",
    self._GetNearRadius()
  }, self._NearbyRadiusEntry, {self})
  if Player.GetSecondaryCharacter() then
    self:_CreateSecondaryNearbyEvent()
  end
  local uPlayerJoin = self:_CreatePersistentEvent(Event.ScriptEvent, {
    "mpPlayerJoin",
    function(tData)
      return Net.IsServer() and not Player.IsLocal(tData[1])
    end
  }, self._PlayerJoin, {self})
end

function _NearbyRadiusEntry(self, tGuids)
  for i, uGuid in ipairs(tGuids) do
    self:_TargetNearby(uGuid)
    local bIncluded = false
    local tIncludedObjects = ObjectFilter.GetObjects(self._uFarTgtFilter, false)
    for i, uIncludedObject in ipairs(tIncludedObjects) do
      if uGuid == uIncludedObject then
        bIncluded = true
        break
      end
    end
    if bIncluded then
      ObjectFilter.RemoveObject(self._uFarTgtFilter, uGuid)
    else
      ObjectFilter.AddObject(self._uFarTgtFilter, uGuid, true)
    end
    self:_CreateFarawayEvent(uGuid)
  end
end

function _TargetNearby(self, uGuid)
  local bPlay = true
  if not MrxPlayState.IsFree() then
    bPlay = self._GetNearbyVoPlaybackMode()
  end
  if bPlay and self._bNearVoInProgress == false then
    self._bNearVoInProgress = true
    _PlayRandomVoSequenceFromTable(self._tTargetNearbyVo, nil, self._NearVoComplete, {self})
  end
end

function _CreateFarawayEvent(self, uGuid)
  local uHandle = self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAllCharacters(),
    uGuid,
    ">",
    self._GetFarRadius()
  }, self._NearbyRadiusExit, {self, uGuid})
end

function _NearbyRadiusExit(self, uGuid)
  self:_TargetFaraway(uGuid)
  local bExcluded = false
  local tExcludedObjects = ObjectFilter.GetObjects(self._uFarTgtFilter, false)
  for i, uExcludedObject in ipairs(tExcludedObjects) do
    if uGuid == uExcludedObject then
      bExcluded = true
      break
    end
  end
  if bExcluded then
    ObjectFilter.RemoveObject(self._uFarTgtFilter, uGuid)
  else
    ObjectFilter.AddObject(self._uFarTgtFilter, uGuid, false)
  end
  ObjectFilter.AddObject(self._uFarTgtFilter, uGuid, false)
end

function _TargetFaraway(self, uGuid)
end

function _PlayRandomVoSequenceFromTable(tVo, nRangeFilter, fCallback, tCallbackArgs)
  if type(tVo) ~= "table" then
    return
  end
  local tHat = {}
  for i, tElem in ipairs(tVo) do
    local bWithinRange = true
    local tRange = tElem.tRange
    if tRange and nRangeFilter ~= nil then
      local nLowerBound, bLowerBoundInclusive, nUpperBound, bUpperBoundInclusive
      local n = #tRange
      if n == 4 then
        nLowerBound = tRange[2]
        bLowerBoundInclusive = tRange[1] == "["
        nUpperBound = tRange[3]
        bUpperBoundInclusive = tRange[4] == "]"
      elseif n == 2 then
        if type(tRange[1]) == "number" then
          nUpperBound = tRange[1]
          bUpperBoundInclusive = tRange[2] == "]"
        else
          nLowerBound = tRange[2]
          bLowerBoundInclusive = tRange[1] == "["
        end
      elseif n == 1 then
        nLowerBound = tRange[1]
        bLowerBoundInclusive = true
        nUpperBound = tRange[1]
        bUpperBoundInclusive = true
      end
      local bWithinLowerBound = true
      if nLowerBound then
        bWithinLowerBound = nRangeFilter > nLowerBound
        if bLowerBoundInclusive then
          bWithinLowerBound = nRangeFilter >= nLowerBound
        end
      end
      local bWithinUpperBound = true
      if nUpperBound then
        bWithinUpperBound = nRangeFilter < nUpperBound
        if bUpperBoundInclusive then
          bWithinUpperBound = nRangeFilter <= nUpperBound
        end
      end
      bWithinRange = bWithinLowerBound and bWithinUpperBound
    end
    if bWithinRange then
      local nWeight = tElem.nWeight or 1
      for j = 1, nWeight do
        table.insert(tHat, i)
      end
    end
  end
  if table.getn(tHat) <= 0 then
    return
  end
  local nWinner = MrxUtil.GetRandomTableElement(tHat)
  local vSequence = tVo[nWinner].vSequence
  if vSequence then
    if fCallback then
      if type(vSequence) ~= "table" then
        vSequence = {vSequence}
      end
      table.insert(vSequence, {fCallback, tCallbackArgs})
    end
    MrxVoSequence.Start(vSequence, false, MrxVoSequence.knPriorityBounties)
  end
end

function _NearVoComplete(self)
  self._bNearVoInProgress = false
end
