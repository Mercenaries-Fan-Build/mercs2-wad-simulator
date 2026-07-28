inherit("Blippable")
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

function Create(oPrototype, uGuid, iArg)
  local oInstance = GetFromGuid(uGuid)
  oInstance = oInstance or Blippable.Create(oPrototype, uGuid, iArg)
  if not oInstance.DriverEnter then
    oInstance.DriverEnter = Event.CreatePersistent(Event.ObjectInSeat, {
      "human",
      uGuid,
      "Driver",
      "enter"
    }, oInstance.PickColor, {oInstance, uGuid})
  end
  if not oInstance.DriverExit then
    oInstance.DriverExit = Event.CreatePersistent(Event.ObjectInSeat, {
      "human",
      uGuid,
      "Driver",
      "exit"
    }, function()
      oInstance:ClearBlipped(true)
    end)
  end
  local sFaction = MrxUtil.GetFaction(uGuid)
  local sFactionAbbrev = MrxFactionManager.GetFactionAbbrev(sFaction)
  if not oInstance.Attitude then
    Debug.Printf("Creating MOOD event for" .. tostring(sFactionAbbrev))
    oInstance.Attitude = MrxFactionManager.CreatePersistentAttitudeChangeEvent({sFactionAbbrev, "Pmc"}, function()
      oInstance.ClearBlipped(oInstance, uGuid)
      oInstance.PickColor(oInstance, uGuid)
      oInstance.SetBlipped(oInstance, uGuid)
    end, {})
  end
  local uDriver = Vehicle.GetDriver(uGuid)
  if uDriver then
    oInstance:PickColor(uGuid)
  end
  return oInstance
end

function Delete(self)
  if self.DriverEnter then
    Event.Delete(self.DriverEnter)
    self.DriverEnter = nil
  end
  if self.Attitude then
    Event.Delete(self.Attitude)
    self.Attitude = nil
  end
  if self.DriverExit then
    Event.Delete(self.DriverExit)
    self.DriverExit = nil
  end
  Blippable.Delete(self)
end

function PickColor(self, uGuid)
  uRider = Vehicle.GetDriver(uGuid)
  if uRider and Object.IsPlayerControlled(uRider) then
    self:ClearBlipped()
    return
  end
  if uRider then
    nRelation = Ai.GetRelation(uRider, Pg.GetGuidByName("PMC"))
    Debug.Printf("VehicleBlippable: Checking driver relation...")
    if Object.HasLabel(uGuid, "pmc") or Object.HasLabel(uRider, "pmc") then
      self.tColor = self.tColorPmc
      if self.tMarker then
        self.tMarker.tColor = self.tColorPmc
      end
    elseif nRelation < 60 and nRelation > -60 then
      self.tColor = self.tColorNeutral
      if self.tMarker then
        self.tMarker.tColor = self.tColorNeutral
      end
    elseif nRelation <= -60 then
      self.bHostile = true
      self.tColor = self.tColorEnemy
      if self.tMarker then
        self.tMarker.tColor = self.tColorEnemy
      end
    elseif nRelation >= 60 then
      self.tColor = self.tColorAlly
      if self.tMarker then
        self.tMarker.tColor = self.tColorAlly
      end
    end
  else
    Debug.Printf("VehicleBlippable: NO DRIVER FOUND")
    self.tColor = self.tColorEmpty
  end
  if self.tColor then
    self:SetBlipped(true)
  end
end
