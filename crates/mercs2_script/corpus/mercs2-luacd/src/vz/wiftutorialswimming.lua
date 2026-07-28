inherit("MrxTutorial")

function GetMessage()
  return "[Tutorial.Swimming]"
end

function SetupActivationCriteria(self)
  local uPlayer = Player.GetLocalCharacter()
  self:_CreateEvent(Event.HumanStateTransition, {
    uPlayer,
    "*",
    "Swim.*"
  }, self.ActivateTutorial, {self, true})
end

function SetupCancellationCriteria(self)
  local uPlayer = Player.GetLocalCharacter()
  self:_CreateEvent(Event.HumanStateTransition, {
    uPlayer,
    "Swim.*",
    "Upright.*"
  }, self.EndTutorial, {self, false})
  self:_CreateEvent(Event.HumanStateTransition, {
    uPlayer,
    "Swim.*",
    "InVehicle.*"
  }, self.EndTutorial, {self, false})
end
