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
  Event.Create(Event.TimerRelative, {3}, Salvo, {uGuid})
end

function Salvo(uGuid)
  Debug.Printf("SALVO")
  local nSpawnX, nSpawnY, nSpawnZ = Object.GetPosition(Player.GetLocalCharacter())
  if not nSpawnX or not Object.IsAwake(uGuid) then
    Debug.Printf("ERROR")
    return
  end
  local tTargets = Pg.FastCollectGroundVehicles(nSpawnX, nSpawnY, nSpawnZ, 200)
  for i, target in pairs(tTargets) do
    Debug.Printf(tostring(MrxUtil.GetFaction(target)))
    if target ~= uLastTarget and Object.IsAlive(target) and (Object.HasLabel(target, "VZ") or Object.HasLabel(target, "China") or Object.HasLabel(target, "Guerilla")) then
      uTarget = target
      break
    end
  end
  uLastTarget = uTarget
  if uTarget then
    for i = 1, 4 do
      Event.Create(Event.TimerRelative, {
        0.25 * i
      }, LaunchMissile, {uGuid, uTarget})
    end
  else
    Debug.Printf("No valid target found!")
  end
  Event.Create(Event.TimerRelative, {3}, Salvo, {uGuid, uTarget})
end

function LaunchMissile(uGuid, uTarget)
  if not uTarget then
    Debug.Printf("LaunchMissile ERROR")
    return
  end
  Debug.Printf("*** Fire in the hole! ***")
  local nSpawnX, nSpawnY, nSpawnZ = Object.GetPosition(uGuid)
  local nTargetX, nTargetY, nTargetZ = Object.GetPosition(uTarget)
  if not nTargetX or not nSpawnX then
    Debug.Printf("ERROR with target position")
    return
  end
  nTargetX = nTargetX + math.randi(5) - math.randi(5)
  nTargetZ = nTargetZ + math.randi(5) - math.randi(5)
  local nVectorX, nVectorY, nVectorZ = nTargetX - nSpawnX, nTargetY - nSpawnY, nTargetZ - nSpawnZ
  nVectorX, nVectorY, nVectorZ = Math.Normalize(nVectorX, nVectorY, nVectorZ)
  Sound.CueSound(uGuid, "wpn_tankgun_fire_npc")
  Pg.Spawn("global_particle_muzzleflash_tank", nSpawnX, nSpawnY, nSpawnZ)
  local nSpeedScale = 100
  Airstrike.SpawnOrdnance("Gunship Shell", nSpawnX, nSpawnY, nSpawnZ, nVectorX * nSpeedScale, nVectorY * nSpeedScale, nVectorZ * nSpeedScale, "impact", 1)
end
