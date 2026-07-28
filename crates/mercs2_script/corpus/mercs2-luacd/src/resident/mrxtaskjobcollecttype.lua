inherit("MrxTaskJob")
import("MrxPmc")
import("MrxStatsManager")

function _SetLabelFilter(self, sLabelFilter)
  self._sLabelFilter = sLabelFilter
end

function _SetQuota(self, nQuota)
  self._nQuota = nQuota
end

function _Go(self)
  self:_ExcludeCompletedTargets()
  self:_AddToPda()
  local tSaveData = self._tSaveData
  local nCollected = 0
  local tTargetsToExclude = {}
  if tSaveData and tSaveData.tCollected then
    for n, uGuid in ipairs(tSaveData.tCollected) do
      self._tCollectedItems[uGuid] = true
      _DisableCollectable(uGuid)
      nCollected = nCollected + 1
      table.insert(tTargetsToExclude, uGuid)
    end
  end
  self._oObjective = self:CreateChild({
    sName = "CollectType",
    sModuleName = "MrxTaskObjectiveDestroy",
    sTgtLabelFilter = self._sLabelFilter,
    sDspShortDesc = self._sDspShortDesc,
    bDspCollectible = true,
    bSkipInitialNotifications = self._bSkipInitialNotifications,
    nQuota = self._nQuota,
    vTgtExclude = tTargetsToExclude,
    fOnPartComplete = function(uGuid)
      MrxStatsManager.CompleteToolboxPart()
      local nReward = Object.GetCashValue(uGuid)
      if nReward then
        MrxPmc.AddCashQty(nReward, true, "[Generic.Collectibles]")
      end
      self._oObjective._tConfig.sDspShortDesc = Object.GetLocalizedName(uGuid)
      self:_RecordCollectedItem(uGuid)
      self:_TargetComplete(uGuid)
    end,
    fOnComplete = function()
      self:Complete()
    end,
    fOnCancel = function()
      self:Cancel()
    end,
    nPartsCompleted = nCollected
  })
end

function _DisableCollectable(uGuid)
  Object.AddLabel(uGuid, "CollectableInvalidated")
end

function _SetCollectName(self, sCollectName)
  self._sCollectName = sCollectName
end

function _RecordCollectedItem(self, uGuid)
  if uGuid then
    self._tCollectedItems[uGuid] = true
  end
end

function LoadAssets(self, tSaveData)
  self._tCollectedItems = {}
  MrxTaskJob.LoadAssets(self, tSaveData)
  if tSaveData then
    for uGuid, bCollected in pairs(tSaveData.tCollected) do
      if bCollected then
        _DisableCollectable(uGuid)
      end
    end
  end
end

function SaveInstance(self)
  local tSaveData = MrxTaskJob.SaveInstance(self)
  tSaveData.tCollected = {}
  for uGuid, bCollected in pairs(self._tCollectedItems) do
    if bCollected then
      table.insert(tSaveData.tCollected, uGuid)
    end
  end
  return tSaveData
end
