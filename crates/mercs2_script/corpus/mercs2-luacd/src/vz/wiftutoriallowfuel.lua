inherit("MrxTutorial")

function GetMessage()
  return "[Tutorial.LowFuel]"
end

function SetupCompletionCriteria(self)
  self:_CreateEvent(Event.TimerRelative, {10}, self.EndTutorial, {self, true})
end
