inherit("MrxTaskObjective")

function Activated(self, tConfig)
  MrxTaskObjective.Activated(self)
  local tConfig = self:GetConfig()
  tConfig.uPlayer = MrxUtil.SetDefault(tConfig.uPlayer, Player.GetAnyCharacter())
  local tVehicles = ObjectFilter.GetObjects(self._uTgtObjFilter, false)
  for i, uVehicle in pairs(tVehicles) do
    self:_SetupEvents(uVehicle)
  end
end

function Cleanup(self)
  for i, tTargetData in pairs(self._tTargets) do
    self:_CleanupTargetEvents(tTargetData)
  end
  MrxTaskObjective.Cleanup(self)
end

function _SetupEvents(self, uGuid)
  local tTargetData = self._tTargets[uGuid]
  tTargetData.tEvents = {}
  tTargetData.tEvents.eDeathEvent = Event.Create(Event.ObjectDeath, {uGuid}, _OnStatusChange, {self, "destroyed"})
  local tConfig = self:GetConfig()
  local sSeat = "d"
  if tConfig.bUseAnySeat then
    sSeat = "a"
  end
  if tConfig.uPlayer == Player.GetAllCharacters() then
    self._bUseAllChars = true
    tTargetData.tEvents.eObjectInSeat = Event.CreatePersistent(Event.ObjectInSeat, {
      Player.GetAnyCharacter(),
      uGuid,
      "a",
      "e"
    }, _TargetEntered, {self})
  else
    tTargetData.tEvents.eObjectInSeat = Event.Create(Event.ObjectInSeat, {
      tConfig.uPlayer,
      uGuid,
      sSeat,
      "ei"
    }, _TargetEntered, {self})
  end
end

function _CleanupTargetEvents(self, tTargetData)
  if tTargetData == nil then
    Debug.Printf(" ??? attempting to clean nil target data ")
    return
  end
  if type(tTargetData.tEvents) == "table" then
    for j, e in pairs(tTargetData.tEvents) do
      Event.Delete(e)
    end
  end
  tTargetData.tEvents = nil
end

function _OnStatusChange(self, sStatusType, uGuid)
  local tConfig = self:GetConfig()
  if tConfig.fStatusChangeCallback then
    if type(tConfig.tStatusChangeCallbackData) == "table" then
      tConfig.fStatusChangeCallback(unpack(tConfig.tStatusChangeCallbackData), uGuid, sStatusType)
    else
      tConfig.fStatusChangeCallback(uGuid, sStatusType)
    end
  end
  self:_CleanupTargetEvents(self._tTargets[uGuid])
  self:_SetTargetStatus(uGuid, false)
  self:CancelPart()
end

function _TargetEntered(self, uChar, uVehicle)
  if self._bUseAllChars then
    for i, uPlayer in ipairs(Player.GetAllPlayers()) do
      if Player.GetControlledObject(uPlayer) ~= uVehicle then
        return false
      end
    end
  end
  self:_CleanupTargetEvents(self._tTargets[uVehicle])
  if type(uVehicle) == "userdata" then
    self:RemoveTarget(uVehicle)
  end
  self:CompletePart(uChar, uVehicle)
end

function _GetShortDescription(self)
  local tConfig = self:GetConfig()
  local tVehicles = ObjectFilter.GetObjects(self._uTgtObjFilter, false)
  if tVehicles[1] then
    sName = Object.GetLocalizedName(tVehicles[1])
  end
  if sName then
    if tVehicles[1] and Object.HasLabel(tVehicles[1], "helicopter") then
      return "[ContextAction.PilotVehicleName:" .. tostring(sName) .. "]"
    else
      return "[ContextAction.EnterVehicleName:" .. tostring(sName) .. "]"
    end
  else
    return "[Generic.ObjectiveEnterVehicle]"
  end
end

function _GetTargetRadarIcon()
  return "objective_action"
end

function _GetTargetPdaIcon(bOptional)
  if bOptional then
    return "icon_action_2_mc"
  else
    return "icon_action_1_mc"
  end
end

function _GetTargetGameSpaceIcon()
  return "HUD_objective_action"
end
