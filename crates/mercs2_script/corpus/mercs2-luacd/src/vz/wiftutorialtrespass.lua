inherit("MrxTutorial")

function GetMessage()
  return "[Tutorial.Trespassing]"
end

function ActivateTutorial(self, bDontNetSync)
  if not self._CompleteEvent then
    MrxTutorial.ActivateTutorial(self, bDontNetSync)
  end
end

function SetupCompletionCriteria(self)
  if not self._CompleteEvent then
    self._CompleteEvent = self:_CreateEvent(Event.TimerRelative, {10}, self.EndTutorial, {self, true})
  end
end
