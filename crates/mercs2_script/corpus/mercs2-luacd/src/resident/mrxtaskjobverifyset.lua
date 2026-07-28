inherit("MrxTaskJob")
import("MrxFactionManager")
import("MrxUtil")

function _AddTarget(self, ...)
  local sType = type(arg[1])
  if sType == "table" then
    local tConfig = arg[1]
    MrxTaskJob._AddTarget(self, tConfig.sTarget, tConfig.sDefenseLayer)
    local tTargetData = self:_GetTargetData(tConfig.sTarget)
    if tTargetData then
      tTargetData.vNearVoSequence = tConfig.vNearVoSequence
      tTargetData.sStagingLayer = tConfig.sStagingLayer
      tTargetData.sDefenseLayer = tConfig.sDefenseLayer
      tTargetData.sPristineLayer = tConfig.sPristineLayer
      tTargetData.sVerifiedLayer = tConfig.sVerifiedLayer
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
    sName = "VerifySet",
    sModuleName = "MrxTaskObjectiveVerify",
    vTgtInclude = self:_GetTargetList(),
    vTgtExclude = self:_GetPartsCompletedList(),
    nPartsCompleted = nTempCompletedParts,
    nQuota = #self:_GetTargetList() + nTempCompletedParts,
    sDspShortDesc = self._sDspShortDesc or "[GurJob002.Objectives.001]",
    bDspBounty = true,
    bDspMsgUpd = false,
    bDspMsgCpl = false,
    sDspBlpPdaIcon = "icon_verify_3_mc",
    bTrackOnActivate = self._bTrackOnActivate,
    bSkipInitialNotifications = self._bSkipInitialNotifications,
    sFactionId = self._sFactionId,
    fOnActivate = function()
      self:_CreateNearbyEvent()
      MrxUtil.CallWithOptionalArgs(fCallback, tCallbackArgs)
    end,
    fOnPartComplete = function(uGuid, bKilled)
      local tTargetData = self:_GetTargetData(uGuid)
      if tTargetData then
        if tTargetData.sStagingLayer then
          MrxLayerManager.MarkForRemoval(tTargetData.sStagingLayer)
        end
        if tTargetData.sVerifiedLayer then
          MrxLayerManager.MarkForAddition(tTargetData.sVerifiedLayer)
        end
      end
      local sFanfareType = "hvtcapture"
      if bKilled then
        sFanfareType = "hvtkill"
      end
      local sFanfareText = MrxFactionManager.GetInlineIcon(self:GetFactionId()) .. " " .. self._oObjective:GetDescription(true)
      Hud.EventFanfare:Commence({sType = sFanfareType, vText = sFanfareText})
      if Net.IsServer() then
        local iFanfareType = 1
        if bKilled then
          iFanfareType = 2
        end
        local sFactionId = self:GetFactionId()
        local sDesc = self._oObjective:GetShortDescription()
        local iInlineIcon = MrxUtil.GetInlineIconIndexByName(self._oObjective:GetInlineIcon())
        local nCompleted = self._oObjective:GetProgressCompleted()
        local nQuota = self._oObjective:GetProgressQuota()
        Net.SendEvent_HVTFanfare(iFanfareType, sFactionId, sDesc, iInlineIcon, nCompleted, nQuota)
      end
      if bKilled then
        MrxRewardData.EnableCashRewardHalving(true)
      end
      self:_TargetComplete(uGuid)
      MrxRewardData.EnableCashRewardHalving(false)
    end,
    fOnComplete = function()
      self:Complete()
    end,
    fOnCancel = function()
      self:Cancel()
    end
  })
end

function _SetFactionId(self, sFactionId)
  self._sFactionId = sFactionId
end

function _GetPerTargetLayerKeys()
  return {
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

function _GetNearbyVoPlaybackMode()
  return true
end

_bPlayedVerificationVO = false

function _PlayVerificationVO(self)
  if not _bPlayedVerificationVO then
    MrxVoSequence.Start({
      "Fiona.Misc.Verification01",
      0.5,
      "Fiona.Misc.Verification02"
    }, nil, MrxVoSequence.knPriorityBounties)
    _bPlayedVerificationVO = true
  end
end

function _NearVoComplete(self)
  self._bNearVoInProgress = false
  Event.Create(Event.TimerRelative, {1}, _PlayVerificationVO, {self})
end

function _TargetNearby(self, uGuid)
  local tTargetData = self:_GetTargetData(uGuid)
  if tTargetData and tTargetData.vNearVoSequence then
    if not self._bPlayedNearVO then
      MrxVoSequence.Start(tTargetData.vNearVoSequence, false, MrxVoSequence.knPriorityBounties)
      self._bPlayedNearVO = true
    end
  else
    MrxTaskJob._TargetNearby(self, uGuid)
  end
end

function LoadAssets(self, tSaveData)
  if tSaveData and tSaveData.bPlayedVerificationVO then
    _bPlayedVerificationVO = tSaveData.bPlayedVerificationVO
  end
  MrxTaskJob.LoadAssets(self, tSaveData)
end

function SaveInstance(self)
  local tSaveData = MrxTaskJob.SaveInstance(self)
  tSaveData.bPlayedVerificationVO = _bPlayedVerificationVO
  return tSaveData
end
