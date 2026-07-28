inherit("MrxTaskJob")

function _SetLabelFilter(self, sLabelFilter)
  self._sLabelFilter = sLabelFilter
end

function _SetQuota(self, nQuota)
  self._nQuota = nQuota
end

function _SetMessageDisplay(self, bDspMsg)
  self._bDspMsg = bDspMsg
end

function _Go(self, fCallback, tCallbackArgs)
  self:_ExcludeCompletedTargets()
  self:_AddToPda()
  local nTempCompletedParts = 0
  if self._nTargetsComplete then
    nTempCompletedParts = self._nTargetsComplete
  end
  self._oObjective = self:CreateChild({
    sName = "DestroyType",
    sModuleName = "MrxTaskObjectiveDestroy",
    sTgtLabelFilter = self._sLabelFilter,
    nPartsCompleted = nTempCompletedParts,
    bDspMsg = self._bDspMsg,
    sDspShortDesc = self._sDspShortDesc or "[Generic.StandingBounty]",
    bDspBounty = true,
    bHeroOnly = self._bHeroOnly,
    bSkipInitialNotifications = self._bSkipInitialNotifications,
    nQuota = self._nQuota,
    bDspMsgUpd = false,
    fOnActivate = function()
      MrxUtil.CallWithOptionalArgs(fCallback, tCallbackArgs)
    end,
    fOnPartComplete = function(uGuid)
      self:_TargetComplete(uGuid)
    end,
    fOnComplete = function()
      self:Complete()
    end,
    fOnCancel = function()
      self:Cancel()
    end
  })
end

function _SetHeroOnly(self, bHeroOnly)
  self._bHeroOnly = bHeroOnly
end

function _GetAutosaveMode()
  return false
end
