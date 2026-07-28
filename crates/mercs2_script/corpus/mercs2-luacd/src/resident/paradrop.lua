inherit("OrientedBlippable")
import("MrxUtil")
tEvents = tEvents or {}
tTemplates = {
  China = "Chinese Paratrooper",
  Allied = "Allied Paratrooper"
}
sTexture = "temp_radar_icon_airplane"
nSize = 5
tColorAlly = {
  0,
  127,
  255
}
tColorNeutral = {
  200,
  200,
  200
}
tColorEnemy = {
  255,
  0,
  0
}

function OnActivate(uGuid)
  Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, Start, {uGuid})
end

function Start(uGuid)
  local oPrototype = getfenv()
  local oInstance = oPrototype:Create(uGuid, uRuntimeOwner)
  local sFaction = MrxUtil.GetFaction(uGuid)
  if sFaction then
    nRelation = Ai.GetRelation(Pg.GetGuidByName(sFaction), Pg.GetGuidByName("PMC"))
  else
    nRelation = 0
  end
  if Object.HasLabel(uGuid, "PMC") then
    Object.SetUnkillable(uGuid, true, "Support")
  end
  if nRelation < 60 and nRelation > -60 then
    oInstance.tColor = tColorNeutral
  elseif nRelation <= -60 then
    oInstance.tColor = tColorEnemy
  elseif nRelation >= 60 then
    oInstance.tColor = tColorAlly
  end
  OrientedBlippable.SetBlipped(oInstance)
  oInstance:SetBlipped()
  for i = 1, 16 do
    Event.Create(Event.TimerRelative, {
      5.25 + i * 0.75
    }, DropDude, {uGuid, iArg})
  end
end

function DropDude(uGuid, iArg)
  if Object.IsAlive(uGuid) then
    local sFaction = MrxUtil.GetFaction(uGuid)
    x, y, z = Object.GetPosition(uGuid)
    yaw = Object.GetYaw(uGuid)
    Pg.Spawn(tTemplates[sFaction], x + math.randi(10) - math.randi(10), y, z + math.randi(10) - math.randi(10), yaw, true, true)
    Object.SetYaw(uGuid, yaw)
  end
end
