inherit("MrxTutorial")

function GetMessage()
  return "[Tutorial.C4Switch]"
end

function SetupActivationCriteria(self)
  local uPlayer = Player.GetLocalCharacter()
  self:_CreatePersistentEvent(Event.WeaponEvent, {
    uPlayer,
    "Pickup",
    "C4 Pickup"
  }, ActivateTutorial, {self, true})
end

function SetupCompletionCriteria(self)
  local uPlayer = Player.GetLocalCharacter()
  self:_CreateEvent(Event.WeaponEvent, {
    uPlayer,
    "Equip",
    "c4"
  }, self.EndTutorial, {self, true})
end
