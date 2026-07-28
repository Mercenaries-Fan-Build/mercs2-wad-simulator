inherit("MrxTaskObjective")
import("MrxOutpostManager")

function Activated(self)
  MrxTaskObjective.Activated(self)
  local tConfig = self:GetConfig()
  local uOutpostBldg = tConfig.uOutpostBldg
  if uOutpostBldg then
    MrxOutpostManager.RegisterOutpostEvent(uOutpostBldg, self._HandleOutpostStatusChange, {self})
  end
end

function Cleanup(self)
  local tConfig = self:GetConfig()
  local uOutpostBldg = tConfig.uOutpostBldg
  if uOutpostBldg then
    MrxOutpostManager.UnregisterOutpost(uOutpostBldg)
  end
  MrxTaskObjective.Cleanup(self)
end

function _HandleOutpostStatusChange(self, uOutpost, nStatus)
  if nStatus == MrxOutpostManager.knStatusCaptured then
    self:RemoveTarget(uOutpost)
    self:CompletePart(uOutpost)
  elseif nStatus == MrxOutpostManager.knStatusDestroyed then
    self:RemoveTarget(uOutpost)
    self:CancelPart(uOutpost)
  end
end

function _GetShortDescription()
  return "[Generic.ObjectiveOutpost]"
end

function GetInlineIcon(self)
  local tConfig = self:GetConfig()
  if tConfig.bOptional then
    return "[objoutpost2]"
  else
    return "[objoutpost]"
  end
end

function _GetTargetRadarIcon()
  return "objective_outpost"
end

function _GetTargetPdaIcon(bOptional)
  if bOptional then
    return "icon_outpost_2_mc"
  else
    return "icon_outpost_1_mc"
  end
end

function _GetTargetGameSpaceIcon()
  return "HUD_objective_outpost"
end
