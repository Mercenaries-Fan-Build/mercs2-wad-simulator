import("MrxUtil")
import("MrxHq")
import("WifHqData")
import("MrxFactionManager")
_tHqs = {}
_tHqEvents = {}

function GetHq(sHqName)
  if not sHqName or not _tHqs[sHqName] then
    Debug.Printf("HQ " .. tostring(sHqName) .. " could not be retrieved")
    return nil
  end
  return _tHqs[sHqName]
end

function AddStarter(sHqName, tStarter)
  local tHq = GetHq(sHqName)
  tHq = tHq or UnlockHq(sHqName)
  tHq:AddStarter(tStarter)
end

function RemoveStarter(sHqName, tStarter)
  local tHq = GetHq(sHqName)
  if not tHq then
    return
  end
  tHq:RemoveStarter(tStarter)
end

function UnlockHq(sHqName)
  Debug.Printf("Attempting to unlock HQ " .. sHqName)
  local tHq = _tHqs[sHqName]
  if not tHq then
    Debug.Printf("Loading data for HQ " .. sHqName)
    local tHqData = WifHqData.GetHqConfigFromId(sHqName)
    if not tHqData then
      Debug.Printf("Failed to find data for HQ " .. sName)
      return
    end
    _tHqs[sHqName] = MrxHq:Create(tHqData)
    tHq = _tHqs[sHqName]
    tHq:SetName(sHqName)
  end
  if not tHq:IsLocked() then
    Debug.Printf("HQ " .. sHqName .. " already unlocked")
    return
  end
  Debug.Printf("HQ " .. sHqName .. " setup complete")
  tHq:SetLock(false)
  tHq:RefreshUiDisplay()
  if tHq:GetRespawn() == nil then
    SetHqRespawn(sHqName, true)
  end
  return tHq
end

function LockHq(sHqName)
  Debug.Printf("Attempting to lock HQ " .. sHqName)
  local tHq = _tHqs[sHqName]
  if not tHq then
    return
  end
  if tHq:IsLocked() then
    Debug.Printf("HQ " .. sHqName .. " already locked")
    return
  end
  tHq:SetLock(true)
  tHq:RefreshUiDisplay()
  if _tHqEvents[sHqName] then
    Event.Delete(_tHqEvents[sHqName])
    _tHqEvents[sHqName] = nil
  end
end

function LockAllHq()
  for sHqName, tHq in pairs(_tHqs) do
    if not tHq:IsLocked() then
      Debug.Printf("Globally Locking HQ " .. sHqName)
      tHq.bGloballyLocked = true
      LockHq(sHqName)
    end
  end
end

function UnlockAllHq()
  for sHqName, tHq in pairs(_tHqs) do
    if tHq.bGloballyLocked then
      Debug.Printf("Globally Unlocking HQ " .. sHqName)
      tHq.bGloballyLocked = nil
      UnlockHq(sHqName)
    end
  end
end

function _CreateDeathEvent(sHqName, uHqGuid)
  local tHq = _tHqs[sHqName]
  if not tHq then
    return
  end
  if tHq.bWatchBuildingHealth then
    Debug.Printf("Creating HealthEvent for " .. sHqName .. "(" .. tostring(uHqGuid) .. ")")
    _tHqEvents[uHqGuid] = Event.Create(Event.ObjectHealth, {
      uHqGuid,
      "*",
      "<=",
      1
    }, _OnHqDeath, {sHqName, uHqGuid})
  else
    Debug.Printf("Creating DeathEvent for " .. sHqName .. "(" .. tostring(uHqGuid) .. ")")
    _tHqEvents[uHqGuid] = Event.Create(Event.ObjectDeath, {uHqGuid}, _OnHqDeath, {sHqName})
  end
end

function _SetupRespawn(bEnable, sHqName, uHqGuid)
  if bEnable then
    if Object.IsAlive(uHqGuid) then
      _tHqEvents[uHqGuid] = _CreateDeathEvent(sHqName, uHqGuid)
    else
      _OnHqDeath(sHqName, uHqGuid)
    end
  elseif not Object.IsAlive(uHqGuid) and _tHqEvents[uHqGuid] then
    Event.Delete(_tHqEvents[uHqGuid])
    _tHqEvents[uHqGuid] = nil
  end
end

function SetHqRespawn(sHqName, bEnable)
  local tHq = _tHqs[sHqName]
  if not tHq then
    return
  end
  if tHq:GetRespawn() == bEnable then
    return
  end
  local uHqGuid
  if not tHq.vBuildingName then
    Debug.Printf("@@@@@@@@@@@@@ HQ " .. sHqName .. " does not have an associated building!")
    return
  end
  Debug.Printf(sHqName .. " Respawn set to " .. tostring(bEnable))
  tHq:SetRespawn(bEnable)
  if type(tHq.vBuildingName) == "string" then
    tHq.vBuildingName = {
      tHq.vBuildingName
    }
  end
  for _, sBuildingName in ipairs(tHq.vBuildingName) do
    uHqGuid = Pg.GetGuidByName(sBuildingName)
    if not uHqGuid then
      Debug.Printf("@@@@@@@@@@@@@ HQ " .. sHqName .. "'s building " .. sBuildingName .. " is out-of-date!")
    else
      _SetupRespawn(bEnable, sHqName, uHqGuid)
    end
  end
end

function _SetHq(sHqName, bEnable)
  local tHq = _tHqs[sHqName]
  if not tHq then
    return
  end
  tHq:SetLock(not bEnable)
  tHq:RefreshUiDisplay()
end

function _OnHqDeath(sHqName, uHqGuid, uKilledByTemplate, uCulprit)
  local tHq = _tHqs[sHqName]
  if not tHq then
    return
  end
  Debug.Printf(sHqName .. "(" .. tostring(uHqGuid) .. ")" .. " has been destroyed!")
  _tHqEvents[uHqGuid] = nil
  _SetHq(sHqName, false)
  if type(uCulprit) ~= "userdata" or Object.IsPlayerControlled(uCulprit) then
    local sFaction = tHq:GetFaction()
    if sFaction then
      MrxFactionManager.SetRelation(sFaction, "Pmc", -100)
    end
  end
  local tHq = GetHq(sHqName)
  if tHq and not tHq:GetRespawn() then
    Debug.Printf(sHqName .. " set to not respawn, canceling respawn process...!")
    return
  end
  _tHqEvents[uHqGuid] = Event.Create(Event.ObjectHibernation, {uHqGuid, "s"}, _OnHqHibernation, {sHqName, uHqGuid})
end

function _OnHqHibernation(sHqName, uHqGuid)
  local tHq = _tHqs[sHqName]
  if not tHq then
    return
  end
  Debug.Printf("Reviving " .. sHqName .. "(" .. tostring(uHqGuid) .. ")")
  _tHqEvents[uHqGuid] = nil
  Object.Revive(uHqGuid)
  _SetHq(sHqName, true)
  _tHqEvents[uHqGuid] = _CreateDeathEvent(sHqName, uHqGuid)
end

function IsInside()
  return _bInside
end

function SetInside(bInside)
  _bInside = bInside
end

function SetUnloadCallback(fCallback, tCallbackArgs)
  _fUnloadCallback = fCallback
  _tUnloadCallbackArgs = tCallbackArgs
end

function GetUnloadCallback()
  return _fUnloadCallback, _tUnloadCallbackArgs
end
