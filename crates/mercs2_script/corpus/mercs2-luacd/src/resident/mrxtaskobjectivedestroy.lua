inherit("MrxTaskObjective")

function Activated(self)
  MrxTaskObjective.Activated(self)
  self._tEvents.uDeathEvent = Event.CreatePersistent(Event.ObjectDeath, {
    self._uTgtObjFilter
  }, _TargetDestroyed, {self})
  self._tEvents.uClientKill = Event.CreatePersistent(Event.ScriptEvent, {
    "ClientKill",
    function(tData)
      return (ObjectFilter.Eval(self._uTgtObjFilter, tData[1]))
    end
  }, function(tData)
    self:RemoveTarget(tData[1])
    self:CompletePart(tData[1])
  end)
  ASSERT(self._tEvents.uDeathEvent)
end

function _TargetDestroyed(self, uGuid, uCause, uKiller)
  local tConfig = self:GetConfig()
  if tConfig.bHeroOnly and type(uGuid) == "userdata" then
    for i, player in ipairs(Player.GetAllPlayers()) do
      local hero = Player.GetCharacter(player)
      if hero == uKiller then
        self:RemoveTarget(uGuid)
        self:CompletePart(uGuid)
      end
    end
  elseif type(uGuid) == "userdata" then
    self:RemoveTarget(uGuid)
    self:CompletePart(uGuid)
  end
end

function _GetShortDescription()
  return "[Generic.ObjectiveDestroy]"
end

function GetInlineIcon(self)
  local tConfig = self:GetConfig()
  if tConfig.bOptional then
    return "[objdestroy2]"
  else
    return "[objdestroy]"
  end
end

function _GetTargetRadarIcon()
  return "objective_destroy"
end

function _GetTargetPdaIcon(bOptional)
  if bOptional then
    return "icon_destroy_2_mc"
  else
    return "icon_destroy_1_mc"
  end
end

function _GetTargetGameSpaceIcon()
  return "HUD_objective_destroy"
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
