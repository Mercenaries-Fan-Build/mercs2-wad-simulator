import("MrxTutorialManager")

function Create(mModule, self)
  self = self or {}
  setmetatable(self, {__index = mModule})
  self._tEvents = {}
  return self
end

function DestroyEvents(self)
  if self._tEvents then
    for i, uHandle in pairs(self._tEvents) do
      Event.Delete(uHandle)
    end
    self._tEvents = {}
  end
end

function GetName(self)
  return self.sName
end

function ActivateTutorial(self, bDontNetSync)
  local bSuccess = MrxTutorialManager.SetCurrentTutorial(self, bDontNetSync)
  self:DestroyEvents()
  if bSuccess then
    self:SetupCompletionCriteria()
    self:SetupCancellationCriteria()
  else
    self:SetupActivationCriteria()
  end
  return bSuccess
end

function EndTutorial(self, bComplete)
  local bSuccess = MrxTutorialManager.HideCurrentTutorial(self, bComplete)
  if bSuccess then
    self:DestroyEvents()
    if bComplete then
      MrxTutorialManager.DestroyTutorial(self)
    else
      self:SetupActivationCriteria()
    end
  end
end

function SetupActivationCriteria(self)
end

function SetupCompletionCriteria(self)
  self:_CreateEvent(Event.TimerRelative, {20}, self.EndTutorial, {self, true})
end

function SetupCancellationCriteria(self)
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
