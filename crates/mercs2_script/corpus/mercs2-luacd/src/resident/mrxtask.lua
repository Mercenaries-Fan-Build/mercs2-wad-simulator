import("MrxGui")
import("MrxLayerManager")
import("MrxTaskState")
import("MrxTimer")
import("MrxUtil")

function Create(mModule, self)
  self = self or {}
  setmetatable(self, {__index = mModule})
  self._tChildren = {}
  self._tConfig = {}
  return self
end

function Cleanup(self)
  if not self:IsLatent() and not self._bCleanedUp then
    Debug.Printf("Cleaning up " .. self:GetLineage())
    local oParent = self:GetParent()
    if oParent then
      oParent:_RemoveChild(self:GetName())
    end
    if self._tEvents then
      for i, uHandle in pairs(self._tEvents) do
        Event.Delete(uHandle)
      end
    end
    if self._oTimer then
      self._oTimer:Stop()
    end
    local tConfig = self:GetConfig()
    if type(tConfig.tLayers) == "table" then
      for _, sLayerName in ipairs(tConfig.tLayers) do
        MrxLayerManager.MarkForRemoval(sLayerName)
      end
    end
    local sModuleName = self:GetConfig().sModuleName
    if sModuleName then
      Debug.Printf("Dynamically removing module " .. sModuleName)
      dynamic_remove(tConfig.sModuleName)
    end
    setmetatable(self, {__index = _THIS})
    local tChildren = self:GetChildren()
    for sChildName, oChild in pairs(tChildren) do
      Debug.Printf("Attempting to clean up child " .. oChild:GetName())
      oChild:Cleanup()
    end
    self._tChildren = {}
    self._bCleanedUp = true
  else
    Debug.Printf("Not destroying task \"" .. self:GetLineage() .. "\" (Reason: " .. MrxTaskState.GetStateDisplayName(self:_GetState()) .. ")")
  end
end

function IsLiveConfigureable(self, sConfigKey)
  local tValidKeys = {
    tOnActivate = true,
    fOnActivate = true,
    tOnComplete = true,
    fOnComplete = true,
    tOnCancel = true,
    fOnCancel = true
  }
  if tValidKeys[sConfigKey] then
    return true
  end
  return false
end

function ReinterpretConfig(self)
end

function CreateChild(self, tConfig)
  local oTask = _THIS:Create()
  oTask:Configure(tConfig)
  oTask:Configure({oParent = self})
  oTask:Activate()
  return oTask
end

function Activated(self)
  if self:IsActive() then
    Debug.Printf("Activation of task " .. self:GetName() .. " FAILED; task is already activated.")
    return
  end
  self:_SetState(MrxTaskState._knActive)
  self:_IssueStateChangeCallbacks()
  local tConfig = self:GetConfig()
  if type(tConfig.tTimerParams) == "table" then
    tConfig.tTimerParams.tDoneCallbacks = tConfig.tTimerParams.tDoneCallbacks or {
      {
        self.Cancel,
        {self}
      }
    }
    self._oTimer = MrxTimer:Create(tConfig.tTimerParams)
    if not tConfig.tTimerParams.bTaskManualStart then
      self._oTimer:Start()
    end
  elseif type(tConfig.nTimeLimit) == "number" then
    self._oTimer = MrxTimer:Create({
      nStartTime = tConfig.nTimeLimit,
      tDoneCallbacks = {
        {
          self.Cancel,
          {self}
        }
      }
    })
    self._oTimer:Start()
  end
end

function Complete(self)
  if self:IsCompleted() then
    Debug.Printf("Completion of task " .. self:GetName() .. " FAILED; task is already completed.")
    return
  end
  Debug.Printf("Task \"" .. self:GetLineage() .. "\" complete")
  self:Cleanup()
  self:_SetState(MrxTaskState._knCompleted)
  self:_IssueStateChangeCallbacks()
end

function Cancel(self)
  if self:IsCancelled() then
    Debug.Printf("Cancellation of task " .. self:GetName() .. " FAILED; task is already cancelled.")
    return
  end
  Debug.Printf("Task \"" .. self:GetLineage() .. "\" cancelled")
  self:Cleanup()
  self:_SetState(MrxTaskState._knCancelled)
  self:_IssueStateChangeCallbacks()
end

