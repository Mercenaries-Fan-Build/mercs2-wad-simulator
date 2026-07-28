inherit("MrxTutorial")

function GetMessage()
  return "[Tutorial.NoFuel]"
end

function SetupCompletionCriteria(self)
  self:_CreateEvent(Event.TimerRelative, {10}, self.EndTutorial, {self, true})
end
