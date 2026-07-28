function OnActivate(guid, args)
  local uRiders = Vehicle.GetRiders(guid, "driver") or {}
  
  if uRiders[1] == nil then
    Vehicle.OpenDoor(guid, "DriverHatch")
  end
end