function _SetState(self, nState)
  local nOldState = self:_GetState()
  ASSERT(MrxTaskState.IsValidState(nState))
  if nOldState == nState then
    return false
  end
  self._nState = nState
  Debug.Printf("_SetState \"" .. self:GetLineage() .. "\" " .. MrxTaskState.GetStateDisplayName(nState))
  local bIsCompleted = self:IsCompleted()
  local bIsCancelled = self:IsCancelled()
  if bIsCompleted or bIsCancelled then
    self:_SetChildrenState(nState)
  end
  return true
end

function _IssueStateChangeCallbacks(self)
  local bIsLatent = self:IsLatent()
  local bIsActive = self:IsActive()
  local bIsCompleted = self:IsCompleted()
  local bIsCancelled = self:IsCancelled()
  if not bIsLatent then
    local tConfig = self:GetConfig()
    local tCallbacks
    if bIsActive then
      tCallbacks = tConfig.tOnActivate
    elseif bIsCompleted then
      tCallbacks = tConfig.tOnComplete
    elseif bIsCancelled then
      tCallbacks = tConfig.tOnCancel
    end
    if tCallbacks then
      for i, tCallback in ipairs(tCallbacks) do
        MrxUtil.CallWithOptionalArgs(tCallback[1], tCallback[2])
      end
    end
    local fCallback
    if bIsActive then
      fCallback = tConfig.fOnActivate
    elseif bIsCompleted then
      fCallback = tConfig.fOnComplete
    elseif bIsCancelled then
      fCallback = tConfig.fOnCancel
    end
    if fCallback then
      fCallback()
    end
  end
end

function _SetChildrenState(self, nState)
  local tChildren = self:GetChildren()
  for sChildName, oChild in pairs(tChildren) do
    oChild:_SetState(nState)
  end
end

function _ResetState(self)
  self:_SetState(MrxTaskState._knLatent)
end

function IsLatent(self)
  return self:_GetState() == MrxTaskState._knLatent
end

function IsActive(self)
  return self:_GetState() == MrxTaskState._knActive
end

function IsCompleted(self)
  return self:_GetState() == MrxTaskState._knCompleted
end

function IsCancelled(self)
  return self:_GetState() == MrxTaskState._knCancelled
end

function _GetState(self)
  local nState
  if self._nState then
    nState = self._nState
  else
    nState = MrxTaskState._knLatent
  end
  ASSERT(nState)
  return nState
end

function Configure(self, tConfig)
  if type(tConfig) ~= "table" then
    return false
  end
  local bIsLatent = self:IsLatent()
  local bIsActive = self:IsActive()
  if bIsLatent then
    for k, v in pairs(tConfig) do
      self._tConfig[k] = v
    end
    if tConfig.oParent then
      tConfig.oParent:_AddChild(self)
    end
    return true
  elseif bIsActive then
    for k, v in pairs(tConfig) do
      if self:IsLiveConfigureable(k) then
        self._tConfig[k] = v
        Debug.Printf("Key " .. k .. " configured for active task.")
      else
        Debug.Printf("Sorry, key " .. k .. " is not configureable for an active task.")
      end
    end
    self:ReinterpretConfig()
    return true
  end
end

function GetConfig(self)
  return self._tConfig
end

function AddCallback(self, sConfigKey, fCallback, tData)
  if not sConfigKey or not fCallback then
    return false
  end
  if "table" ~= type(tData) then
    tData = {}
  end
  local bIsLatent = self:IsLatent()
  local bIsActive = self:IsActive()
  if bIsActive and not bIsLatent and not self:IsLiveConfigureable(sConfigKey) then
    Debug.Printf("Sorry, key " .. sConfigKey .. " is not configureable for an active task.")
    return false
  end
  local tConfig = self:GetConfig()
  if "table" ~= type(tConfig[sConfigKey]) then
    if nil == tConfig[sConfigKey] then
      tConfig[sConfigKey] = {}
    else
      Debug.Printf("Config key " .. sConfigKey .. " does not refer to a callback table.")
      return false
    end
  end
  table.insert(tConfig[sConfigKey], {fCallback, tData})
  return true
end

