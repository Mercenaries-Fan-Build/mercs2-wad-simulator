inherit("MrxTaskObjective")
import("MrxUtil")

function Activated(self)
  MrxTaskObjective.Activated(self)
  self:_PrepTargets()
  self._tEvents.uActionEvent = Event.CreatePersistent(Event.ContextAction, {
    0,
    self._uTgtObjFilter
  }, self._TargetActioned, {self})
  ASSERT(self._tEvents.uActionEvent)
  self._tEvents.uDeathEvent = Event.CreatePersistent(Event.ObjectDeath, {
    self._uTgtObjFilter
  }, self._TargetDestroyed, {self})
  ASSERT(self._tEvents.uDeathEvent)
end

function _PrepTargets(self)
  local tConfig = self:GetConfig()
  local sActionLabel = MrxUtil.SetDefault(tConfig.sActionLabel, "[ContextAction.Talk]")
  local tGuids = ObjectFilter.GetObjects(self._uTgtObjFilter, false)
  for i, uGuid in ipairs(tGuids) do
    local bSuccess = Pg.AddContextAction(uGuid, sActionLabel, 2, 0, 200, 0, 2)
    ASSERT(bSuccess)
  end
end

function _TargetActioned(self, uActionerGuid, uActioneeGuid)
  local bSuccess = Pg.RemoveContextAction(uActioneeGuid)
  ASSERT(bSuccess)
  if type(uActioneeGuid) == "userdata" then
    self:RemoveTarget(uActioneeGuid)
  end
  self:CompletePart(uActionerGuid, uActioneeGuid)
end

function _TargetDestroyed(self, uGuid)
  local bSuccess = Pg.RemoveContextAction(uGuid)
  ASSERT(bSuccess)
  if type(uGuid) == "userdata" then
    self:RemoveTarget(uGuid)
  end
  self:CancelPart()
end

function Cleanup(self)
  local tGuids = ObjectFilter.GetObjects(self._uTgtObjFilter, false)
  for i, uGuid in ipairs(tGuids) do
    Pg.RemoveContextAction(uGuid)
  end
  MrxTaskObjective.Cleanup(self)
end

function _GetShortDescription()
  return "[Generic.ObjectiveAction]"
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

function _IsValidTarget(uGuid)
  local anyPlayer = Player.GetAnyCharacter()
  local allPlayers = Player.GetAllCharacters()
  if anyPlayer == uGuid then
    return true
  end
  if allPlayers == uGuid then
    return true
  end
  return Object.IsAlive(uGuid)
end
