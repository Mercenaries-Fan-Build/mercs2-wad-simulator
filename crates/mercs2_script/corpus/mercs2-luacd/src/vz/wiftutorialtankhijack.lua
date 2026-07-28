inherit("MrxTutorial")
import("MrxFactionManager")

function GetMessage()
  return "[Tutorial.TankHijack]"
end

function SetupActivationCriteria(self)
  local uPlayer = Player.GetLocalCharacter()
  self:_CreateEvent(Event.ObjectProximity, {
    "tank",
    uPlayer,
    "<",
    20,
    false,
    false
  }, self.ActivateTutorial2, {self})
end

function ActivateTutorial2(self, tGuids)
  if type(tGuids) == "table" then
    for i, uVehicleObject in pairs(tGuids) do
      local uGunners = Vehicle.GetRiders(uVehicleObject, "g")
      local nGunners
      local sFaction = MrxFactionManager.GetFactionStringAbbrev(uVehicleObject)
      local sHeroFaction = MrxFactionManager.GetFactionStringAbbrev(Player.GetLocalCharacter())
      local sFactionAttitude = MrxFactionManager.GetAttitudeLabel(sHeroFaction, sFaction)
      Debug.Printf("TANK FACTION ATTITUDE! thingy:" .. sFactionAttitude)
      if sFactionAttitude == "Hostile" then
      end
      if type(uGunners) == "table" then
        nGunners = table.getn(uGunners)
      end
      if nGunners and 1 < nGunners then
      else
        return
      end
    end
  end
  self:ActivateTutorial()
end

function EndTutorial(self, bComplete)
  if bComplete == true then
    bComplete = false
  end
  MrxTutorial.EndTutorial(self, bComplete)
end
