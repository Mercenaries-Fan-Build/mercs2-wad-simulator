inherit("MrxTutorial")

function GetMessage()
  return "[Tutorial.SatelliteInterrupted]"
end

function SetupActivationCriteria(self)
  self:_CreateEvent(Event.ScriptEvent, {
    "Satellite Targetting Start",
    function()
      return true
    end
  }, SetupNextActivationCriteria, {self})
end

function SetupNextActivationCriteria(self)
  self._nPlayerHealth = Object.GetHealth(Player.GetLocalCharacter())
  self:_CreateEvent(Event.ScriptEvent, {
    "Satellite Targetting Cancelled",
    function()
      return true
    end
  }, ActivateTutorial2, {self})
end

function ActivateTutorial2(self)
  if self._nPlayerHealth > Object.GetHealth(Player.GetLocalCharacter()) then
    self:ActivateTutorial(true)
  else
    self:SetupActivationCriteria()
  end
end

function SetupCompletionCriteria(self)
  self:_CreateEvent(Event.TimerRelative, {10}, self.EndTutorial, {self, true})
end
