function Init()
  tEvents = tEvents or {}
end

function OnActivate(uGuid, args)
  Object.PlayMaterialAnimation(uGuid, "global_weapon_beacon", true)
  tEvents[uGuid] = Event.Create(Event.TimerRelative, {1}, Sound.CueSound, {
    uGuid,
    "wpn_bomb_timer_01_armed"
  })
end

function OnDeactivate(uGuid, args)
  Object.StopMaterialAnimation(uGuid, "global_weapon_beacon")
  Sound.StopSound(uGuid, "wpn_bomb_timer_01_armed")
  Event.Delete(tEvents[uGuid])
  tEvents[uGuid] = nil
end
