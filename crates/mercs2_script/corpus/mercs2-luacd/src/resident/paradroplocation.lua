import("MrxUtil")
tEvents = tEvents or {}
tTemplates = {
  China = "Support Vehicle (paradop_ch)",
  Allied = "Support Vehicle (paradop_al)"
}

function OnActivate(uGuid)
  Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, Start, {uGuid})
end

function Start(uGuid)
  local x, y, z = Object.GetPosition(uGuid)
  local sFaction = MrxUtil.GetFaction(uGuid)
  if x and sFaction and tTemplates[sFaction] then
    Airstrike.Flyby(tTemplates[sFaction], x - 50, z + 300, x, z, y + 100, 40)
  else
    return
  end
  Object.Remove(uGuid)
end
