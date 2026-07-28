inherit("MrxTutorial")

function GetMessage()
  if Gui.ControllerInUse and Gui.ControllerInUse() then
    return "[Tutorial.Boat]"
  else
    return "[SHELL.PCShell.Tutorial_Boat_PC]"
  end
end

function SetupActivationCriteria(self)
  self:_CreateEvent(Event.ObjectInSeat, {
    Player.GetLocalCharacter(),
    "Boat",
    "D",
    "E"
  }, self.ActivateTutorial, {self, true})
end

function SetupCancellationCriteria(self)
  self:_CreateEvent(Event.ObjectInSeat, {
    Player.GetLocalCharacter(),
    "Boat",
    "D",
    "X"
  }, self.EndTutorial, {self, false})
end
