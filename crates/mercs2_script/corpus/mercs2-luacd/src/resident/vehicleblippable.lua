inherit("OrientedBlippable")
import("MrxUtil")
import("MrxFactionManager")
tColorAlly = {
  0,
  127,
  255
}
tColorNeutral = {
  230,
  230,
  255
}
tColorEnemy = {
  255,
  0,
  0
}
tColorEmpty = {
  100,
  100,
  100
}
tColorPmc = {
  0,
  255,
  0
}

function OnActivate(uGuid, uRuntimeOwner, iArg)
  Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, Start, {
    uGuid,
    uRuntimeOwner,
    iArg
  })
end

function Start(uGuid, uRuntimeOwner, iArg)
  local health = Object.GetHealth(uGuid)
  Debug.Printf(" *=* ", health, Object.IsAlive(uGuid))
  if type(health) == "number" and 0 < health then
    local oPrototype = getfenv()
    local self = oPrototype:Create(uGuid, uRuntimeOwner)
  end
end

function Create(oPrototype, uGuid, uRuntimeOwner)
  Debug.Printf("VehicleBlippable Create")
  local self = OrientedBlippable.Create(oPrototype, uGuid, uRuntimeOwner)
  if not self.DriverEnter then
    self.DriverEnter = Event.CreatePersistent(Event.ObjectInSeat, {
      "Human",
      uGuid,
      "Driver",
      "enter"
    }, self.SetBlipped, {self, uGuid})
  end
  if not self.DriverExit then
    self.DriverExit = Event.CreatePersistent(Event.ObjectInSeat, {
      "Human",
      uGuid,
      "Driver",
      "exit"
    }, self.SetBlipped, {self, uGuid})
  end
  local sFaction = MrxUtil.GetFaction(uGuid)
  local sFactionAbbrev = MrxFactionManager.GetFactionAbbrev(sFaction)
  if not self.Attitude then
    Debug.Printf("Creating MOOD event for" .. tostring(sFactionAbbrev))
    self.Attitude = MrxFactionManager.CreatePersistentAttitudeChangeEvent({sFactionAbbrev, "Pmc"}, self.SetBlipped, {self, uGuid})
  end
  self:SetBlipped(uGuid)
  return self
end

function Delete(self)
  Event.Delete(self.DriverEnter)
  Event.Delete(self.DriverExit)
  Event.Delete(self.Attitude)
  OrientedBlippable.Delete(self)
end

function SetBlipped(oSelf, uVehicle)
  Debug.Printf("VehicleBlippable.SetBlipped")
  uRider = Vehicle.GetDriver(uVehicle)
  if uRider and Object.IsPlayerControlled(uRider) then
    oSelf:ClearBlipped()
    return
  end
  if uRider then
    nRelation = Ai.GetRelation(uRider, Pg.GetGuidByName("PMC"))
    if Object.HasLabel(uVehicle, "pmc") or Object.HasLabel(uRider, "pmc") then
      oSelf.tColor = oSelf.tColorPmc
    elseif nRelation < 60 and nRelation > -60 then
      oSelf.tColor = oSelf.tColorNeutral
    elseif nRelation <= -60 then
      oSelf.tColor = oSelf.tColorEnemy
    elseif nRelation >= 60 then
      oSelf.tColor = oSelf.tColorAlly
    end
  else
    Debug.Printf("VehicleBlippable: NO DRIVER FOUND")
    oSelf.tColor = oSelf.tColorEmpty
  end
  OrientedBlippable.SetBlipped(oSelf)
end