function Activate(self, tSaveData)
  self:_ResetState()
  if type(tSaveData) == "table" then
    self:_SetSaveData(MrxUtil.CopyTable(tSaveData))
  end
  local tConfig = self:GetConfig()
  if tConfig.sModuleName then
    dynamic_import(tConfig.sModuleName, self._ModuleLoaded, {self})
  else
    self:LoadAssets(self:_GetSaveData())
  end
end

function _ModuleLoaded(self, mModule)
  ASSERT(mModule)
  local tConfig = self:GetConfig()
  if tConfig.sModuleName then
    Debug.Printf("Dynamically imported module " .. tConfig.sModuleName)
  end
  setmetatable(self, {__index = mModule})
  self._tEvents = {}
  self:PreLoadAssets()
  self:LoadAssets(self:_GetSaveData())
end

function PreLoadAssets(self)
end

function LoadAssets(self, tSaveData)
  local tConfig = self:GetConfig()
  if type(tConfig.tLayers) == "table" then
    local bOldStaticLayers = Pg.GetLoadingStaticLayers()
    Pg.LoadingStaticLayers(false)
    MrxLayerManager.Add(tConfig.tLayers, self.AssetsLoaded, {self})
    Pg.LoadingStaticLayers(bOldStaticLayers)
  else
    self:AssetsLoaded()
  end
end

function AssetsLoaded(self)
  self:_IssueAssetsLoadedCallbacks()
  self:Activated()
end

function _IssueAssetsLoadedCallbacks(self)
  local tConfig = self:GetConfig()
  if tConfig.tOnAssetsLoaded then
    MrxUtil.ProcessCallbackTable(tConfig.tOnAssetsLoaded)
  end
  if tConfig.fOnAssetsLoaded then
    MrxUtil.CallWithOptionalArgs(tConfig.fOnAssetsLoaded)
  end
end

function _AddChild(self, oChild)
  local tChildren = self:GetChildren()
  local sChildName = oChild:GetName()
  Debug.Printf("Adding " .. sChildName .. " as a child of " .. self:GetName())
  ASSERT(tChildren[sChildName] == nil)
  tChildren[sChildName] = oChild
end

function _RemoveChild(self, sChildName)
  local tChildren = self:GetChildren()
  tChildren[sChildName] = nil
end

function GetChild(self, sChildName)
  local tChildren = self:GetChildren()
  return tChildren[sChildName]
end

function _AddChildren(self, tChildren)
  for _, oChild in ipairs(tChildren) do
    self:_AddChild(oChild)
  end
end

function GetChildren(self)
  return self._tChildren
end

function GetName(self)
  return self:GetConfig().sName
end

function GetTitle(self)
  return self:GetConfig().sTitle
end

function GetParent(self)
  return self:GetConfig().oParent
end

function GetLineage(self)
  local oChild = self
  local oParent = oChild:GetParent()
  local sLineage = oChild:GetName()
  while oParent do
    sLineage = oParent:GetName() .. "." .. sLineage
    oChild = oParent
    oParent = oChild:GetParent()
  end
  return sLineage
end

function SaveInstance(self)
  Debug.Printf("Saving " .. self:GetLineage())
  local tSaveData
  local tOriginalSaveData = self:_GetSaveData()
  if tOriginalSaveData then
    tSaveData = MrxUtil.CopyTable(tOriginalSaveData)
  else
    tSaveData = {}
  end
  tSaveData.nState = self:_GetState()
  return tSaveData
end

function _GetSaveData(self)
  return self._tSaveData
end

function _SetSaveData(self, tSaveData)
  self._tSaveData = tSaveData
end

function _GetRewards(self)
  return {nCash = 0, nFuel = 0}
end

function _CanCompleteViaCheatMenu()
  return true
end

function _CreateEvent(self, nEventId, tEventArgs, fCallback, tCallbackArgs)
  local uHandle = Event.Create(nEventId, tEventArgs, fCallback, tCallbackArgs)
  table.insert(self._tEvents, uHandle)
  return uHandle
end

function _CreatePersistentEvent(self, nEventId, tEventArgs, fCallback, tCallbackArgs)
  local uHandle = Event.CreatePersistent(nEventId, tEventArgs, fCallback, tCallbackArgs)
  table.insert(self._tEvents, uHandle)
  return uHandle
end

function GetStub(self)
  return self
end

function _SetTask(self, oTask)
  self._oTask = oTask
end

function GetTask(self)
  return self
end
