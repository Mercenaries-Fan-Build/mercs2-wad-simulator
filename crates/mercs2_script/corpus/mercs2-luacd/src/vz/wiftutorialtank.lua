inherit("MrxTutorial")

function GetMessage()
  return "[Tutorial.Tank]"
end

function SetupActivationCriteria(self)
  self:_CreateEvent(Event.ObjectInSeat, {
    Player.GetLocalCharacter(),
    "Tank && !APC",
    "D",
    "E"
  }, self.ActivateTutorial, {self, true})
end

function SetupCancellationCriteria(self)
  self:_CreateEvent(Event.ObjectInSeat, {
    Player.GetLocalCharacter(),
    "Tank",
    "D",
    "X"
  }, self.EndTutorial, {self, false})
end
