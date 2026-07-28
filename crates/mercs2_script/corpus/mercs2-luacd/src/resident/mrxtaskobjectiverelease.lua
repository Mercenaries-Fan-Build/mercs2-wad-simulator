inherit("MrxTaskObjectiveAction")
import("MrxUtil")
import("MrxFactionManager")
_knTgtNearbyRadius = 100

function Activated(self)
  MrxTaskObjectiveAction.Activated(self)
  self:_CreateNearbyEvent()
end

function _PrepTargets(self)
end

function _TargetActioned(self, uActionerGuid, uActioneeGuid)
  MrxTaskObjectiveAction._TargetActioned(self, uActionerGuid, uActioneeGuid)
  Human.SetState(uActioneeGuid, "Upright", "Idle")
  local tConfig = self:GetConfig()
  tConfig = tConfig.oParent:GetConfig()
  if tConfig.tMaterielScale then
    for faction, scale in pairs(tConfig.tMaterielScale) do
      local sFaction = MrxFactionManager.GetFactionTemplateName(faction)
      local uFaction = Pg.GetGuidByName(sFaction)
      if uFaction then
        Ai.AddInfraction(uActionerGuid, uFaction, 5)
        Ai.SetRelation(uFaction, uActioneeGuid, -100)
      else
        Debug.Printf("^^^^^^^^^^ No faction found???")
      end
    end
  end
end

function _GetShortDescription()
  return "[Generic.ObjectiveRelease]"
end

function _CreateNearbyEvent(self)
  self._uFarTgtFilter = ObjectFilter.Copy(self:GetTargetObjectFilter())
  ObjectFilter.RemoveObject(self._uFarTgtFilter, Player.GetAnyCharacter())
  ObjectFilter.RemoveObject(self._uFarTgtFilter, Player.GetAllCharacters())
  local uHandle = self:_CreatePersistentEvent(Event.ObjectProximity, {
    self._uFarTgtFilter,
    Player.GetLocalCharacter(),
    "<",
    _knTgtNearbyRadius
  }, self._TargetNearby, {self})
end

function _TargetNearby(self, tGuids)
  for i, uGuid in ipairs(tGuids) do
    local tConfig = self:GetConfig()
    tConfig = tConfig.oParent:GetConfig()
    if tConfig.tMaterielScale then
      for faction, scale in pairs(tConfig.tMaterielScale) do
        local sFaction = MrxFactionManager.GetFactionTemplateName(faction)
        local uFaction = Pg.GetGuidByName(sFaction)
        if uFaction then
          Ai.SetRelation(uFaction, uGuid, 0)
        else
          Debug.Printf("^^^^^^^^^^ No faction found???")
        end
      end
    end
    Human.SetState(uGuid, "Subdued", "Idle")
    Pg.RemoveContextAction(uGuid)
    local tConfig = self:GetConfig()
    local sActionLabel = "[ContextAction.ReleasePrisoner]"
    local bSuccess = Pg.AddContextAction(uGuid, sActionLabel, 2, 0, 200, 0, 2)
    ASSERT(bSuccess)
    ObjectFilter.RemoveObject(self._uFarTgtFilter, uGuid)
    self:_CreateFarawayEvent(uGuid)
  end
end

function _CreateFarawayEvent(self, uGuid)
  local uHandle = self:_CreateEvent(Event.ObjectProximity, {
    uGuid,
    Player.GetLocalCharacter(),
    ">",
    _knTgtNearbyRadius
  }, self._TargetFaraway, {self, uGuid})
end

function _TargetFaraway(self, uGuid)
  ObjectFilter.AddObject(self._uFarTgtFilter, uGuid, false)
end
