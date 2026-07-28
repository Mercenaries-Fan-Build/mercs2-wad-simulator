import("MrxUtil")
tEvents = tEvents or {}
tTemplates = {
  China = "Chinese Airborne",
  Allied = "Allied Airborne"
}

function OnActivate(uGuid, iArg)
  Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, Start, {uGuid, iArg})
end

function Start(uGuid, iArg)
  Event.Create(Event.ObjectHealth, {
    uGuid,
    "<",
    "100"
  }, RemoveChute, {uGuid, iArg})
end

function RemoveChute(uGuid, iArg)
  local sFaction = MrxUtil.GetFaction(uGuid)
  x, y, z = Object.GetPosition(uGuid)
  yaw = Object.GetYaw(uGuid)
  Object.Remove(uGuid, 0.25)
  Pg.Spawn(tTemplates[sFaction], x, y, z, yaw, true, true)
  Object.SetYaw(uGuid, yaw)
end
