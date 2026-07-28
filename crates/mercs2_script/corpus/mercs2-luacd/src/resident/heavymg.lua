import("MrxUtil")
tEvents = tEvents or {}

function OnActivate(uGuid, iArg)
  tEvents[uGuid] = Event.Create(Event.WeaponEvent, {
    "Human",
    "Drop",
    uGuid
  }, Object.Remove, {uGuid})
end

function OnDeactivate(uGuid)
  if tEvents[uGuid] then
    Event.Delete(tEvents[uGuid])
  end
end
