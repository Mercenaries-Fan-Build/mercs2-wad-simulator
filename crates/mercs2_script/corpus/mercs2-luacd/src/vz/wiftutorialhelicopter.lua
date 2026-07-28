inherit("MrxTutorial")

function GetMessage()
  return "[Tutorial.Helicopter]"
end

function SetupActivationCriteria(self)
  self:_CreateEvent(Event.ObjectInSeat, {
    Player.GetLocalCharacter(),
    "helicopter",
    "D",
    "E"
  }, self.ActivateTutorial, {self, true})
end

function SetupCancellationCriteria(self)
  self:_CreateEvent(Event.ObjectInSeat, {
    Player.GetLocalCharacter(),
    "helicopter",
    "D",
    "X"
  }, self.EndTutorial, {self, false})
end
