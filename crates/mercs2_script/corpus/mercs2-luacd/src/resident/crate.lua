local sTexture = "pickup_crate_2"
local tGuids = {}

function OnActivate(uGuid)
  tGuids[uGuid] = tGuids[uGuid] or {}
  tGuids[uGuid].Awake = Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, Awake, {uGuid})
end

function Awake(uGuid)
  tGuids[uGuid] = tGuids[uGuid] or {}
  if not tGuids[uGuid].Winched then
    tGuids[uGuid].Winched = Event.CreatePersistent(Event.ObjectWinched, {
      uGuid,
      0,
      "any"
    }, Awake, {uGuid})
  end
  if Object.IsWinched(uGuid) then
    if tGuids[uGuid].Marker then
      Marker.Remove(tGuids[uGuid].Marker)
      tGuids[uGuid].Marker = nil
    end
  else
    tGuids[uGuid].Marker = Marker.AddBlip(uGuid, sTexture, 48, 255, 255, 255, 255, 0.5, 16, 20)
  end
end

function OnDeactivate(uGuid)
  tGuids[uGuid] = tGuids[uGuid] or {}
  if tGuids[uGuid].Marker then
    Marker.Remove(tGuids[uGuid].Marker)
    tGuids[uGuid].Marker = nil
  end
  for type, event in pairs(tGuids[uGuid]) do
    Event.Delete(event)
  end
  tGuids[uGuid] = nil
end

function OnDeath(uGuid)
  OnDeactivate(uGuid)
end
