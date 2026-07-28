inherit("MrxTutorial")

function GetMessage()
  return "[Tutorial.Alarms]"
end

function SetupCompletionCriteria(self)
  self:_CreateEvent(Event.TimerRelative, {10}, self.EndTutorial, {self, true})
end
