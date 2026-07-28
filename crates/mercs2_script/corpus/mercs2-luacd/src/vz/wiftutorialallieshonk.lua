inherit("MrxTutorial")
import("MrxTutorialManager")
import("MrxFactionManager")
tFactions = {
  "OC",
  "Pirate",
  "Guerilla",
  "Allied",
  "China"
}
tPositions = {
  {
    2463.2,
    1,
    1492.27
  },
  {
    2140.88,
    1,
    2600.9
  },
  {
    1582.65,
    1,
    -2408.16
  },
  {
    287.89,
    1,
    153.89
  },
  {
    -2390.02,
    1,
    1128.63
  }
}

function GetMessage()
  return "[Tutorial.VehicleHorn.Attract]"
end

function SetupActivationCriteria(self)
  self:OnExitSeat()
end

function OnEnterSeat(self)
  self:_CreateEvent(Event.ObjectInSeat, {
    Player.GetLocalCharacter(),
    0,
    "a",
    "x"
  }, OnExitSeat, {self})
  for uFaction, _ in pairs(tFactions) do
    self:CreateEvent(uFaction)
  end
end

function OnExitSeat(self)
  self:DestroyEvents()
  self:_CreateEvent(Event.ObjectInSeat, {
    Player.GetLocalCharacter(),
    0,
    "d",
    "e"
  }, OnEnterSeat, {self})
end

function CreateEvent(self, uFaction)
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetLocalCharacter(),
    tPositions[uFaction][1],
    tPositions[uFaction][2],
    tPositions[uFaction][3],
    "<",
    500,
    false,
    true
  }, WithinRegion, {self, uFaction})
end

function WithinRegion(self, uFaction)
  local fRelation = Ai.GetRelation(Pg.GetGuidByName(tFactions[uFaction]), Pg.GetGuidByName("PMC"))
  if 0 < fRelation then
    local bSuccess = self:ActivateTutorial(true)
    Debug.Printf("Tutorial - Activate tutorial : ", bSuccess)
  end
end
