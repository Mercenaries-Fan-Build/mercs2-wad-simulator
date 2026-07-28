import("MrxTutorialManager")
tEvents = tEvents or {}

function OnActivate(uGuid, iArg)
  tEvents[uGuid] = tEvents[uGuid] or {}
  tEvents[uGuid].uActivate = Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, function()
    bLightStart = Vehicle.SetParts(uGuid, "LightFront", false)
    SetupActivationEvents(uGuid)
  end)
end

function OnDeactivate(uGuid)
  tEvents = tEvents or {}
  tEvents[uGuid] = tEvents[uGuid] or {}
  for i, event in pairs(tEvents[uGuid]) do
    Event.Delete(event)
  end
  tEvents[uGuid] = nil
end

function OnDeath(uGuid)
  Vehicle.SetParts(uGuid, "LightFront", false)
  OnDeactivate(uGuid)
end

function SetupActivationEvents(uGuid)
  bLightStart = Vehicle.SetParts(uGuid, "LightFront", true)
end
