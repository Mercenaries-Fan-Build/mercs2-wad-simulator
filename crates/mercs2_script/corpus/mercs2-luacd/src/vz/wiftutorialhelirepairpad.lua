inherit("MrxTutorial")

function GetMessage()
  return "[Tutorial.LandingZoneHealth]"
end

function ActivateTutorial(self)
  local uRider = Player.GetLocalCharacter()
  local uVehicle = Vehicle.GetFromRider(uRider)
  if not uVehicle then
    return
  elseif not Object.HasLabel(uVehicle, "Helicopter") then
    return
  end
  MrxTutorial.ActivateTutorial(self, true)
end

function SetupCompletionCriteria(self)
  self:_CreateEvent(Event.TimerRelative, {10}, EndTutorial, {self, true})
end
