inherit("MrxTutorial")

function GetMessage()
  if Gui.ControllerInUse and Gui.ControllerInUse() then
    return "[Tutorial.WheeledVehicleBasic]"
  else
    return "[SHELL.PCShell.Tutorial_WheeledVehicleBasic_PC]"
  end
end

function SetupActivationCriteria(self)
  self._oActivate1 = self:_CreateEvent(Event.ObjectInSeat, {
    Player.GetLocalCharacter(),
    "Car",
    "D",
    "E"
  }, self.ActivateTutorial2, {self})
  self._oActivate1 = self:_CreateEvent(Event.ObjectInSeat, {
    Player.GetLocalCharacter(),
    "Car",
    "D",
    "I"
  }, self.ActivateTutorial2, {self})
end

function ActivateTutorial2(self)
  if self._oActivate1 then
    Event.Delete(self._oActivate1)
  end
  if self._oActivate2 then
    Event.Delete(self._oActivate1)
  end
  self:ActivateTutorial(true)
end

function SetupCancellationCriteria(self)
  self:_CreateEvent(Event.ObjectInSeat, {
    Player.GetLocalCharacter(),
    "Car",
    "D",
    "X"
  }, self.EndTutorial, {self, false})
end
