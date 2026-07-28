inherit("MrxTaskJob")
import("MrxUtil")
import("MrxStatsManager")

function _AddTarget(self, ...)
  local sType = type(arg[1])
  if sType == "table" then
    local tConfig = arg[1]
    MrxTaskJob._AddTarget(self, tConfig.sTarget)
    local tTargetData = self:_GetTargetData(tConfig.sTarget)
    if tTargetData then
      tTargetData.sStagingLayer = tConfig.sStagingLayer
      tTargetData.sDefenseLayer = tConfig.sDefenseLayer
      tTargetData.sPristineLayer = tConfig.sPristineLayer
      tTargetData.sDestroyedLayer = tConfig.sDestroyedLayer
    end
    self:_SetTargetMilestoneKey(tConfig.sTarget, tConfig.sMilestoneKey)
  else
    MrxTaskJob._AddTarget(self, unpack(arg))
  end
end

function _Go(self, fCallback, tCallbackArgs)
  self:_ExcludeCompletedTargets()
  self:_AddToPda()
  local nTempCompletedParts = 0
  if self:_GetPartsCompletedList() then
    nTempCompletedParts = #self:_GetPartsCompletedList()
  end
  self._oObjective = self:CreateChild({
    sName = "DestroySet",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = self:_GetTargetList(),
    vTgtExclude = self:_GetPartsCompletedList(),
    nPartsCompleted = nTempCompletedParts,
    nQuota = #self:_GetTargetList() + nTempCompletedParts,
    sDspShortDesc = self._sDspShortDesc or "[Generic.ObjectiveDestroy]",
    bDspBounty = true,
    sDspBlpPdaIcon = "icon_destroy_3_mc",
    bTrackOnActivate = self._bTrackOnActivate,
    bSkipInitialNotifications = self._bSkipInitialNotifications,
    fOnActivate = function()
      self:_CreateNearbyEvent()
      MrxUtil.CallWithOptionalArgs(fCallback, tCallbackArgs)
    end,
    fOnPartComplete = function(uGuid)
      local tTargetData = self:_GetTargetData(uGuid)
      if tTargetData then
        if tTargetData.sPristineLayer then
          MrxLayerManager.MarkForRemoval(tTargetData.sPristineLayer)
        end
        if tTargetData.sStagingLayer then
          MrxLayerManager.MarkForRemoval(tTargetData.sStagingLayer)
        end
        if tTargetData.sDefenseLayer then
          MrxLayerManager.MarkForRemoval(tTargetData.sDefenseLayer)
        end
        if tTargetData.sDestroyedLayer then
          MrxLayerManager.MarkForAddition(tTargetData.sDestroyedLayer)
        end
      end
      self:_TargetComplete(uGuid)
      MrxStatsManager.JobDestroyPart(self:GetFactionId())
    end,
    fOnComplete = function()
      self:Complete()
    end,
    fOnCancel = function()
      self:Cancel()
    end
  })
end

function _GetPerTargetLayerKeys()
  return {
    "sTargetLayer",
    "sPristineLayer",
    "sStagingLayer",
    "sDefenseLayer"
  }
end

function _GetNearRadius()
  return 150
end

function _GetFarRadius()
  return 200
end

function _TargetNearby(self, uGuid)
  if Object.IsAlive(uGuid) then
    local tTargetData = self:_GetTargetData(uGuid)
    if tTargetData and tTargetData.vNearVoSequence then
      MrxVoSequence.Start(tTargetData.vNearVoSequence, false, MrxVoSequence.knPriorityBounties)
    else
      MrxTaskJob._TargetNearby(self, uGuid)
    end
  end
end
