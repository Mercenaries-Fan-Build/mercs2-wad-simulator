inherit("MrxTutorial")

function GetMessage()
  return "[Fiona.Misc.Revive01]"
end

function SetupCompletionCriteria(self)
  self:_CreateEvent(Event.TimerRelative, {10}, self.EndTutorial, {self, true})
end
