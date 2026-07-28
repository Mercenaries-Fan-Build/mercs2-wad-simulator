inherit("MrxTutorial")
import("MrxTutorialManager")
import("MrxFactionManager")

function GetMessage()
  return "[Tutorial.GateHonk]"
end

function SetupActivationCriteria(self)
  local uGateGuid = Sys.StringToGuid("0x000f9a64")
  uEvent = self:_CreatePersistentEvent(Event.ObjectProximity, {
    uGateGuid,
    Player.GetLocalCharacter(),
    "<",
    20,
    false,
    false
  }, ShowMessage, {self})
end

function ShowMessage(self)
  local bDisguiseState = Player.GetVehicleDisguiseState({
    Player = Player.GetLocalCharacter()
  })
  if tostring(bDisguiseState) == "true" then
    ActivateTutorial(self, true)
  end
end
