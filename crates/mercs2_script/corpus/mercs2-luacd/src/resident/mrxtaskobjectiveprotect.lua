inherit("MrxTaskObjective")

function Activated(self)
  MrxTaskObjective.Activated(self)
  self._tEvents.uDeathEvent = Event.CreatePersistent(Event.ObjectDeath, {
    self._uTgtObjFilter
  }, _TargetDestroyed, {self})
  ASSERT(self._tEvents.uDeathEvent)
end

function _TargetDestroyed(self, uGuid, uCause, uKiller)
  local tConfig = self:GetConfig()
  if tConfig.bHeroOnly and type(uGuid) == "userdata" then
    for i, player in ipairs(Player.GetAllPlayers()) do
      local hero = Player.GetCharacter(player)
      if hero == uKiller then
        self:RemoveTarget(uGuid)
        self:CancelPart(uGuid)
      end
    end
  elseif type(uGuid) == "userdata" then
    self:RemoveTarget(uGuid)
    self:CancelPart(uGuid)
  end
end

function _GetShortDescription()
  return "[Generic.ObjectiveProtect]"
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

function _GetTargetRadarIcon()
  return "objective_defend"
end

function GetInlineIcon(self)
  local tConfig = self:GetConfig()
  if tConfig.bOptional then
    return "[objdefend2]"
  else
    return "[objdefend]"
  end
end

function _GetTargetPdaIcon(bOptional)
  if bOptional then
    return "icon_defend_2_mc"
  else
    return "icon_defend_1_mc"
  end
end

function _GetTargetGameSpaceIcon()
  return "HUD_objective_defend"
end
