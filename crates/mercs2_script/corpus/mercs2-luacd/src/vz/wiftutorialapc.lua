inherit("MrxTutorial")

function GetMessage()
  return "[Tutorial.APC]"
end

function SetupActivationCriteria(self)
  self:_CreateEvent(Event.ObjectInSeat, {
    Player.GetLocalCharacter(),
    "APC",
    "D",
    "E"
  }, self.ActivateTutorial, {self, true})
end

function SetupCancellationCriteria(self)
  self:_CreateEvent(Event.ObjectInSeat, {
    Player.GetLocalCharacter(),
    "APC",
    "D",
    "X"
  }, self.EndTutorial, {self, false})
end
