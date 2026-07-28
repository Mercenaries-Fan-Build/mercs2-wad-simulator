uEvent = nil
uFilter = nil

function Init()
  uEvent = {}
  uFilter = ObjectFilter.Create()
  ObjectFilter.SetFilter(uFilter, "human")
end

function Deinit()
  uEvent = nil
  uFilter = nil
end

function OnActivate(uGuid, nArg)
  uEvent[uGuid] = Event.Create(Event.ObjectProximity, {
    uFilter,
    uGuid,
    "<",
    6,
    false,
    false
  }, Triggered, {uGuid})
end

function OnDeactivate(uGuid)
  if uEvent[uGuid] then
    Event.Delete(uEvent[uGuid])
    uEvent[uGuid] = nil
  end
end

function Triggered(uGuid, tListOfObjects)
  Event.Create(Event.TimerRelative, {0.001}, Popup, {uGuid})
end

function Popup(uGuid)
  local nX, nY, nZ = Object.GetPosition(uGuid)
  if not nX then
    return
  end
  Object.Remove(uGuid)
  Airstrike.SpawnOrdnance("Grenade MG Projectile", nX, nY, nZ, 0, 8, 0, "distance", 1.8, nil, Object.Kill, {})
end
